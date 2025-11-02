//! CPython Node Executor
//!
//! Executes Python SDK nodes using CPython via PyO3.
//! Provides full Python stdlib and PyPI ecosystem access.

use crate::data::RuntimeData;
use crate::executor::PyObjectCache;
use crate::nodes::{NodeContext, NodeExecutor, NodeInfo};
use crate::python::marshal::{json_to_python, json_to_python_with_cache, python_to_json_with_cache};
use crate::python::{runtime_data_to_py, runtime_data_to_py_with_session, PyRuntimeData};
use crate::{Error, Result};
use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

mod cpython_async;
mod cpython_runtime_data;
mod cpython_streaming;

use cpython_async::{AsyncGenerator, EventLoopManager};
use cpython_runtime_data::extract_runtime_data;
use cpython_streaming::StreamingQueue;

/// CPython-based node executor
pub struct CPythonNodeExecutor {
    node_type: String,
    instance: Option<Py<PyAny>>,
    initialized: bool,
    py_cache: Option<PyObjectCache>,

    // Async/streaming support
    event_loop: EventLoopManager,
    active_generator: Option<AsyncGenerator>,
    streaming_queue: Option<StreamingQueue>,
    is_streaming: bool,
}

impl CPythonNodeExecutor {
    pub fn new(node_type: impl Into<String>) -> Self {
        Self {
            node_type: node_type.into(),
            instance: None,
            initialized: false,
            py_cache: None,
            event_loop: EventLoopManager::new(),
            active_generator: None,
            streaming_queue: None,
            is_streaming: false,
        }
    }

    pub fn new_with_cache(node_type: impl Into<String>, py_cache: PyObjectCache) -> Self {
        Self {
            node_type: node_type.into(),
            instance: None,
            initialized: false,
            py_cache: Some(py_cache),
            event_loop: EventLoopManager::new(),
            active_generator: None,
            streaming_queue: None,
            is_streaming: false,
        }
    }

    pub fn set_is_streaming_node(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
    }

