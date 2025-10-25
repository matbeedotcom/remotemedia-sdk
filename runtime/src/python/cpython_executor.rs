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
use crate::python::marshal::{json_to_python, json_to_python_with_cache, python_to_json, python_to_json_with_cache};
use crate::executor::PyObjectCache;
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

    /// Call the initialize() method if it exists
    fn call_initialize(&self, py: Python, instance: &Bound<'_, PyAny>) -> PyResult<()> {
        // Check if the node has an initialize method
        if instance.hasattr("initialize")? {
            tracing::info!("Calling initialize() on CPython node: {}", self.node_type);

            // As of Phase 1 WASM compatibility update, initialize() is now synchronous
            // This eliminates asyncio dependency which is not fully supported in WASM
            instance.call_method0("initialize")?;

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
    fn feed_streaming_queue(&self, py: Python, queue: &Bound<'_, PyAny>, item: &Bound<'_, PyAny>) -> PyResult<()> {
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
        tracing::info!("Created new event loop for streaming node: {}", self.node_type);

        // Create the streaming queue
        let queue = self.create_async_queue(py)?;
        tracing::info!("Created streaming queue for node: {}", self.node_type);

        // Get the stream() method from the queue
        let stream_gen = queue.call_method0("stream")?;

        // Call node.process(stream) to start the generator
        let process_result = instance.call_method1("process", (stream_gen,))?;

        // The result should be an async generator
        if !self.is_async_generator(py, &process_result)? {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                format!("Streaming node {} process() must return async generator", self.node_type)
            ));
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
    fn await_coroutine<'py>(&self, py: Python<'py>, coro: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
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
    fn get_next_from_generator(&self, py: Python, async_gen: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
        tracing::info!("Getting next item from async generator for node: {}", self.node_type);

        // Use __anext__() to get the next item from the async generator
        let code = std::ffi::CString::new(r#"
async def _get_next(agen):
    """Get next item from async generator, or None if exhausted."""
    try:
        item = await agen.__anext__()
        return (True, item)
    except StopAsyncIteration:
        return (False, None)
"#).unwrap();

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

        tracing::info!("Got next item from async generator for node {}", self.node_type);

        Ok(Some(json_value))
    }

    /// Try to get next item from async generator without blocking
    /// Returns None if no item is immediately available
    fn try_get_next_from_generator(&self, py: Python, async_gen: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
        // Use asyncio.wait_for with a reasonable timeout to allow async code to run
        // Use 1 second timeout to ensure we catch any yields that happen during processing
        let code = std::ffi::CString::new(r#"
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
"#).unwrap();

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
    fn iterate_async_generator(&self, py: Python, async_gen: &Bound<'_, PyAny>) -> PyResult<Vec<Value>> {
        tracing::info!("Iterating async generator for node: {}", self.node_type);

        // We need to iterate through the async generator and convert each item
        // This requires running it in an asyncio event loop
        let code = std::ffi::CString::new(r#"
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
"#).unwrap();

        // Execute the helper function
        py.run(&code, None, None)?;

        // Get the helper function
        let locals_code = std::ffi::CString::new("locals()").unwrap();
        let locals = py.eval(&locals_code, None, None)?;
        let iterate_fn = locals.get_item("_iterate_async_gen")?;

        // Create a converter function that will be called for each item
        // This converts each item to JSON with caching BEFORE appending to the list
        let converter_code = std::ffi::CString::new(r#"
def _converter(item):
    """Identity function - just return the item as-is for now."""
    return item
"#).unwrap();

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

        tracing::info!("Converted {} items from async generator for node {}", json_items.len(), self.node_type);

        Ok(json_items)
    }

    /// Convert Python output to JSON, using cache for complex objects if available
    fn convert_output_to_json(&self, py: Python, py_obj: &Bound<'_, PyAny>) -> PyResult<Value> {
        // Use cache-aware conversion if cache is available
        python_to_json_with_cache(py, py_obj, self.py_cache.as_ref())
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
                    sys.setattr("recursionlimit", 100).expect("Failed to set recursion limit");
                    tracing::info!("Set Python recursion limit to 100 for WASM");
                });
            });
        }

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
            tracing::info!("CPython node {} not initialized, skipping cleanup", self.node_type);
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
            tracing::info!("Draining remaining items from streaming node {}", self.node_type);
            loop {
                if let Some(gen) = &self.active_generator {
                    match Python::with_gil(|py| -> PyResult<Option<Value>> {
                        let bound_gen = gen.bind(py);
                        self.get_next_from_generator(py, bound_gen)
                    }) {
                        Ok(Some(_)) => continue, // Keep draining
                        _ => break,  // Generator exhausted or error
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

    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        if !self.is_streaming || self.streaming_queue.is_none() {
            return Ok(vec![]);
        }

        tracing::info!("Finishing streaming for node {}", self.node_type);

        // FIRST: Try to collect any outputs that were already yielded
        // The generator may have already processed the finish signal and yielded data
        let mut outputs = Vec::new();
        tracing::info!("Pre-finish: checking for any already-yielded outputs from {}", self.node_type);
        if let Some(gen) = &self.active_generator {
            // Try a non-blocking poll first
            match Python::with_gil(|py| -> PyResult<Option<Value>> {
                let bound_gen = gen.bind(py);
                self.try_get_next_from_generator(py, bound_gen)
            }) {
                Ok(Some(value)) => {
                    tracing::info!("Found pre-yielded value from streaming node {}", self.node_type);
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
        tracing::info!("Collecting remaining outputs from streaming node {}", self.node_type);
        loop {
            if let Some(gen) = &self.active_generator {
                tracing::info!("Polling async generator for node: {}", self.node_type);
                match Python::with_gil(|py| -> PyResult<Option<Value>> {
                    let bound_gen = gen.bind(py);
                    // Call get_next which will resume the generator and execute flush code
                    self.get_next_from_generator(py, bound_gen)
                }) {
                    Ok(Some(value)) => {
                        tracing::info!("Streaming node {} yielded value during flush", self.node_type);
                        outputs.push(value);
                        // Continue polling in case there are more items
                    }
                    Ok(None) => {
                        tracing::info!("Async generator exhausted for node {}", self.node_type);
                        break;  // Generator exhausted
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

        tracing::info!("Streaming node {} finished, collected {} outputs", self.node_type, outputs.len());
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
