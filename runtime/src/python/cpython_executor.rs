//! CPython Node Executor (Phase 1.10)
//!
//! This module provides in-process execution of Python SDK nodes using CPython via PyO3.
//! Unlike RustPython (which is embedded), this executor uses the system Python interpreter
//! directly through FFI, providing:
//! - Full Python stdlib and PyPI ecosystem access
//! - Zero-copy numpy arrays via rust-numpy
//! - Microsecond FFI call latency
//! - Native C-extension support (pandas, torch, transformers, etc.)
//!
//! Architecture:
//! - Reuses existing PyO3 FFI infrastructure from ffi.rs
//! - Leverages marshal.rs + numpy_marshal.rs for data conversion
//! - Loads Python SDK nodes from remotemedia.nodes module
//! - Manages GIL-protected node instances

use crate::{Error, Result};
use crate::nodes::{NodeContext, NodeExecutor};
use crate::python::marshal::{json_to_python, python_to_json};
use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

/// CPython-based node executor
///
/// Executes Python SDK nodes in the system Python interpreter (CPython)
/// using PyO3 FFI bindings. Provides full compatibility with the Python
/// ecosystem including C-extensions.
pub struct CPythonNodeExecutor {
    /// Node type (class name, e.g., "AudioTransform")
    node_type: String,

    /// Python node instance (holds Py<PyAny> which is Send + Sync)
    /// None until initialized
    instance: Option<Py<PyAny>>,

    /// Whether the node has been initialized
    initialized: bool,
}

impl CPythonNodeExecutor {
    /// Create a new CPython node executor
    ///
    /// The node is not loaded until initialize() is called.
    pub fn new(node_type: impl Into<String>) -> Self {
        Self {
            node_type: node_type.into(),
            instance: None,
            initialized: false,
        }
    }

    /// Load the node class from the Python SDK
    ///
    /// Phase 1.10.2: Import remotemedia.nodes and get the class
    fn load_class<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // Import the remotemedia.nodes module
        let nodes_module = py.import("remotemedia.nodes")?;

        // Get the node class by name
        let node_class = nodes_module.getattr(self.node_type.as_str())?;