    /// Load the node class from Python SDK
    fn load_class<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let nodes_module = py.import("remotemedia.nodes")?;
        nodes_module.getattr(self.node_type.as_str())
    }

    /// Instantiate the node with parameters
    fn instantiate_node<'py>(
        &self,
        py: Python<'py>,
        class: &Bound<'py, PyAny>,
        params: &Value,
    ) -> PyResult<Bound<'py, PyAny>> {
        let instance = if params.is_object() {
            let py_params = json_to_python(py, params)?;
            let kwargs = py_params.downcast::<PyDict>()?;
            class.call((), Some(kwargs))?
        } else {
            class.call0()?
        };

        tracing::info!(
            "Instantiated CPython node: {} with params: {:?}",
            self.node_type,
            params
        );

        Ok(instance)
    }

    /// Call initialize() method if it exists
    fn call_initialize(&mut self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        if !instance.hasattr("initialize")? {
            tracing::info!("CPython node {} has no initialize() method", self.node_type);
            return Ok(());
        }

        tracing::info!("Calling initialize() on CPython node: {}", self.node_type);

        let result = instance.call_method0("initialize")?;

        // Check if it's a coroutine (async method)
        let inspect = py.import("inspect")?;
        let is_coroutine = inspect
            .call_method1("iscoroutine", (&result,))?
            .extract::<bool>()?;

        if is_coroutine {
            // Use thread-isolated async initialization (PyTorch workaround)
            let event_loop = cpython_async::call_initialize_async(py, instance)?;
            self.event_loop = EventLoopManager::new();
            // Store the event loop for later use
            // (The event loop is stored internally in EventLoopManager)
        }

        tracing::info!("CPython node {} initialized", self.node_type);
        Ok(())
    }

    /// Call cleanup() method if it exists
    fn call_cleanup(&self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        if !instance.hasattr("cleanup")? {
            return Ok(());
        }

        tracing::info!("Calling cleanup() on CPython node: {}", self.node_type);
        instance.call_method0("cleanup")?;
        tracing::info!("CPython node {} cleaned up", self.node_type);

        Ok(())
    }

    /// Process RuntimeData through the CPython node
    pub async fn process_runtime_data(&mut self, input: RuntimeData) -> Result<RuntimeData> {
        tracing::info!("process_runtime_data called for node: {}", self.node_type);

        if !self.initialized {
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        // Create generator and get first result
        let (process_result_py, event_loop_py, get_next_fn_py) =
            Python::with_gil(|py| -> Result<(Py<PyAny>, Py<PyAny>, Py<PyAny>)> {
                let instance = self
                    .instance
                    .as_ref()
                    .ok_or_else(|| Error::Execution("Python node not initialized".to_string()))?
                    .bind(py);

                // Convert RuntimeData to Python
                let py_runtime_data_struct = runtime_data_to_py(input);
                let py_runtime_data = Py::new(py, py_runtime_data_struct)
                    .map_err(|e| Error::Execution(format!("Failed to create Python RuntimeData: {}", e)))?;

                // Create event loop
                let event_loop = self.event_loop.get_or_create(py)
                    .map_err(|e| Error::Execution(format!("Failed to create event loop: {}", e)))?;

                // Call process()
                let process_result = instance
                    .call_method1("process", (py_runtime_data,))
                    .map_err(|e| Error::Execution(format!("Failed to call process(): {}", e)))?;

                // Verify it's an async generator
                if !AsyncGenerator::is_async_generator(py, &process_result)
                    .map_err(|e| Error::Execution(format!("Failed to check async generator: {}", e)))?
                {
                    return Err(Error::Execution(
                        "Python node process() must return an async generator".to_string(),
                    ));
                }

                // Define helper function
                let code = std::ffi::CString::new(
                    r#"
def _run_with_existing_loop(agen, loop):
    import sys
    sys.stdout.flush()
    sys.stderr.flush()

    async def get_next():
        return await anext(agen)

    return loop.run_until_complete(get_next())
"#,
                )
                .unwrap();

                py.run(&code, None, None)
                    .map_err(|e| Error::Execution(format!("Failed to define helper: {}", e)))?;

                let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)
                    .map_err(|e| Error::Execution(format!("Failed to get locals: {}", e)))?;
                let get_next_fn = locals.get_item("_run_with_existing_loop")
                    .map_err(|e| Error::Execution(format!("Failed to get helper function: {}", e)))?;

                Ok((process_result.unbind(), event_loop.unbind(), get_next_fn.unbind()))
            })?;

        // Call helper function to get first item
        let anext_result_py = Python::with_gil(|py| -> Result<Py<PyAny>> {
            let process_result = process_result_py.bind(py);
            let event_loop = event_loop_py.bind(py);
            let get_next_fn = get_next_fn_py.bind(py);

            let anext_result = py.allow_threads(move || {
                Python::with_gil(|py| -> Result<Py<PyAny>> {
                    let pr = process_result_py.bind(py);
                    let el = event_loop_py.bind(py);
                    let func = get_next_fn_py.bind(py);

                    let result = func.call1((pr, el))
                        .map_err(|e| Error::Execution(format!("Failed to get first item: {}", e)))?;

                    Ok(result.unbind())
                })
            })?;

            Ok(anext_result)
        })?;

        // Extract RuntimeData from result
        Python::with_gil(|py| -> Result<RuntimeData> {
            let anext_result = anext_result_py.bind(py);
            extract_runtime_data(py, &anext_result)
        })
    }

    /// Process RuntimeData with streaming callback
    pub async fn process_runtime_data_streaming<F>(
        &mut self,
        input: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize>
    where
        F: FnMut(RuntimeData) -> Result<()> + Send,
    {
        tracing::info!(
            "process_runtime_data_streaming called for node: {} with session_id: {:?}",
            self.node_type,
            session_id
        );

        if !self.initialized {
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        // Create generator
        let (process_result_py, event_loop_py, get_next_fn_py) =
            Python::with_gil(|py| -> Result<(Py<PyAny>, Py<PyAny>, Py<PyAny>)> {
                let instance = self
                    .instance
                    .as_ref()
                    .ok_or_else(|| Error::Execution("Python node not initialized".to_string()))?
                    .bind(py);

                // Convert RuntimeData to Python with session_id
                let py_runtime_data_struct = runtime_data_to_py_with_session(input, session_id.clone());
                let py_runtime_data = Py::new(py, py_runtime_data_struct)
                    .map_err(|e| Error::Execution(format!("Failed to create Python RuntimeData: {}", e)))?;

                // Create event loop
                let event_loop = self.event_loop.get_or_create(py)
                    .map_err(|e| Error::Execution(format!("Failed to create event loop: {}", e)))?;

                // Call process()
                let process_result = instance
                    .call_method1("process", (py_runtime_data,))
                    .map_err(|e| Error::Execution(format!("Failed to call process(): {}", e)))?;

                // Verify it's an async generator
                if !AsyncGenerator::is_async_generator(py, &process_result)
                    .map_err(|e| Error::Execution(format!("Failed to check async generator: {}", e)))?
                {
                    return Err(Error::Execution(
                        "Python node process() must return an async generator".to_string(),
                    ));
                }

                // Define helper function
                let code = std::ffi::CString::new(
                    r#"
def _get_next_with_loop(agen, loop):
    import sys
    sys.stdout.flush()
    sys.stderr.flush()

    async def get_next():
        try:
            item = await anext(agen)
            return (True, item)
        except StopAsyncIteration:
            return (False, None)

    return loop.run_until_complete(get_next())
"#,
                )
                .unwrap();

                py.run(&code, None, None)
                    .map_err(|e| Error::Execution(format!("Failed to define helper: {}", e)))?;

                let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)
                    .map_err(|e| Error::Execution(format!("Failed to get locals: {}", e)))?;
                let get_next_fn = locals.get_item("_get_next_with_loop")
                    .map_err(|e| Error::Execution(format!("Failed to get helper function: {}", e)))?;

                Ok((process_result.unbind(), event_loop.unbind(), get_next_fn.unbind()))
            })?;

        // Iterate and call callback
        tracing::info!("Starting async generator iteration loop for node: {}", self.node_type);
        let mut chunk_count = 0;

        loop {
            tracing::debug!("Calling get_next iteration {}", chunk_count);
            let (has_value, runtime_data_opt) =
                Python::with_gil(|py| -> Result<(bool, Option<RuntimeData>)> {
                    let process_result = process_result_py.bind(py);
                    let event_loop = event_loop_py.bind(py);
                    let get_next_fn = get_next_fn_py.bind(py);

                    let result_tuple = get_next_fn
                        .call1((process_result, &event_loop))
                        .map_err(|e| Error::Execution(format!("Failed to get next item: {}", e)))?;

                    let has_value: bool = result_tuple
                        .get_item(0)
                        .and_then(|v| v.extract())
                        .map_err(|e| Error::Execution(format!("Failed to extract has_value: {}", e)))?;

                    if !has_value {
                        return Ok((false, None));
                    }

                    let py_item = result_tuple
                        .get_item(1)
                        .map_err(|e| Error::Execution(format!("Failed to get item: {}", e)))?;

                    let runtime_data = extract_runtime_data(py, &py_item)?;

                    Ok((true, Some(runtime_data)))
                })?;

            if !has_value {
                tracing::info!("Generator exhausted after {} items", chunk_count);
                break;
            }

            if let Some(runtime_data) = runtime_data_opt {
                chunk_count += 1;
                tracing::info!("Yielded item {}: type={:?}", chunk_count, runtime_data.data_type());
                callback(runtime_data)?;
                tokio::task::yield_now().await;
            }
        }

        tracing::info!("Successfully processed {} chunks via callback", chunk_count);
        Ok(chunk_count)
    }
}

