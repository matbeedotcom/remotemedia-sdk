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

use crate::data::RuntimeData;
use crate::executor::PyObjectCache;
use crate::nodes::{NodeContext, NodeExecutor};
use crate::python::marshal::{
    json_to_python, json_to_python_with_cache, python_to_json, python_to_json_with_cache,
};
use crate::{Error, Result};
use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::{PyRef, PyRefMut};
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

    /// Cache for Python objects (avoids serialization for Python-to-Python data flow)
    py_cache: Option<PyObjectCache>,

    /// Active async generator for streaming nodes
    /// Stores the generator between process() calls to yield items one at a time
    active_generator: Option<Py<PyAny>>,

    /// Whether this node is a streaming node (async generator)
    is_streaming: bool,

    /// Streaming queue for feeding data to streaming nodes
    /// This queue persists across process() calls and allows continuous feeding
    streaming_queue: Option<Py<PyAny>>,

    /// Whether the streaming queue has been signaled as finished
    streaming_finished: bool,

    /// Event loop for async operations (shared across all async calls for streaming nodes)
    /// This ensures the queue and generator use the same event loop
    event_loop: Option<Py<PyAny>>,
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
            py_cache: None,
            active_generator: None,
            is_streaming: false,
            streaming_queue: None,
            streaming_finished: false,
            event_loop: None,
        }
    }

    /// Create a new CPython node executor with PyObject cache
    pub fn new_with_cache(node_type: impl Into<String>, py_cache: PyObjectCache) -> Self {
        Self {
            node_type: node_type.into(),
            instance: None,
            initialized: false,
            py_cache: Some(py_cache),
            active_generator: None,
            is_streaming: false,
            streaming_queue: None,
            streaming_finished: false,
            event_loop: None,
        }
    }

    /// Set whether this node should use streaming mode
    pub fn set_is_streaming_node(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
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

    /// Call the initialize() method if it exists (supports both sync and async)
    fn call_initialize(&mut self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        // Check if the node has an initialize method
        if instance.hasattr("initialize")? {
            tracing::info!("Calling initialize() on CPython node: {}", self.node_type);

            let result = instance.call_method0("initialize")?;

            // Check if the result is a coroutine (async method)
            let inspect = py.import("inspect")?;
            let is_coroutine = inspect.call_method1("iscoroutine", (&result,))?.extract::<bool>()?;

            if is_coroutine {
                tracing::info!("Initialize method is async, awaiting it with thread isolation...");

                // CRITICAL WORKAROUND: PyTorch and other libraries cause heap corruption when
                // their operations execute within an async event loop context on Windows.
                // We MUST run the coroutine in a completely isolated thread pool.

                let code = std::ffi::CString::new(
                    r#"
import asyncio

def _run_init_in_thread(coro):
    """Run async initialization in a separate thread to isolate from async context.

    CRITICAL: PyTorch operations cause heap corruption when executed within
    an event loop context on Windows. This function creates a new thread with
    its own event loop, completely isolating PyTorch from the caller's context.
    """
    # Create a new event loop for this thread
    new_loop = asyncio.new_event_loop()
    asyncio.set_event_loop(new_loop)

    try:
        # Run the coroutine in this thread's event loop
        result = new_loop.run_until_complete(coro)
        return result
    finally:
        # Clean up the thread's event loop
        new_loop.close()
"#,
                )
                .unwrap();

                py.run(&code, None, None)?;

                let locals_code = std::ffi::CString::new("locals()").unwrap();
                let locals = py.eval(&locals_code, None, None)?;
                let run_init_fn = locals.get_item("_run_init_in_thread")?;

                // Import asyncio for to_thread
                let asyncio = py.import("asyncio")?;

                // Create a coroutine that runs the initialization in a thread
                let thread_runner_code = std::ffi::CString::new(
                    r#"
import asyncio

async def _run_with_thread_isolation(run_fn, coro):
    """Wrapper that runs initialization in thread pool."""
    return await asyncio.to_thread(run_fn, coro)
"#,
                )
                .unwrap();

                py.run(&thread_runner_code, None, None)?;
                let locals2 = py.eval(&locals_code, None, None)?;
                let thread_wrapper_fn = locals2.get_item("_run_with_thread_isolation")?;

                // Create the wrapper coroutine
                let wrapped_coro = thread_wrapper_fn.call1((run_init_fn, result))?;

                // Create event loop to run the wrapper
                #[cfg(target_os = "windows")]
                let new_loop = {
                    tracing::info!("Windows: creating SelectorEventLoop for initialization wrapper");
                    py.run(&std::ffi::CString::new(
                        "import asyncio; selector_loop = asyncio.SelectorEventLoop()"
                    ).unwrap(), None, None)?;
                    let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
                    locals.get_item("selector_loop")?
                };

                #[cfg(not(target_os = "windows"))]
                let new_loop = asyncio.call_method0("new_event_loop")?;

                asyncio.call_method1("set_event_loop", (&new_loop,))?;

                // Store the event loop for later use
                self.event_loop = Some(new_loop.clone().unbind().into());
                let loop_type = new_loop.get_type().name().map(|s| s.to_string()).unwrap_or_else(|_| "unknown".to_string());
                tracing::info!("Stored event loop for future use (type: {})", loop_type);

                // Run the wrapper coroutine (which runs actual init in thread pool)
                match new_loop.call_method1("run_until_complete", (wrapped_coro,)) {
                    Ok(_) => {
                        tracing::info!("Async initialize completed successfully (thread-isolated)");
                    }
                    Err(e) => {
                        // Check if this is a SystemExit exception
                        let is_system_exit = e.is_instance_of::<pyo3::exceptions::PySystemExit>(py);

                        if is_system_exit {
                            tracing::error!("Python code called sys.exit() during initialization - converting to error: {}", e);
                            // Clean up the event loop
                            let _ = new_loop.call_method0("close");
                            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                                format!("Initialize failed: Python code called sys.exit(). This usually means a dependency failed to install or initialize. Error: {}", e)
                            ).into());
                        } else {
                            tracing::error!("Async initialize failed: {:?}", e);
                            // Clean up the event loop before propagating error
                            let _ = new_loop.call_method0("close");
                            return Err(e);
                        }
                    }
                }

                // DON'T close the event loop here - we need it for later processing!
                // The event loop will be reused for process_runtime_data() calls
                tracing::info!("Keeping event loop alive for future use");
            }

            tracing::info!("CPython node {} initialized", self.node_type);
        } else {
            tracing::info!("CPython node {} has no initialize() method", self.node_type);
        }
        Ok(())
    }

    /// Call the cleanup() method if it exists
    fn call_cleanup(&self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        // Check if the node has a cleanup method
        if instance.hasattr("cleanup")? {
            tracing::info!("Calling cleanup() on CPython node: {}", self.node_type);

            // As of Phase 1 WASM compatibility update, cleanup() is now synchronous
            // This eliminates asyncio dependency which is not fully supported in WASM
            instance.call_method0("cleanup")?;

            tracing::info!("CPython node {} cleaned up", self.node_type);
        } else {
            tracing::info!("CPython node {} has no cleanup() method", self.node_type);
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
    fn process_is_async_gen_function(
        &self,
        py: Python,
        instance: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        let inspect = py.import("inspect")?;

        // Get the process method
        let process_method = instance.getattr("process")?;

        // Check if it's an async generator function
        let is_async_gen_func = inspect.call_method1("isasyncgenfunction", (process_method,))?;
        is_async_gen_func.extract::<bool>()
    }

    /// Wrap input data as an async generator for streaming nodes
    fn wrap_input_as_async_generator<'py>(
        &self,
        py: Python<'py>,
        input: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // Create an async generator that yields the input data once
        let wrapper_code = std::ffi::CString::new(
            r#"
async def _input_wrapper(data):
    """Wrap single input as async generator that yields once."""
    yield data

_wrapped = _input_wrapper
"#,
        )
        .unwrap();

        py.run(&wrapper_code, None, None)?;

        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let wrapper_fn = locals.get_item("_wrapped")?;

        // Call the wrapper function to create the generator
        let async_gen = wrapper_fn.call1((input,))?;

        Ok(async_gen)
    }

    /// Create a persistent async queue for feeding data to streaming nodes
    fn create_async_queue<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let code = std::ffi::CString::new(r#"
import asyncio

class StreamingQueue:
    """Queue for feeding data into streaming nodes."""
    def __init__(self):
        self.items = []
        self.index = 0
        self.finished = False

    def put(self, item):
        """Add an item to the list."""
        self.items.append(item)

    def finish(self):
        """Signal that no more items will be added."""
        self.finished = True

    async def stream(self):
        """Async generator that yields items from the list."""
        # Keep yielding items until finished flag is set
        import logging
        import asyncio
        logger = logging.getLogger(__name__)
        logger.info(f"StreamingQueue.stream() started, items={len(self.items)}, finished={self.finished}")

        while not self.finished or self.index < len(self.items):
            # Yield all available items
            while self.index < len(self.items):
                item = self.items[self.index]
                self.index += 1
                logger.info(f"StreamingQueue.stream() yielding item {self.index}/{len(self.items)}")
                yield item

            # If not finished, wait a bit for more items
            if not self.finished:
                logger.info(f"StreamingQueue.stream() waiting for more items...")
                await asyncio.sleep(0.001)  # Small sleep to yield control

        logger.info(f"StreamingQueue.stream() finished yielding all items")

_StreamingQueue = StreamingQueue
"#).unwrap();

        py.run(&code, None, None)?;

        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let queue_class = locals.get_item("_StreamingQueue")?;

        // Create an instance of the queue
        let queue_instance = queue_class.call0()?;

        Ok(queue_instance)
    }

    /// Feed an item into the streaming queue
    fn feed_streaming_queue(
        &self,
        py: Python,
        queue: &Bound<'_, PyAny>,
        item: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        // Call queue.put(item) using py.run() instead of call_method1()
        // For some reason call_method1() doesn't work reliably when event loops are involved
        let code = std::ffi::CString::new("queue_ref.put(item_ref)").unwrap();
        let locals = pyo3::types::PyDict::new(py);
        locals.set_item("queue_ref", queue)?;
        locals.set_item("item_ref", item)?;
        py.run(&code, None, Some(&locals))?;
        Ok(())
    }

    /// Signal that the streaming queue is finished (no more items)
    fn finish_streaming_queue(&self, _py: Python, queue: &Bound<'_, PyAny>) -> PyResult<()> {
        queue.call_method0("finish")?;
        Ok(())
    }

    /// Initialize streaming mode for this node
    /// Creates a queue, starts the node's process() generator with the queue's stream,
    /// and stores the generator for later result pulling
    fn initialize_streaming(&mut self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        tracing::info!("Initializing streaming mode for node: {}", self.node_type);

        // Get or create the event loop that will be shared across all async operations
        let asyncio = py.import("asyncio")?;
        let event_loop = asyncio.call_method0("new_event_loop")?;
        asyncio.call_method1("set_event_loop", (&event_loop,))?;
        self.event_loop = Some(event_loop.unbind().into());
        tracing::info!(
            "Created new event loop for streaming node: {}",
            self.node_type
        );

        // Create the streaming queue
        let queue = self.create_async_queue(py)?;
        tracing::info!("Created streaming queue for node: {}", self.node_type);

        // Get the stream() method from the queue
        let stream_gen = queue.call_method0("stream")?;

        // Call node.process(stream) to start the generator
        let process_result = instance.call_method1("process", (stream_gen,))?;

        // The result should be an async generator
        if !self.is_async_generator(py, &process_result)? {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Streaming node {} process() must return async generator",
                self.node_type
            )));
        }

        // Store both the queue and the generator
        self.streaming_queue = Some(queue.unbind().into());
        self.active_generator = Some(process_result.unbind().into());
        self.is_streaming = true;

        tracing::info!("Streaming mode initialized for node: {}", self.node_type);
        Ok(())
    }

    /// Check if a Python object is a coroutine
    fn is_coroutine(&self, py: Python, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let inspect = py.import("inspect")?;
        let is_coro = inspect.call_method1("iscoroutine", (obj,))?;
        is_coro.extract::<bool>()
    }

    /// Await a Python coroutine using asyncio
    fn await_coroutine<'py>(
        &self,
        py: Python<'py>,
        coro: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        // For streaming nodes, use the stored event loop to ensure queue and generator
        // run in the same event loop context
        if let Some(event_loop_py) = &self.event_loop {
            let event_loop = event_loop_py.bind(py);
            return event_loop.call_method1("run_until_complete", (coro,));
        }

        // For non-streaming nodes, create a temporary event loop
        let asyncio = py.import("asyncio")?;
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

    /// Get the next item from an async generator
    /// Returns None if the generator is exhausted
    fn get_next_from_generator(
        &self,
        py: Python,
        async_gen: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Value>> {
        tracing::info!(
            "Getting next item from async generator for node: {}",
            self.node_type
        );

        // Use __anext__() to get the next item from the async generator
        let code = std::ffi::CString::new(
            r#"
async def _get_next(agen):
    """Get next item from async generator, or None if exhausted."""
    try:
        item = await agen.__anext__()
        return (True, item)
    except StopAsyncIteration:
        return (False, None)
"#,
        )
        .unwrap();

        py.run(&code, None, None)?;

        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let get_next_fn = locals.get_item("_get_next")?;

        // Call the helper function
        tracing::info!("Calling _get_next coroutine for node: {}", self.node_type);
        let coro = get_next_fn.call1((async_gen,))?;

        // Await the coroutine
        tracing::info!("Awaiting coroutine for node: {}", self.node_type);
        let result_tuple = self.await_coroutine(py, &coro)?;
        tracing::info!("Coroutine completed for node: {}", self.node_type);

        // Extract (has_value, value) tuple
        let has_value: bool = result_tuple.get_item(0)?.extract()?;

        if !has_value {
            tracing::info!("Async generator exhausted for node {}", self.node_type);
            return Ok(None);
        }

        let item = result_tuple.get_item(1)?;

        // Convert this single item using cache-aware conversion
        let json_value = self.convert_output_to_json(py, &item)?;

        tracing::info!(
            "Got next item from async generator for node {}",
            self.node_type
        );

        Ok(Some(json_value))
    }

    /// Try to get next item from async generator without blocking
    /// Returns None if no item is immediately available
    fn try_get_next_from_generator(
        &self,
        py: Python,
        async_gen: &Bound<'_, PyAny>,
    ) -> PyResult<Option<Value>> {
        // Use asyncio.wait_for with a reasonable timeout to allow async code to run
        // Use 1 second timeout to ensure we catch any yields that happen during processing
        let code = std::ffi::CString::new(
            r#"
import asyncio

async def _try_get_next(agen):
    """Try to get next item without blocking, return None if not ready."""
    try:
        item = await asyncio.wait_for(agen.__anext__(), timeout=1.0)
        return (True, item)
    except asyncio.TimeoutError:
        # No item ready yet
        return (False, None)
    except StopAsyncIteration:
        # Generator exhausted
        return (False, None)
"#,
        )
        .unwrap();

        py.run(&code, None, None)?;

        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let try_get_next_fn = locals.get_item("_try_get_next")?;

        // Call the helper function
        let coro = try_get_next_fn.call1((async_gen,))?;

        // Await the coroutine
        let result_tuple = self.await_coroutine(py, &coro)?;

        // Extract (has_value, value) tuple
        let has_value: bool = result_tuple.get_item(0)?.extract()?;

        if !has_value {
            return Ok(None);
        }

        let item = result_tuple.get_item(1)?;

        // Convert this single item using cache-aware conversion
        let json_value = self.convert_output_to_json(py, &item)?;

        Ok(Some(json_value))
    }

    /// Iterate async generator and convert each item to JSON individually
    /// Returns a Vec<Value> instead of collecting into a single array first
    /// THIS IS DEPRECATED - use get_next_from_generator for true streaming
    fn iterate_async_generator(
        &self,
        py: Python,
        async_gen: &Bound<'_, PyAny>,
    ) -> PyResult<Vec<Value>> {
        tracing::info!("Iterating async generator for node: {}", self.node_type);

        // We need to iterate through the async generator and convert each item
        // This requires running it in an asyncio event loop
        let code = std::ffi::CString::new(
            r#"
async def _iterate_async_gen(agen, converter):
    """Iterate async generator and convert each item individually."""
    results = []
    count = 0
    async for item in agen:
        # Converter is called for each item to convert it immediately
        converted = converter(item)
        results.append(converted)
        count += 1
    return results, count
"#,
        )
        .unwrap();

        // Execute the helper function
        py.run(&code, None, None)?;

        // Get the helper function
        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let iterate_fn = locals.get_item("_iterate_async_gen")?;

        // Create a converter function that will be called for each item
        // This converts each item to JSON with caching BEFORE appending to the list
        let converter_code = std::ffi::CString::new(
            r#"
def _converter(item):
    """Identity function - just return the item as-is for now."""
    return item
"#,
        )
        .unwrap();

        py.run(&converter_code, None, None)?;
        let converter_fn = locals.get_item("_converter")?;

        // Call the helper function with the async generator and converter
        let coro = iterate_fn.call1((async_gen, converter_fn))?;

        // Await the coroutine to get results
        tracing::info!("Awaiting async generator iteration...");
        let result_tuple = self.await_coroutine(py, &coro)?;

        // Extract results list and count
        let results_list = result_tuple.get_item(0)?;
        let count: usize = result_tuple.get_item(1)?.extract()?;

        tracing::info!("Async generator yielded {} items", count);

        // Convert each Python item to JSON Value individually
        let mut json_items = Vec::new();
        if let Ok(py_list) = results_list.downcast::<pyo3::types::PyList>() {
            for item in py_list.iter() {
                // Convert this single item using cache-aware conversion
                let json_value = self.convert_output_to_json(py, &item)?;
                json_items.push(json_value);
            }
        }

        tracing::info!(
            "Converted {} items from async generator for node {}",
            json_items.len(),
            self.node_type
        );

        Ok(json_items)
    }

    /// Convert Python output to JSON, using cache for complex objects if available
    fn convert_output_to_json(&self, py: Python, py_obj: &Bound<'_, PyAny>) -> PyResult<Value> {
        // Use cache-aware conversion if cache is available
        python_to_json_with_cache(py, py_obj, self.py_cache.as_ref())
    }

    /// Process RuntimeData through the CPython node (returns single RuntimeData, not Vec)
    ///
    /// This is a simplified interface for nodes that work with RuntimeData directly.
    /// It calls the Python node's process() method with RuntimeData and expects
    /// an async generator that yields RuntimeData objects.
    pub async fn process_runtime_data(&mut self, input: RuntimeData) -> Result<RuntimeData> {
        tracing::info!("========================================");
        tracing::info!("process_runtime_data called for node: {}", self.node_type);
        tracing::info!("Input data type: {}", input.type_name());

        if !self.initialized {
            tracing::error!("Node {} not initialized, returning error", self.node_type);
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        tracing::info!("Node is initialized, acquiring GIL for process_runtime_data");
        Python::with_gil(|py| -> Result<RuntimeData> {
            tracing::info!("GIL acquired, getting instance reference");
            let instance = self.instance.as_ref()
                .ok_or_else(|| {
                    tracing::error!("Python node instance is None");
                    Error::Execution("Python node not initialized".to_string())
                })?
                .bind(py);
            tracing::info!("Got instance reference");

            // Convert RuntimeData to Python (must be done within GIL)
            tracing::info!("Converting RuntimeData to Python, data type: {}", input.type_name());
            use crate::python::{runtime_data_to_py, PyRuntimeData};

            tracing::info!("Calling runtime_data_to_py()");
            let py_runtime_data_struct = runtime_data_to_py(input);
            tracing::info!("runtime_data_to_py() returned successfully");

            // Convert the Rust struct to a Python object
            tracing::info!("Creating Python RuntimeData object with Py::new()");
            let py_runtime_data = match pyo3::Py::new(py, py_runtime_data_struct) {
                Ok(data) => {
                    tracing::info!("Successfully created Python RuntimeData object");
                    data
                }
                Err(e) => {
                    tracing::error!("Failed to create Python RuntimeData: {}", e);
                    return Err(Error::Execution(format!("Failed to create Python RuntimeData: {}", e)));
                }
            };

            // DIFFERENT APPROACH: Create the event loop BEFORE calling process()
            // This way the async generator is created within the event loop context
            tracing::info!("Creating event loop before calling process()");

            let asyncio = py.import("asyncio")
                .map_err(|e| Error::Execution(format!("Failed to import asyncio: {}", e)))?;

            // Create SelectorEventLoop on Windows
            #[cfg(target_os = "windows")]
            let event_loop = {
                tracing::info!("Windows: creating SelectorEventLoop");
                py.run(&std::ffi::CString::new(
                    "import asyncio; _proc_loop = asyncio.SelectorEventLoop(); asyncio.set_event_loop(_proc_loop)"
                ).unwrap(), None, None)
                    .map_err(|e| Error::Execution(format!("Failed to create SelectorEventLoop: {}", e)))?;
                let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)
                    .map_err(|e| Error::Execution(format!("Failed to get locals: {}", e)))?;
                locals.get_item("_proc_loop")
                    .map_err(|e| Error::Execution(format!("Failed to get loop: {}", e)))?
            };

            #[cfg(not(target_os = "windows"))]
            let event_loop = {
                let loop_obj = asyncio.call_method0("new_event_loop")
                    .map_err(|e| Error::Execution(format!("Failed to create event loop: {}", e)))?;
                asyncio.call_method1("set_event_loop", (&loop_obj,))
                    .map_err(|e| Error::Execution(format!("Failed to set event loop: {}", e)))?;
                loop_obj
            };

            tracing::info!("Event loop created and set as current");

            // NOW call process() - the async generator will be created in this loop's context
            tracing::info!("About to call Python node's process() method");
            let process_result = match instance.call_method1("process", (py_runtime_data,)) {
                Ok(result) => {
                    tracing::info!("process() method returned successfully");
                    result
                }
                Err(e) => {
                    tracing::error!("Failed to call process(): {}", e);
                    tracing::error!("Python error details: {:?}", e);
                    let _ = event_loop.call_method0("close");
                    return Err(Error::Execution(format!("Failed to call process(): {}", e)));
                }
            };

            // Check if it's an async generator
            tracing::info!("Checking if result is async generator");
            let inspect = py.import("inspect")
                .map_err(|e| Error::Execution(format!("Failed to import inspect: {}", e)))?;
            let is_async_gen = inspect.call_method1("isasyncgen", (&process_result,))
                .and_then(|v| v.extract::<bool>())
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to check if async gen: {}", e))
                })?;

            if !is_async_gen {
                let _ = event_loop.call_method0("close");
                return Err(Error::Execution(
                    "Python node process() must return an async generator".to_string()
                ));
            }
            tracing::info!("Confirmed result is async generator");

            // Now iterate it using the SAME event loop
            tracing::info!("Getting first item from async generator using the same event loop");

            // Use the EXISTING event loop that we just set as current
            let code = std::ffi::CString::new(
                r#"
import asyncio
import logging
import sys

logger = logging.getLogger(__name__)

async def _get_first_item_async(agen):
    """Get the first item from async generator."""
    logger.info("_get_first_item_async: Starting")
    logger.info(f"_get_first_item_async: agen type: {type(agen).__name__}")

    try:
        logger.info("_get_first_item_async: Calling anext(agen)")
        result = await anext(agen)
        logger.info(f"_get_first_item_async: Got result type: {type(result).__name__}")
        return result
    except StopAsyncIteration:
        logger.error("_get_first_item_async: Async generator was empty")
        raise
    except Exception as e:
        logger.error(f"_get_first_item_async: Error: {type(e).__name__}: {e}")
        logger.error("_get_first_item_async: Traceback:", exc_info=True)
        raise

def _run_anext_in_thread(agen):
    """Run anext() in a separate thread to isolate from async context.

    CRITICAL WORKAROUND: PyTorch and other libraries cause heap corruption when
    their operations execute within an async event loop context on Windows.
    Running anext() in a thread pool completely isolates the node's code from
    the event loop, preventing the corruption.
    """
    import asyncio

    # Create a new event loop for this thread
    new_loop = asyncio.new_event_loop()
    asyncio.set_event_loop(new_loop)

    try:
        # Run the async generator iteration in this thread's event loop
        async def get_next():
            return await anext(agen)

        result = new_loop.run_until_complete(get_next())
        return result
    finally:
        # Clean up the thread's event loop
        new_loop.close()

def _run_with_existing_loop(agen, loop):
    """Use the existing event loop to iterate the async generator.

    GLOBAL THREAD POOL ISOLATION: To prevent heap corruption with PyTorch and
    other libraries on Windows, we run the actual anext() call in a separate
    thread using asyncio.to_thread().
    """
    try:
        # Flush all Python I/O streams before event loop operations
        sys.stdout.flush()
        sys.stderr.flush()

        # WORKAROUND: Explicitly garbage collect before event loop to ensure clean state
        import gc
        gc.collect()

        # CRITICAL: Run anext() in a thread pool to isolate from async context
        # This prevents heap corruption with PyTorch and other libraries
        import asyncio
        result = loop.run_until_complete(
            asyncio.to_thread(_run_anext_in_thread, agen)
        )
        return result

    except Exception as e:
        raise
"#,
            ).unwrap();

            tracing::info!("Defining Python helper functions for anext()");
            py.run(&code, None, None)
                .map_err(|e| Error::Execution(format!("Failed to define helper: {}", e)))?;
            tracing::info!("Python helper functions defined successfully");

            let locals_code = std::ffi::CString::new("locals()").unwrap();
            let locals = py.eval(&locals_code, None, None)
                .map_err(|e| Error::Execution(format!("Failed to get locals: {}", e)))?;
            tracing::info!("Got locals dict");

            let existing_loop_fn = locals.get_item("_run_with_existing_loop")
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to get existing loop wrapper: {}", e))
                })?;
            tracing::info!("Got _run_with_existing_loop function reference");

            tracing::info!("About to call _run_with_existing_loop - generator and loop from same context");
            tracing::info!("Parameters: process_result type={:?}, event_loop type={:?}",
                process_result.get_type().name(),
                event_loop.get_type().name());

            let anext_result = match existing_loop_fn.call1((process_result, &event_loop)) {
                Ok(result) => {
                    tracing::info!("Successfully called _run_with_existing_loop, got result");
                    result
                }
                Err(e) => {
                    tracing::error!("Failed to get first item from async generator: {}", e);
                    tracing::error!("Python traceback: {:?}", e);
                    let _ = event_loop.call_method0("close");
                    return Err(Error::Execution(format!("Failed to get first item: {}", e)));
                }
            };
            tracing::info!("Got first item from async generator, about to extract RuntimeData");

            // IMPORTANT: Do NOT close the event loop!
            // Closing the event loop after PyTorch operations causes heap corruption on Windows
            // The event loop will be garbage collected by Python when no longer referenced
            tracing::info!("Skipping event loop close (PyTorch heap corruption workaround)");
            // let _ = event_loop.call_method0("close");  // DISABLED - causes crash with PyTorch

            // Extract RuntimeData from the Python result
            tracing::info!("Extracting RuntimeData from Python result");
            tracing::info!("Python result type: {:?}", anext_result.get_type().name());

            // Try to get some debug info about the object
            if let Ok(repr) = anext_result.repr() {
                if let Ok(repr_str) = repr.extract::<String>() {
                    // Truncate to avoid flooding logs
                    let truncated = if repr_str.len() > 200 {
                        format!("{}...", &repr_str[..200])
                    } else {
                        repr_str
                    };
                    tracing::info!("Python result repr (truncated): {}", truncated);
                }
            }

            // Check if this is a dict with "_audio_numpy" key (from TTS node workaround)
            // This avoids calling numpy_to_audio() inside the event loop which causes heap corruption
            if anext_result.is_instance_of::<pyo3::types::PyDict>() {
                tracing::info!("Result is a dict, checking for _audio_numpy key");
                let result_dict = anext_result.downcast::<pyo3::types::PyDict>()
                    .map_err(|e| Error::Execution(format!("Failed to downcast to PyDict: {}", e)))?;

                if let Ok(audio_numpy) = result_dict.get_item("_audio_numpy") {
                    if let Some(audio_numpy) = audio_numpy {
                        tracing::info!("Found _audio_numpy in dict, converting to RuntimeData.Audio AFTER event loop");

                        // Get sample_rate and channels
                        let sample_rate = result_dict.get_item("_sample_rate")
                            .ok().flatten()
                            .and_then(|v| v.extract::<u32>().ok())
                            .ok_or_else(|| Error::Execution("Missing _sample_rate in audio dict".to_string()))?;

                        let channels = result_dict.get_item("_channels")
                            .ok().flatten()
                            .and_then(|v| v.extract::<u32>().ok())
                            .ok_or_else(|| Error::Execution("Missing _channels in audio dict".to_string()))?;

                        // NOW call numpy_to_audio - this is AFTER the event loop is closed
                        tracing::info!("Calling numpy_to_audio() with sample_rate={}, channels={}", sample_rate, channels);
                        use crate::python::runtime_data_py::PyRuntimeData;
                        let numpy_to_audio_fn = py.import("remotemedia_runtime.runtime_data")
                            .and_then(|m| m.getattr("numpy_to_audio"))
                            .map_err(|e| Error::Execution(format!("Failed to import numpy_to_audio: {}", e)))?;

                        let py_runtime_data_obj = numpy_to_audio_fn.call1((audio_numpy, sample_rate, channels))
                            .map_err(|e| Error::Execution(format!("Failed to call numpy_to_audio: {}", e)))?;

                        let py_runtime_data = py_runtime_data_obj.extract::<PyRuntimeData>()
                            .map_err(|e| Error::Execution(format!("Failed to extract PyRuntimeData from numpy_to_audio result: {}", e)))?;

                        tracing::info!("Successfully converted dict to RuntimeData.Audio");
                        let result = py_runtime_data.inner;
                        tracing::info!("Returning from Python::with_gil closure");
                        return Ok(result);
                    }
                }
            }

            // Otherwise, try to extract as PyRuntimeData (normal case)
            tracing::info!("About to extract PyRuntimeData from Python object");

            // WORKAROUND: PyO3 extraction is failing due to type mismatch
            // Instead, manually access the inner RuntimeData by calling methods
            let py_runtime_data = match anext_result.call_method0("data_type") {
                Ok(data_type_obj) => {
                    let data_type: String = data_type_obj.extract()
                        .map_err(|e| Error::Execution(format!("Failed to extract data_type: {}", e)))?;
                    tracing::info!("RuntimeData.data_type() = {}", data_type);

                    // Reconstruct RuntimeData based on type
                    match data_type.as_str() {
                        "text" => {
                            // Get text content - as_text() returns Option<String>
                            let text_obj = anext_result.call_method0("as_text")
                                .map_err(|e| Error::Execution(format!("Failed to call as_text(): {}", e)))?;
                            let text_opt: Option<String> = text_obj.extract()
                                .map_err(|e| Error::Execution(format!("Failed to extract text: {}", e)))?;
                            let text = text_opt.ok_or_else(|| Error::Execution("as_text() returned None".to_string()))?;
                            tracing::info!("Successfully extracted text: {} chars", text.len());
                            RuntimeData::Text(text)
                        }
                        "audio" => {
                            // Get audio buffer components using as_audio() method
                            // as_audio() returns Option<(PyObject, u32, u32, String, u64)>
                            // which is (samples_bytes, sample_rate, channels, format_str, num_samples)
                            // Note: as_audio() takes py: Python in Rust, but that's injected by PyO3, so call_method0
                            let audio_tuple_opt = anext_result.call_method0("as_audio")
                                .map_err(|e| Error::Execution(format!("Failed to call as_audio(): {}", e)))?;

                            // as_audio() returns Option, so we need to check if it's None
                            if audio_tuple_opt.is_none() {
                                return Err(Error::Execution("as_audio() returned None".to_string()));
                            }

                            // Extract as a tuple directly (as_audio returns Option<tuple>)
                            let audio_tuple_opt: Option<(Vec<u8>, u32, u32, String, u64)> = audio_tuple_opt.extract()
                                .map_err(|e| Error::Execution(format!("Failed to extract audio tuple: {}", e)))?;

                            let (samples, sample_rate, channels, format_str, num_samples) = audio_tuple_opt
                                .ok_or_else(|| Error::Execution("as_audio() returned None".to_string()))?;

                            // Convert format string to i32
                            let format = match format_str.as_str() {
                                "f32" => 0,
                                "i16" => 1,
                                "i32" => 2,
                                _ => 0, // Default to f32
                            };

                            tracing::info!("Successfully extracted audio: {} samples, {}Hz, {} channels, format={}", num_samples, sample_rate, channels, format_str);
                            RuntimeData::Audio(crate::grpc_service::generated::AudioBuffer {
                                samples,
                                sample_rate,
                                channels,
                                format,
                                num_samples,
                            })
                        }
                        "json" => {
                            // For JSON, we need to access the inner field and convert it
                            // Get the inner RuntimeData's JSON value as a string
                            let inner = anext_result.getattr("inner")
                                .map_err(|e| Error::Execution(format!("Failed to get inner: {}", e)))?;

                            // Convert inner to JSON string using Python's json module
                            let json_module = py.import("json")
                                .map_err(|e| Error::Execution(format!("Failed to import json module: {}", e)))?;
                            let json_str: String = json_module.call_method1("dumps", (inner,))
                                .map_err(|e| Error::Execution(format!("Failed to call json.dumps(): {}", e)))?
                                .extract()
                                .map_err(|e| Error::Execution(format!("Failed to extract JSON string: {}", e)))?;
                            let json_value: serde_json::Value = serde_json::from_str(&json_str)?;
                            tracing::info!("Successfully extracted JSON");
                            RuntimeData::Json(json_value)
                        }
                        "binary" => {
                            // For binary, access the inner Bytes directly
                            let inner = anext_result.getattr("inner")
                                .map_err(|e| Error::Execution(format!("Failed to get inner: {}", e)))?;
                            let bytes: Vec<u8> = inner.extract()
                                .map_err(|e| Error::Execution(format!("Failed to extract bytes from inner: {}", e)))?;
                            tracing::info!("Successfully extracted binary: {} bytes", bytes.len());
                            RuntimeData::Binary(prost::bytes::Bytes::from(bytes))
                        }
                        _ => {
                            return Err(Error::Execution(format!("Unsupported RuntimeData type: {}", data_type)));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to call data_type() method: {}", e);
                    return Err(Error::Execution(format!("Failed to extract RuntimeData via methods: {}", e)));
                }
            };

            tracing::info!("Successfully extracted RuntimeData, type: {}", py_runtime_data.type_name());
            tracing::info!("About to return RuntimeData from process_runtime_data");
            tracing::info!("Returning from Python::with_gil closure");
            Ok(py_runtime_data)
        }).map(|data| {
            tracing::info!("process_runtime_data completed successfully for node: {}", self.node_type);
            tracing::info!("Returned data type: {}", data.type_name());
            tracing::info!("========================================");
            data
        }).map_err(|e| {
            tracing::error!("process_runtime_data failed for node {}: {}", self.node_type, e);
            tracing::error!("========================================");
            e
        })
    }

    /// Process input and iterate Python async generator, calling callback for each chunk
    ///
    /// This method enables true streaming: one input can produce multiple outputs.
    /// It creates the generator and calls the callback for each yielded chunk as it arrives.
    pub async fn process_runtime_data_streaming<F>(&mut self, input: RuntimeData, session_id: Option<String>, mut callback: F) -> Result<usize>
    where
        F: FnMut(RuntimeData) -> Result<()> + Send,
    {
        tracing::info!("========================================");
        tracing::info!("process_runtime_data_all_chunks called for node: {} with session_id: {:?}", self.node_type, session_id);
        tracing::info!("Input data type: {}", input.type_name());

        if !self.initialized {
            tracing::error!("Node {} not initialized", self.node_type);
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        // Create the generator and helper function, then release GIL
        let (process_result_py, event_loop_py, get_next_fn_py) = Python::with_gil(|py| -> Result<(Py<PyAny>, Py<PyAny>, Py<PyAny>)> {
            let instance = self.instance.as_ref()
                .ok_or_else(|| Error::Execution("Python node not initialized".to_string()))?
                .bind(py);

            // Convert RuntimeData to Python with session_id
            use crate::python::{runtime_data_to_py_with_session, PyRuntimeData};
            let py_runtime_data_struct = runtime_data_to_py_with_session(input, session_id.clone());
            let py_runtime_data = pyo3::Py::new(py, py_runtime_data_struct)
                .map_err(|e| Error::Execution(format!("Failed to create Python RuntimeData: {}", e)))?;

            // Create event loop
            let asyncio = py.import("asyncio")
                .map_err(|e| Error::Execution(format!("Failed to import asyncio: {}", e)))?;

            #[cfg(target_os = "windows")]
            let event_loop = {
                tracing::info!("Windows: creating SelectorEventLoop");
                py.run(&std::ffi::CString::new(
                    "import asyncio; _proc_loop = asyncio.SelectorEventLoop(); asyncio.set_event_loop(_proc_loop)"
                ).unwrap(), None, None)
                    .map_err(|e| Error::Execution(format!("Failed to create SelectorEventLoop: {}", e)))?;
                let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)
                    .map_err(|e| Error::Execution(format!("Failed to get locals: {}", e)))?;
                locals.get_item("_proc_loop")
                    .map_err(|e| Error::Execution(format!("Failed to get loop: {}", e)))?
            };

            #[cfg(not(target_os = "windows"))]
            let event_loop = {
                let loop_obj = asyncio.call_method0("new_event_loop")
                    .map_err(|e| Error::Execution(format!("Failed to create event loop: {}", e)))?;
                asyncio.call_method1("set_event_loop", (&loop_obj,))
                    .map_err(|e| Error::Execution(format!("Failed to set event loop: {}", e)))?;
                loop_obj
            };

            tracing::info!("Event loop created, calling process() method");

            // Call process() to get the async generator
            let process_result = instance.call_method1("process", (py_runtime_data,))
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to call process(): {}", e))
                })?;

            // Verify it's an async generator
            let inspect = py.import("inspect")
                .map_err(|e| Error::Execution(format!("Failed to import inspect: {}", e)))?;
            let is_async_gen = inspect.call_method1("isasyncgen", (&process_result,))
                .and_then(|v| v.extract::<bool>())
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to check if async gen: {}", e))
                })?;

            if !is_async_gen {
                let _ = event_loop.call_method0("close");
                return Err(Error::Execution(
                    "Python node process() must return an async generator".to_string()
                ));
            }

            tracing::info!("Confirmed async generator, now iterating ALL yields");

            // Define helper to get next item from generator (one at a time)
            let code = std::ffi::CString::new(
                r#"
import asyncio
import logging

logger = logging.getLogger(__name__)

def _run_anext_in_thread(agen):
    """Run anext() in a separate thread to isolate from async context."""
    import asyncio
    new_loop = asyncio.new_event_loop()
    asyncio.set_event_loop(new_loop)
    try:
        async def get_next():
            return await anext(agen)
        result = new_loop.run_until_complete(get_next())
        return result
    finally:
        new_loop.close()

async def _get_next_async(agen):
    """Get next item from async generator."""
    try:
        # Run in thread pool for PyTorch safety
        item = await asyncio.to_thread(_run_anext_in_thread, agen)
        return (True, item)
    except StopAsyncIteration:
        return (False, None)

def _get_next_with_loop(agen, loop):
    """Use event loop to get next item from generator."""
    import sys
    sys.stdout.flush()
    sys.stderr.flush()

    result = loop.run_until_complete(_get_next_async(agen))
    return result
"#,
            ).unwrap();

            py.run(&code, None, None)
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to define helper: {}", e))
                })?;

            let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to get locals: {}", e))
                })?;

            let get_next_fn = locals.get_item("_get_next_with_loop")
                .map_err(|e| {
                    let _ = event_loop.call_method0("close");
                    Error::Execution(format!("Failed to get next function: {}", e))
                })?;

            // Return Py objects that we can use across GIL release/acquire
            Ok((process_result.unbind(), event_loop.unbind(), get_next_fn.unbind()))
        })?;

        // Now iterate with GIL release between chunks
        tracing::info!("Iterating generator and calling callback for each chunk");

        let mut chunk_count = 0;

        // Iterate generator, calling callback for each chunk as it arrives
        // IMPORTANT: We release and re-acquire GIL for each iteration
        loop {
            // Acquire GIL for this iteration
            let (has_value, runtime_data_opt) = Python::with_gil(|py| -> Result<(bool, Option<RuntimeData>)> {
                let process_result = process_result_py.bind(py);
                let event_loop = event_loop_py.bind(py);
                let get_next_fn = get_next_fn_py.bind(py);
                let result_tuple = get_next_fn.call1((process_result, &event_loop))
                    .map_err(|e| {
                        tracing::error!("Failed to get next item: {}", e);
                        Error::Execution(format!("Failed to get next item: {}", e))
                    })?;

                // Extract (has_value, item) tuple
                let has_value: bool = result_tuple.get_item(0)
                    .and_then(|v| v.extract())
                    .map_err(|e| {
                        Error::Execution(format!("Failed to extract has_value: {}", e))
                    })?;

                if !has_value {
                    tracing::info!("Generator exhausted after {} chunks", chunk_count);
                    return Ok((false, None));
                }

                let py_item = result_tuple.get_item(1)
                    .map_err(|e| {
                        Error::Execution(format!("Failed to get item: {}", e))
                    })?;

                tracing::info!("Processing chunk {} from generator", chunk_count + 1);

                // Extract RuntimeData using same logic as process_runtime_data
                let data_type: String = py_item.call_method0("data_type")
                    .and_then(|v| v.extract())
                    .map_err(|e| {
                        Error::Execution(format!("Chunk {}: failed to get data_type: {}", chunk_count + 1, e))
                    })?;

                let runtime_data = match data_type.as_str() {
                    "audio" => {
                        let audio_tuple_opt = py_item.call_method0("as_audio")
                            .map_err(|e| Error::Execution(format!("Chunk {}: failed to call as_audio(): {}", chunk_count + 1, e)))?;

                        let audio_tuple_opt: Option<(Vec<u8>, u32, u32, String, u64)> = audio_tuple_opt.extract()
                            .map_err(|e| Error::Execution(format!("Chunk {}: failed to extract audio tuple: {}", chunk_count + 1, e)))?;

                        let (samples, sample_rate, channels, format_str, num_samples) = audio_tuple_opt
                            .ok_or_else(|| Error::Execution(format!("Chunk {}: as_audio() returned None", chunk_count + 1)))?;

                        let format = match format_str.as_str() {
                            "f32" => 0,
                            "i16" => 1,
                            "i32" => 2,
                            _ => 0,
                        };

                        tracing::info!("Chunk {}: audio with {} samples, {}Hz, {} channels",
                            chunk_count + 1, num_samples, sample_rate, channels);

                        RuntimeData::Audio(crate::grpc_service::generated::AudioBuffer {
                            samples,
                            sample_rate,
                            channels,
                            format,
                            num_samples,
                        })
                    }
                    "text" => {
                        let text_opt: Option<String> = py_item.call_method0("as_text")
                            .and_then(|v| v.extract())
                            .map_err(|e| Error::Execution(format!("Chunk {}: failed to extract text: {}", chunk_count + 1, e)))?;

                        let text = text_opt.ok_or_else(|| Error::Execution(format!("Chunk {}: as_text() returned None", chunk_count + 1)))?;
                        RuntimeData::Text(text)
                    }
                    _ => {
                        return Err(Error::Execution(format!("Chunk {}: unsupported data type: {}", chunk_count + 1, data_type)));
                    }
                };

                Ok((true, Some(runtime_data)))
            })?;

            // Check if generator is exhausted
            if !has_value {
                break;
            }

            // We now have the chunk outside of GIL context
            if let Some(runtime_data) = runtime_data_opt {
                chunk_count += 1;
                tracing::info!("Calling callback for chunk {} (GIL released)", chunk_count);

                // Call callback WITHOUT holding GIL - this allows Tokio to run
                callback(runtime_data)?;

                tracing::info!("Callback completed for chunk {}", chunk_count);

                // Yield to allow Tokio scheduler to run the send task
                tokio::task::yield_now().await;
            }
        }

        tracing::info!("Successfully processed {} chunks via callback", chunk_count);
        tracing::info!("========================================");
        Ok(chunk_count)
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

        // Initialize Python runtime lazily (WASM-compatible)
        #[cfg(target_family = "wasm")]
        {
            use std::sync::Once;
            static PYTHON_INIT: Once = Once::new();
            PYTHON_INIT.call_once(|| {
                tracing::info!("Initializing Python runtime for WASM");
                pyo3::prepare_freethreaded_python();

                // Increase recursion limit for WASM (browser has limited call stack)
                Python::with_gil(|py| {
                    let sys = py.import("sys").expect("Failed to import sys");
                    // Reduce from default 1000 to 100 to avoid browser stack overflow
                    sys.setattr("recursionlimit", 100)
                        .expect("Failed to set recursion limit");
                    tracing::info!("Set Python recursion limit to 100 for WASM");
                });
            });
        }

        // Acquire GIL and create node instance
        let instance = Python::with_gil(|py| -> PyResult<Py<PyAny>> {
            // Load the class
            let class = self.load_class(py)?;

            // Merge node_id into params for Python node instantiation
            let mut params_with_id = context.params.clone();
            if let Some(obj) = params_with_id.as_object_mut() {
                // Add node_id to existing params object
                obj.insert("node_id".to_string(), serde_json::Value::String(context.node_id.clone()));
            } else {
                // Create new params object with just node_id
                params_with_id = serde_json::json!({"node_id": context.node_id.clone()});
            }

            // Instantiate with parameters (including node_id)
            let instance = self.instantiate_node(py, &class, &params_with_id)?;

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

        // Check if this is a streaming node and initialize streaming mode
        if self.is_streaming {
            Python::with_gil(|py| -> PyResult<()> {
                let bound_instance = instance.bind(py);
                self.initialize_streaming(py, &bound_instance)?;
                Ok(())
            })
            .map_err(|e: PyErr| {
                Error::Execution(format!(
                    "Failed to initialize streaming mode for node {}: {}",
                    self.node_type, e
                ))
            })?;
        }

        self.instance = Some(instance);

        self.initialized = true;

        Ok(())
    }

    /// Process data through the CPython node
    ///
    /// Phase 1.10.4: Call node.process(data) and marshal results
    /// Phase 1.10.10: Support async generators for streaming nodes
    /// Returns a Vec with ONE result per call (true streaming - no collection)
    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        if !self.initialized {
            return Err(Error::Execution(format!(
                "CPython node {} not initialized",
                self.node_type
            )));
        }

        // Handle streaming nodes with persistent queue
        // Just feed inputs - don't poll until finish_streaming()
        if self.is_streaming && self.streaming_queue.is_some() {
            return Python::with_gil(|py| -> PyResult<Vec<Value>> {
                // Feed the input into the queue
                let queue = self.streaming_queue.as_ref().unwrap().bind(py);
                let py_input = json_to_python_with_cache(py, &input, self.py_cache.as_ref())?;
                self.feed_streaming_queue(py, queue, &py_input)?;

                // Don't poll the generator here!
                // The generator won't make progress until we poll it,
                // but if we poll now, the StreamingQueue will wait for more items.
                // Instead, we'll feed all inputs first, then poll during finish_streaming().
                Ok(vec![])
            })
            .map_err(|e: PyErr| {
                Error::Execution(format!(
                    "Failed to process streaming node {}: {}",
                    self.node_type, e
                ))
            });
        }

        // If we have an active generator (non-streaming mode), get the next item from it
        if let Some(gen) = &self.active_generator {
            return Python::with_gil(|py| -> PyResult<Vec<Value>> {
                let bound_gen = gen.bind(py);

                match self.get_next_from_generator(py, bound_gen)? {
                    Some(value) => Ok(vec![value]),
                    None => {
                        // Generator exhausted
                        tracing::info!("Async generator exhausted, returning empty");
                        Ok(vec![])
                    }
                }
            })
            .map_err(|e: PyErr| {
                Error::Execution(format!(
                    "Failed to get next item from generator in node {}: {}",
                    self.node_type, e
                ))
            });
        }

        let instance = self.instance.as_ref().ok_or_else(|| {
            Error::Execution(format!("CPython node {} has no instance", self.node_type))
        })?;

        tracing::info!("Processing data through CPython node: {}", self.node_type);

        // Acquire GIL and process data
        let result = Python::with_gil(|py| -> PyResult<(Option<Py<PyAny>>, Vec<Value>)> {
            // Get the bound instance
            let bound_instance = instance.bind(py);

            // Convert input, handling __pyobj__ references recursively if cache is available
            let py_input = json_to_python_with_cache(py, &input, self.py_cache.as_ref())?;

            // Check if this node's process method is an async generator function
            // (streaming node that expects async generator input)
            let is_streaming_node = self.process_is_async_gen_function(py, &bound_instance)?;

            // Prepare the input - wrap as async generator if needed
            let prepared_input = if is_streaming_node {
                tracing::info!("Node {} is a streaming node, wrapping input as async generator", self.node_type);
                self.wrap_input_as_async_generator(py, &py_input)?
            } else {
                py_input
            };

            // Call process(data)
            let py_result = bound_instance.call_method1("process", (prepared_input,))?;

            // Check if result is None (filtered out)
            if py_result.is_none() {
                return Ok((None, vec![]));
            }

            // Check if the result is an async generator (coroutine)
            // For streaming nodes, process() returns an async generator
            if self.is_async_generator(py, &py_result)? {
                tracing::info!("CPython node {} returned async generator (streaming node) - iterating all items", self.node_type);

                // Iterate the async generator completely and collect all items
                // This is the correct behavior: streaming nodes yield multiple items which become
                // multiple inputs for the next node in the pipeline
                let items = self.iterate_async_generator(py, &py_result)?;
                tracing::info!("CPython node {} async generator yielded {} items", self.node_type, items.len());
                Ok((None, items))
            } else if self.is_coroutine(py, &py_result)? {
                // Check if the result is a coroutine (async function result)
                tracing::info!("CPython node {} returned coroutine, awaiting it", self.node_type);

                // We need to await the coroutine
                let awaited_result = self.await_coroutine(py, &py_result)?;

                // Check if the awaited result is an async generator
                if self.is_async_generator(py, &awaited_result)? {
                    tracing::info!("Coroutine resolved to async generator - iterating all items");
                    let items = self.iterate_async_generator(py, &awaited_result)?;
                    tracing::info!("CPython node {} async generator (from coroutine) yielded {} items", self.node_type, items.len());
                    Ok((None, items))
                } else {
                    // Convert awaited result - cache if possible, otherwise serialize to JSON
                    let json_result = self.convert_output_to_json(py, &awaited_result)?;
                    Ok((None, vec![json_result]))
                }
            } else {
                // Non-streaming node - convert Python result and return it
                let json_result = self.convert_output_to_json(py, &py_result)?;
                Ok((None, vec![json_result]))
            }
        })
        .map_err(|e: PyErr| {
            Error::Execution(format!(
                "Failed to process data in CPython node {}: {}",
                self.node_type, e
            ))
        })?;

        // Store the generator if we got one
        let (maybe_generator, results) = result;
        if let Some(generator) = maybe_generator {
            self.active_generator = Some(generator);
            self.is_streaming = true;
        }
        Ok(results)
    }

    /// Cleanup the CPython node
    async fn cleanup(&mut self) -> Result<()> {
        if !self.initialized {
            tracing::info!(
                "CPython node {} not initialized, skipping cleanup",
                self.node_type
            );
            return Ok(());
        }

        // Finish the streaming queue if this is a streaming node
        if self.is_streaming && self.streaming_queue.is_some() && !self.streaming_finished {
            Python::with_gil(|py| -> PyResult<()> {
                let queue = self.streaming_queue.as_ref().unwrap().bind(py);
                self.finish_streaming_queue(py, queue)?;
                Ok(())
            })
            .map_err(|e: PyErr| {
                Error::Execution(format!(
                    "Failed to finish streaming queue for node {}: {}",
                    self.node_type, e
                ))
            })?;
            self.streaming_finished = true;

            // Pull any remaining items from the generator
            tracing::info!(
                "Draining remaining items from streaming node {}",
                self.node_type
            );
            loop {
                if let Some(gen) = &self.active_generator {
                    match Python::with_gil(|py| -> PyResult<Option<Value>> {
                        let bound_gen = gen.bind(py);
                        self.get_next_from_generator(py, bound_gen)
                    }) {
                        Ok(Some(_)) => continue, // Keep draining
                        _ => break,              // Generator exhausted or error
                    }
                } else {
                    break;
                }
            }
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

    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        if !self.is_streaming || self.streaming_queue.is_none() {
            return Ok(vec![]);
        }

        tracing::info!("Finishing streaming for node {}", self.node_type);

        // FIRST: Try to collect any outputs that were already yielded
        // The generator may have already processed the finish signal and yielded data
        let mut outputs = Vec::new();
        tracing::info!(
            "Pre-finish: checking for any already-yielded outputs from {}",
            self.node_type
        );
        if let Some(gen) = &self.active_generator {
            // Try a non-blocking poll first
            match Python::with_gil(|py| -> PyResult<Option<Value>> {
                let bound_gen = gen.bind(py);
                self.try_get_next_from_generator(py, bound_gen)
            }) {
                Ok(Some(value)) => {
                    tracing::info!(
                        "Found pre-yielded value from streaming node {}",
                        self.node_type
                    );
                    outputs.push(value);
                }
                _ => {}
            }
        }

        // Signal that no more inputs will be provided
        Python::with_gil(|py| -> PyResult<()> {
            let queue = self.streaming_queue.as_ref().unwrap().bind(py);
            self.finish_streaming_queue(py, queue)?;
            Ok(())
        })
        .map_err(|e: PyErr| {
            Error::Execution(format!(
                "Failed to finish streaming queue for node {}: {}",
                self.node_type, e
            ))
        })?;
        self.streaming_finished = true;

        // Collect all remaining outputs from the generator
        // IMPORTANT: After finishing the queue, the generator needs to be polled
        // for it to notice the stream ended and execute flush code.
        // Calling __anext__() will resume the generator and let it complete.
        tracing::info!(
            "Collecting remaining outputs from streaming node {}",
            self.node_type
        );
        loop {
            if let Some(gen) = &self.active_generator {
                tracing::info!("Polling async generator for node: {}", self.node_type);
                match Python::with_gil(|py| -> PyResult<Option<Value>> {
                    let bound_gen = gen.bind(py);
                    // Call get_next which will resume the generator and execute flush code
                    self.get_next_from_generator(py, bound_gen)
                }) {
                    Ok(Some(value)) => {
                        tracing::info!(
                            "Streaming node {} yielded value during flush",
                            self.node_type
                        );
                        outputs.push(value);
                        // Continue polling in case there are more items
                    }
                    Ok(None) => {
                        tracing::info!("Async generator exhausted for node {}", self.node_type);
                        break; // Generator exhausted
                    }
                    Err(e) => {
                        return Err(Error::Execution(format!(
                            "Error collecting streaming outputs from node {}: {}",
                            self.node_type, e
                        )));
                    }
                }
            } else {
                break;
            }
        }

        tracing::info!(
            "Streaming node {} finished, collected {} outputs",
            self.node_type,
            outputs.len()
        );
        Ok(outputs)
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
            let code = CString::new(
                r#"
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
"#,
            )
            .unwrap();
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
        assert_eq!(result, vec![serde_json::json!(15)]);

        let result2 = executor.process(serde_json::json!(10)).await.unwrap();
        assert_eq!(result2, vec![serde_json::json!(30)]);

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
            let code = CString::new(
                r#"
class MinimalNode:
    def process(self, data):
        return {"result": data}
"#,
            )
            .unwrap();
            py.run(&code, None, None).unwrap();

            let sys = py.import("sys").unwrap();
            let modules = sys.getattr("modules").unwrap();

            let mock_code = CString::new(
                "import types; mock_module = types.ModuleType('remotemedia.nodes'); mock_module.MinimalNode = MinimalNode"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();

            let mock_module = py
                .eval(&CString::new("mock_module").unwrap(), None, None)
                .unwrap();
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
        assert!(!result.is_empty());
        assert_eq!(result[0]["result"], "test");

        executor.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cpython_executor_async_generator() {
        pyo3::prepare_freethreaded_python();

        // Create a streaming node with async generator
        Python::with_gil(|py| {
            let code = CString::new(
                r#"
class StreamingNodeAsyncGen:
    def __init__(self):
        self.count = 0

    async def process(self, data):
        """Async generator that yields multiple results."""
        for i in range(3):
            yield {"index": i, "data": data, "multiplier": i * 2}
"#,
            )
            .unwrap();
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
        let result = executor
            .process(serde_json::json!("test_data"))
            .await
            .unwrap();
        assert!(!result.is_empty());

        // result is now Vec<Value>, check it directly
        assert_eq!(result.len(), 3);

        // Verify each yielded item
        assert_eq!(result[0]["index"], 0);
        assert_eq!(result[0]["data"], "test_data");
        assert_eq!(result[1]["index"], 1);
        assert_eq!(result[2]["index"], 2);
        assert_eq!(result[2]["multiplier"], 4);

        executor.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_cpython_executor_async_coroutine() {
        pyo3::prepare_freethreaded_python();

        // Create a node with async process method (coroutine, not generator)
        Python::with_gil(|py| {
            let code = CString::new(
                r#"
import asyncio

class AsyncNode:
    async def process(self, data):
        """Async function that returns a single result."""
        await asyncio.sleep(0)  # Simulate async work
        return {"result": data * 2, "async": True}
"#,
            )
            .unwrap();
            py.run(&code, None, None).unwrap();

            let sys = py.import("sys").unwrap();
            let modules = sys.getattr("modules").unwrap();

            let mock_code = CString::new(
                "import types; mock_module = types.ModuleType('remotemedia.nodes'); mock_module.AsyncNode = AsyncNode"
            ).unwrap();
            py.run(&mock_code, None, None).unwrap();

            let mock_module = py
                .eval(&CString::new("mock_module").unwrap(), None, None)
                .unwrap();
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
        assert!(!result.is_empty());

        let result_obj = &result[0];
        assert_eq!(result_obj["result"], 10);
        assert_eq!(result_obj["async"], true);

        executor.cleanup().await.unwrap();
    }
}