        Ok(node_class)
    }

    /// Instantiate the node with parameters
    ///
    /// Phase 1.10.3: Call class(**params) via PyO3
    fn instantiate_node<'py>(
        &self,
        py: Python<'py>,
        class: &Bound<'py, PyAny>,
        params: &Value,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Convert JSON params to Python dict
        let py_params = json_to_python(py, params)?;

        // If params is a dict, unpack it as **kwargs
        // Otherwise, create an empty instance
        let instance = if params.is_object() {
            let kwargs = py_params.downcast::<PyDict>()?;
            class.call((), Some(kwargs))?
        } else {
            // No params or non-dict params, call with no arguments
            class.call0()?
        };

        tracing::info!(
            "Instantiated CPython node: {} with params: {:?}",
            self.node_type,
            params
        );

        Ok(instance)
    }

    /// Call the initialize() method if it exists
    fn call_initialize(&self, _py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        // Check if the node has an initialize method
        if instance.hasattr("initialize")? {
            tracing::debug!("Calling initialize() on CPython node: {}", self.node_type);
            instance.call_method0("initialize")?;
            tracing::info!("CPython node {} initialized", self.node_type);
        } else {
            tracing::debug!("CPython node {} has no initialize() method", self.node_type);
        }
        Ok(())
    }

    /// Call the cleanup() method if it exists
    fn call_cleanup(&self, _py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        // Check if the node has a cleanup method
        if instance.hasattr("cleanup")? {
            tracing::debug!("Calling cleanup() on CPython node: {}", self.node_type);
            instance.call_method0("cleanup")?;
            tracing::info!("CPython node {} cleaned up", self.node_type);
        } else {
            tracing::debug!("CPython node {} has no cleanup() method", self.node_type);
        }
        Ok(())
    }

    /// Check if a Python object is an async generator
    fn is_async_generator(&self, py: Python, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        // Check if the object is an async generator by checking its type
        let inspect = py.import("inspect")?;
        let is_async_gen = inspect.call_method1("isasyncgen", (obj,))?;
        is_async_gen.extract::<bool>()
    }

    /// Check if the node's process method is an async generator function
    fn process_is_async_gen_function(&self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<bool> {
        let inspect = py.import("inspect")?;

        // Get the process method
        let process_method = instance.getattr("process")?;

        // Check if it's an async generator function
        let is_async_gen_func = inspect.call_method1("isasyncgenfunction", (process_method,))?;
        is_async_gen_func.extract::<bool>()
    }

    /// Wrap input data as an async generator for streaming nodes
    fn wrap_input_as_async_generator<'py>(&self, py: Python<'py>, input: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        // Create an async generator that yields the input data once
        let wrapper_code = std::ffi::CString::new(r#"
async def _input_wrapper(data):
    """Wrap single input as async generator that yields once."""
    yield data

_wrapped = _input_wrapper
"#).unwrap();

        py.run(&wrapper_code, None, None)?;

        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let wrapper_fn = locals.get_item("_wrapped")?;

        // Call the wrapper function to create the generator
        let async_gen = wrapper_fn.call1((input,))?;

        Ok(async_gen)
    }

    /// Check if a Python object is a coroutine
    fn is_coroutine(&self, py: Python, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let inspect = py.import("inspect")?;
        let is_coro = inspect.call_method1("iscoroutine", (obj,))?;
        is_coro.extract::<bool>()
    }

    /// Await a Python coroutine using asyncio
    fn await_coroutine<'py>(&self, py: Python<'py>, coro: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        // We need to run the coroutine in an event loop
        // Since we're in a sync context (with_gil), we need to use asyncio.run()
        let asyncio = py.import("asyncio")?;

        // Try to get the running loop first
        let result = asyncio.call_method1("run", (coro,));

        match result {
            Ok(res) => Ok(res),
            Err(e) => {
                tracing::warn!("Failed to await coroutine with asyncio.run: {}", e);
                // If asyncio.run fails (e.g., loop already running), try get_event_loop().run_until_complete()
                let loop_result = asyncio.call_method0("get_event_loop");
                if let Ok(event_loop) = loop_result {
                    event_loop.call_method1("run_until_complete", (coro,))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Collect all values from an async generator
    fn collect_async_generator(&self, py: Python, async_gen: &Bound<'_, PyAny>) -> PyResult<Value> {
        tracing::debug!("Collecting results from async generator");

        // We need to iterate through the async generator
        // This requires running it in an asyncio event loop
        let code = std::ffi::CString::new(r#"
async def _collect_async_gen(agen):
    """Collect all items from an async generator."""
    results = []
    async for item in agen:
        results.append(item)
    return results
"#).unwrap();

        // Execute the helper function
        py.run(&code, None, None)?;

        // Get the helper function
        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let collect_fn = locals.get_item("_collect_async_gen")?;

        // Call the helper function with the async generator
        let coro = collect_fn.call1((async_gen,))?;

        // Await the coroutine to get results
        let results = self.await_coroutine(py, &coro)?;

        // Convert Python list to JSON array
        let json_results = python_to_json(py, &results)?;

        tracing::debug!("Collected {} results from async generator",
            json_results.as_array().map(|a| a.len()).unwrap_or(0));

        Ok(json_results)
    }
}

#[async_trait]
impl NodeExecutor for CPythonNodeExecutor {
    /// Initialize the CPython node
    ///
    /// Phase 1.10.2 + 1.10.3: Load class and instantiate with parameters
    async fn initialize(&mut self, context: &NodeContext) -> Result<()> {
        if self.initialized {
            tracing::warn!("CPython node {} already initialized", self.node_type);
            return Ok(());
        }

        tracing::info!(
            "Initializing CPython node: {} (id: {})",
            self.node_type,
            context.node_id
        );

        // Acquire GIL and create node instance
        let instance = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            // Load the class
            let class = self.load_class(py)?;

            // Instantiate with parameters
            let instance = self.instantiate_node(py, &class, &context.params)?;

            // Call initialize() if it exists
            self.call_initialize(py, &instance)?;

            // Convert to Py<PyAny> to store across GIL releases
            Ok(instance.unbind())
        })
        .map_err(|e: PyErr| {
            Error::Execution(format!(
                "Failed to initialize CPython node {}: {}",
                self.node_type, e
            ))
        })?;

        self.instance = Some(instance);
        self.initialized = true;

        Ok(())
    }

    /// Process data through the CPython node
    ///
    /// Phase 1.10.4: Call node.process(data) and marshal results
    /// Phase 1.10.10: Support async generators for streaming nodes
    async fn process(&mut self, input: Value) -> Result<Option<Value>> {
        if !self.initialized {
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        let instance = self.instance.as_ref().ok_or_else(|| {
            Error::Execution(format!("CPython node {} has no instance", self.node_type))
        })?;

        tracing::debug!("Processing data through CPython node: {}", self.node_type);

        // Acquire GIL and process data
        let result = Python::with_gil(|py| -> PyResult<Option<Value>> {
            // Get the bound instance
            let bound_instance = instance.bind(py);

            // Convert input JSON to Python
            let py_input = json_to_python(py, &input)?;

            // Check if this node's process method is an async generator function
            // (streaming node that expects async generator input)
            let is_streaming_node = self.process_is_async_gen_function(py, &bound_instance)?;

            // Prepare the input - wrap as async generator if needed
            let prepared_input = if is_streaming_node {
                tracing::debug!("Node {} is a streaming node, wrapping input as async generator", self.node_type);
                self.wrap_input_as_async_generator(py, &py_input)?
            } else {
                py_input
            };

            // Call process(data)
            let py_result = bound_instance.call_method1("process", (prepared_input,))?;

            // Check if result is None (filtered out)
            if py_result.is_none() {
                return Ok(None);
            }

            // Check if the result is an async generator (coroutine)
            // For streaming nodes, process() returns an async generator
            if self.is_async_generator(py, &py_result)? {
                tracing::debug!("CPython node {} returned async generator (streaming node)", self.node_type);

                // Collect all results from the async generator
                let collected_results = self.collect_async_generator(py, &py_result)?;

                return Ok(Some(collected_results));
            }

            // Check if the result is a coroutine (async function result)
            if self.is_coroutine(py, &py_result)? {
                tracing::debug!("CPython node {} returned coroutine, awaiting it", self.node_type);

                // We need to await the coroutine
                let awaited_result = self.await_coroutine(py, &py_result)?;

                // Check if the awaited result is an async generator
                if self.is_async_generator(py, &awaited_result)? {
                    tracing::debug!("Coroutine resolved to async generator");
                    let collected_results = self.collect_async_generator(py, &awaited_result)?;
                    return Ok(Some(collected_results));
                }

                // Convert awaited result to JSON
                let json_result = python_to_json(py, &awaited_result)?;
                return Ok(Some(json_result));
            }

            // Convert Python result back to JSON (synchronous result)
            let json_result = python_to_json(py, &py_result)?;

            Ok(Some(json_result))
        })
        .map_err(|e: PyErr| {
            Error::Execution(format!(
                "Failed to process data in CPython node {}: {}",
                self.node_type, e
            ))
        })?;

        Ok(result)
    }

    /// Cleanup the CPython node
    async fn cleanup(&mut self) -> Result<()> {
        if !self.initialized {
            tracing::debug!("CPython node {} not initialized, skipping cleanup", self.node_type);
            return Ok(());
        }

        if let Some(instance) = &self.instance {
            Python::with_gil(|py| -> PyResult<()> {
                let bound_instance = instance.bind(py);
                self.call_cleanup(py, bound_instance)?;
                Ok(())
            })
            .map_err(|e: PyErr| {
                Error::Execution(format!(
                    "Failed to cleanup CPython node {}: {}",
                    self.node_type, e
                ))
            })?;
        }

        self.instance = None;
        self.initialized = false;

        tracing::info!("CPython node {} cleanup complete", self.node_type);
        Ok(())
    }

    fn info(&self) -> crate::nodes::NodeInfo {
        crate::nodes::NodeInfo {
            name: format!("CPython({})", self.node_type),
            version: "1.0.0".to_string(),
            description: Some(format!(
                "CPython-based executor for {} node (full Python ecosystem support)",
                self.node_type
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::ffi::CString;

    #[tokio::test]
    async fn test_cpython_executor_creation() {
        let executor = CPythonNodeExecutor::new("PassThroughNode");
        assert_eq!(executor.node_type, "PassThroughNode");
        assert!(!executor.initialized);
        assert!(executor.instance.is_none());
    }

    #[tokio::test]
    async fn test_cpython_executor_lifecycle() {
        pyo3::prepare_freethreaded_python();

        // Create a simple test node in Python
        Python::with_gil(|py| {
            let code = CString::new(r#"
class TestNodeLifecycle:
    def __init__(self, multiplier=1):
        self.multiplier = multiplier
        self.count = 0

    def initialize(self):
        self.count = 0

    def process(self, data):
        self.count += 1
        return data * self.multiplier

    def cleanup(self):
        pass
"#).unwrap();
            py.run(&code, None, None).unwrap();

            // Register it in remotemedia.nodes (mock)
            let sys = py.import("sys").unwrap();
            let modules = sys.getattr("modules").unwrap();

            // Create or get existing remotemedia.nodes module
            let mock_code = CString::new(
                "import types, sys; mock_module = sys.modules.get('remotemedia.nodes') or types.ModuleType('remotemedia.nodes'); mock_module.TestNodeLifecycle = TestNodeLifecycle; sys.modules['remotemedia.nodes'] = mock_module"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();
        });

        let mut executor = CPythonNodeExecutor::new("TestNodeLifecycle");

        // Initialize
        let context = NodeContext {
            node_id: "test_0".to_string(),
            node_type: "TestNodeLifecycle".to_string(),
            params: serde_json::json!({ "multiplier": 3 }),
            session_id: None,
            metadata: HashMap::new(),
        };

        executor.initialize(&context).await.unwrap();
        assert!(executor.initialized);
        assert!(executor.instance.is_some());

        // Process data
        let result = executor.process(serde_json::json!(5)).await.unwrap();
        assert_eq!(result, Some(serde_json::json!(15)));

        let result2 = executor.process(serde_json::json!(10)).await.unwrap();
        assert_eq!(result2, Some(serde_json::json!(30)));

        // Cleanup
        executor.cleanup().await.unwrap();
        assert!(!executor.initialized);
        assert!(executor.instance.is_none());
    }

    #[tokio::test]
    async fn test_cpython_executor_without_optional_methods() {
        pyo3::prepare_freethreaded_python();

        // Create a minimal node without initialize/cleanup
        Python::with_gil(|py| {
            let code = CString::new(r#"
class MinimalNode:
    def process(self, data):
        return {"result": data}
"#).unwrap();
            py.run(&code, None, None).unwrap();

            let sys = py.import("sys").unwrap();
            let modules = sys.getattr("modules").unwrap();

            let mock_code = CString::new(
                "import types; mock_module = types.ModuleType('remotemedia.nodes'); mock_module.MinimalNode = MinimalNode"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();

            let mock_module = py.eval(&CString::new("mock_module").unwrap(), None, None).unwrap();
            modules.set_item("remotemedia.nodes", mock_module).unwrap();
        });

        let mut executor = CPythonNodeExecutor::new("MinimalNode");

        let context = NodeContext {
            node_id: "minimal_0".to_string(),
            node_type: "MinimalNode".to_string(),
            params: serde_json::json!({}),
            session_id: None,
            metadata: HashMap::new(),
        };

        executor.initialize(&context).await.unwrap();

        let result = executor.process(serde_json::json!("test")).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()["result"], "test");

        executor.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cpython_executor_async_generator() {
        pyo3::prepare_freethreaded_python();

        // Create a streaming node with async generator
        Python::with_gil(|py| {
            let code = CString::new(r#"
class StreamingNodeAsyncGen:
    def __init__(self):
        self.count = 0

    async def process(self, data):
        """Async generator that yields multiple results."""
        for i in range(3):
            yield {"index": i, "data": data, "multiplier": i * 2}
"#).unwrap();
            py.run(&code, None, None).unwrap();

            let sys = py.import("sys").unwrap();
            let _modules = sys.getattr("modules").unwrap();

            let mock_code = CString::new(
                "import types, sys; mock_module = sys.modules.get('remotemedia.nodes') or types.ModuleType('remotemedia.nodes'); mock_module.StreamingNodeAsyncGen = StreamingNodeAsyncGen; sys.modules['remotemedia.nodes'] = mock_module"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();
        });

        let mut executor = CPythonNodeExecutor::new("StreamingNodeAsyncGen");

        let context = NodeContext {
            node_id: "streaming_0".to_string(),
            node_type: "StreamingNodeAsyncGen".to_string(),
            params: serde_json::json!({}),
            session_id: None,
            metadata: HashMap::new(),
        };

        executor.initialize(&context).await.unwrap();

        // Process data through async generator
        let result = executor.process(serde_json::json!("test_data")).await.unwrap();
        assert!(result.is_some());

        let results = result.unwrap();
        assert!(results.is_array());

        let results_array = results.as_array().unwrap();
        assert_eq!(results_array.len(), 3);

        // Verify each yielded item
        assert_eq!(results_array[0]["index"], 0);
        assert_eq!(results_array[0]["data"], "test_data");
        assert_eq!(results_array[1]["index"], 1);
        assert_eq!(results_array[2]["index"], 2);
        assert_eq!(results_array[2]["multiplier"], 4);

        executor.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cpython_executor_async_coroutine() {
        pyo3::prepare_freethreaded_python();

        // Create a node with async process method (coroutine, not generator)
        Python::with_gil(|py| {
            let code = CString::new(r#"
import asyncio

class AsyncNode:
    async def process(self, data):
        """Async function that returns a single result."""
        await asyncio.sleep(0)  # Simulate async work
        return {"result": data * 2, "async": True}
"#).unwrap();
            py.run(&code, None, None).unwrap();

            let sys = py.import("sys").unwrap();
            let modules = sys.getattr("modules").unwrap();

            let mock_code = CString::new(
                "import types; mock_module = types.ModuleType('remotemedia.nodes'); mock_module.AsyncNode = AsyncNode"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();

            let mock_module = py.eval(&CString::new("mock_module").unwrap(), None, None).unwrap();
            modules.set_item("remotemedia.nodes", mock_module).unwrap();
        });

        let mut executor = CPythonNodeExecutor::new("AsyncNode");

        let context = NodeContext {
            node_id: "async_0".to_string(),
            node_type: "AsyncNode".to_string(),
            params: serde_json::json!({}),
            session_id: None,
            metadata: HashMap::new(),
        };

        executor.initialize(&context).await.unwrap();

        let result = executor.process(serde_json::json!(5)).await.unwrap();
        assert!(result.is_some());

        let result_obj = result.unwrap();
        assert_eq!(result_obj["result"], 10);
        assert_eq!(result_obj["async"], true);

        executor.cleanup().await.unwrap();
    }
}