#[async_trait]
impl NodeExecutor for CPythonNodeExecutor {
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

        // Initialize Python runtime lazily (WASM-compatible)
        #[cfg(target_family = "wasm")]
        {
            use std::sync::Once;
            static PYTHON_INIT: Once = Once::new();
            PYTHON_INIT.call_once(|| {
                pyo3::prepare_freethreaded_python();
                Python::with_gil(|py| {
                    let sys = py.import("sys").expect("Failed to import sys");
                    sys.setattr("recursionlimit", 100)
                        .expect("Failed to set recursion limit");
                });
            });
        }

        // Create node instance
        let instance = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            let class = self.load_class(py)?;

            // Merge node_id into params
            let mut params_with_id = context.params.clone();
            if let Some(obj) = params_with_id.as_object_mut() {
                obj.insert(
                    "node_id".to_string(),
                    serde_json::Value::String(context.node_id.clone()),
                );
            } else {
                params_with_id = serde_json::json!({"node_id": context.node_id.clone()});
            }

            let instance = self.instantiate_node(py, &class, &params_with_id)?;
            self.call_initialize(py, &instance)?;

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

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        if !self.initialized {
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        let instance = self
            .instance
            .as_ref()
            .ok_or_else(|| Error::Execution(format!("CPython node {} has no instance", self.node_type)))?;

        Python::with_gil(|py| -> PyResult<Vec<Value>> {
            let bound_instance = instance.bind(py);

            // Convert input
            let py_input = json_to_python_with_cache(py, &input, self.py_cache.as_ref())?;

            // Call process()
            let py_result = bound_instance.call_method1("process", (py_input,))?;

            // Handle None (filtered out)
            if py_result.is_none() {
                return Ok(vec![]);
            }

            // Check if it's an async generator
            if AsyncGenerator::is_async_generator(py, &py_result)? {
                let gen = AsyncGenerator::new(py_result.unbind());
                let event_loop = self.event_loop.get_or_create(py)?;
                let items = gen.collect_all(py, &event_loop)?;

                let mut results = Vec::new();
                for item in items {
                    let json_value = python_to_json_with_cache(py, &item, self.py_cache.as_ref())?;
                    results.push(json_value);
                }

                return Ok(results);
            }

            // Check if it's a coroutine
            if AsyncGenerator::is_coroutine(py, &py_result)? {
                let awaited = self.event_loop.run_until_complete(py, &py_result)?;

                if AsyncGenerator::is_async_generator(py, &awaited)? {
                    let gen = AsyncGenerator::new(awaited.unbind());
                    let event_loop = self.event_loop.get_or_create(py)?;
                    let items = gen.collect_all(py, &event_loop)?;

                    let mut results = Vec::new();
                    for item in items {
                        let json_value = python_to_json_with_cache(py, &item, self.py_cache.as_ref())?;
                        results.push(json_value);
                    }

                    return Ok(results);
                }

                let json_result = python_to_json_with_cache(py, &awaited, self.py_cache.as_ref())?;
                return Ok(vec![json_result]);
            }

            // Normal result
            let json_result = python_to_json_with_cache(py, &py_result, self.py_cache.as_ref())?;
            Ok(vec![json_result])
        })
        .map_err(|e: PyErr| {
            Error::Execution(format!(
                "Failed to process data in CPython node {}: {}",
                self.node_type, e
            ))
        })
    }

    async fn cleanup(&mut self) -> Result<()> {
        if !self.initialized {
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
        self.streaming_queue = None;
        self.active_generator = None;

        tracing::info!("CPython node {} cleanup complete", self.node_type);
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        self.is_streaming && self.streaming_queue.is_some()
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: format!("CPython({})", self.node_type),
            version: "1.0.0".to_string(),
            description: Some(format!(
                "CPython-based executor for {} node",
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

        Python::with_gil(|py| {
            let code = CString::new(
                r#"
class TestNodeLifecycle:
    def __init__(self, multiplier=1):
        self.multiplier = multiplier

    def process(self, data):
        return data * self.multiplier
"#,
            )
            .unwrap();
            py.run(&code, None, None).unwrap();

            let mock_code = CString::new(
                "import types, sys; mock_module = sys.modules.get('remotemedia.nodes') or types.ModuleType('remotemedia.nodes'); mock_module.TestNodeLifecycle = TestNodeLifecycle; sys.modules['remotemedia.nodes'] = mock_module"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();
        });

        let mut executor = CPythonNodeExecutor::new("TestNodeLifecycle");

        let context = NodeContext {
            node_id: "test_0".to_string(),
            node_type: "TestNodeLifecycle".to_string(),
            params: serde_json::json!({ "multiplier": 3 }),
            session_id: None,
            metadata: HashMap::new(),
        };

        executor.initialize(&context).await.unwrap();
        assert!(executor.initialized);

        let result = executor.process(serde_json::json!(5)).await.unwrap();
        assert_eq!(result, vec![serde_json::json!(15)]);

        executor.cleanup().await.unwrap();
        assert!(!executor.initialized);
    }
}
