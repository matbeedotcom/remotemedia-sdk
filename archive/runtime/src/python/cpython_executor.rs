//! CPython Node Executor with Dedicated Worker Threads (CUDA-Safe)
//!
//! **DEPRECATED in v0.3.0**: This module is deprecated and will be removed in v0.4.0.
//! Use `MultiprocessExecutor` instead for all Python nodes.
//!
//! Each CPythonNodeExecutor runs in its own dedicated OS thread with persistent GIL.
//! This prevents CUDA tensor corruption from repeated GIL acquisition/release.
//!
//! **Migration Guide:**
//! Replace `CPythonNodeExecutor::new(node_type)` with:
//! ```rust,ignore
//! let config = MultiprocessConfig::from_default_file().unwrap_or_default();
//! MultiprocessExecutor::new(config)
//! ```

use crate::data::RuntimeData;
use crate::executor::PyObjectCache;
use crate::nodes::{NodeContext, NodeExecutor, NodeInfo};
use crate::python::marshal::json_to_python;
use crate::python::runtime_data_to_py_with_session;
use crate::{Error, Result};
use async_trait::async_trait;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;
use std::sync::mpsc as std_mpsc;

mod cpython_async;
mod cpython_runtime_data;
mod cpython_streaming;

use cpython_runtime_data::extract_runtime_data;

/// Sanitize params for logging by truncating large binary data
fn sanitize_params_for_logging(params: &Value) -> String {
    fn sanitize_value(val: &Value, depth: usize) -> Value {
        if depth > 5 {
            return Value::String("<max_depth_reached>".to_string());
        }

        match val {
            Value::Object(map) => {
                let mut sanitized = serde_json::Map::new();
                for (key, value) in map {
                    // Check if this is audio/binary data
                    if key == "samples" || key == "data" || key == "buffer" {
                        if let Some(obj) = value.as_object() {
                            if obj.contains_key("type")
                                && obj.get("type").and_then(|v| v.as_str()) == Some("Buffer")
                            {
                                // This is a Node.js Buffer - show size instead of all bytes
                                if let Some(data_array) = obj.get("data").and_then(|v| v.as_array())
                                {
                                    sanitized.insert(
                                        key.clone(),
                                        Value::String(format!(
                                            "<Buffer: {} bytes>",
                                            data_array.len()
                                        )),
                                    );
                                    continue;
                                }
                            }
                        }
                        // Also handle raw byte arrays
                        if let Some(arr) = value.as_array() {
                            if arr.len() > 100 {
                                sanitized.insert(
                                    key.clone(),
                                    Value::String(format!("<Array: {} items>", arr.len())),
                                );
                                continue;
                            }
                        }
                    }

                    // Recursively sanitize nested objects
                    sanitized.insert(key.clone(), sanitize_value(value, depth + 1));
                }
                Value::Object(sanitized)
            }
            Value::Array(arr) => {
                // Truncate large arrays
                if arr.len() > 10 {
                    Value::String(format!("<Array: {} items>", arr.len()))
                } else {
                    Value::Array(arr.iter().map(|v| sanitize_value(v, depth + 1)).collect())
                }
            }
            _ => val.clone(),
        }
    }

    serde_json::to_string_pretty(&sanitize_value(params, 0))
        .unwrap_or_else(|_| "<serialization_error>".to_string())
}

/// Commands sent to the dedicated Python worker thread
enum WorkerCommand {
    Initialize {
        params: Value,
        node_id: String,
        result_tx: std_mpsc::SyncSender<Result<()>>,
    },
    ProcessStreaming {
        input: RuntimeData,
        session_id: Option<String>,
        result_tx: tokio::sync::mpsc::UnboundedSender<Result<RuntimeData>>,
    },
    Cleanup {
        result_tx: std_mpsc::SyncSender<Result<()>>,
    },
    Shutdown,
}

/// CPython-based node executor with dedicated thread and persistent GIL (CUDA-safe)
///
/// **DEPRECATED**: Use `MultiprocessExecutor` instead for true process isolation and better parallelism.
///
/// Architecture:
///   Rust async → channel → Dedicated OS thread (holds GIL persistently) → channel → Rust async
///
/// Benefits:
///   - CUDA-safe: Same Python thread state for entire node lifetime
///   - Each node isolated: LFM2Audio and VibeVoice run in separate threads
///   - No GIL contention: Each thread has its own Python context
#[deprecated(
    since = "0.3.0",
    note = "Use MultiprocessExecutor instead. This will be removed in v0.4.0"
)]
pub struct CPythonNodeExecutor {
    node_type: String,
    initialized: bool,
    is_streaming: bool,

    // Dedicated worker thread with persistent GIL
    worker_thread: Option<std::thread::JoinHandle<()>>,
    command_tx: Option<std_mpsc::SyncSender<WorkerCommand>>,
}

impl CPythonNodeExecutor {
    pub fn new(node_type: impl Into<String>) -> Self {
        let node_type_str = node_type.into();

        // Create command channel
        let (command_tx, command_rx) = std_mpsc::sync_channel::<WorkerCommand>(10);

        // Spawn dedicated worker thread with persistent GIL
        let node_type_clone = node_type_str.clone();
        let worker_thread = std::thread::spawn(move || {
            Self::worker_thread_main(node_type_clone, command_rx);
        });

        Self {
            node_type: node_type_str,
            initialized: false,
            is_streaming: false,
            worker_thread: Some(worker_thread),
            command_tx: Some(command_tx),
        }
    }

    pub fn new_with_cache(node_type: impl Into<String>, _py_cache: PyObjectCache) -> Self {
        // TODO: Pass cache to worker thread if needed
        Self::new(node_type)
    }

    pub fn set_is_streaming_node(&mut self, is_streaming: bool) {
        self.is_streaming = is_streaming;
    }

    /// Worker thread main loop - holds GIL persistently (CUDA-safe!)
    ///
    /// This thread maintains a SINGLE Python thread state for its entire lifetime,
    /// preventing CUDA tensor metadata corruption from GIL churn.
    fn worker_thread_main(node_type: String, command_rx: std_mpsc::Receiver<WorkerCommand>) {
        tracing::info!("[Worker-{}] Starting dedicated Python thread", node_type);

        // Hold GIL for entire thread lifetime (CUDA-safe: same thread state always!)
        Python::attach(|py| {
            tracing::info!(
                "[Worker-{}] GIL acquired persistently for thread lifetime",
                node_type
            );

            let mut instance: Option<Py<PyAny>> = None;
            let mut event_loop: Option<Py<PyAny>> = None;

            // Process commands in this thread's persistent Python context
            while let Ok(command) = command_rx.recv() {
                match command {
                    WorkerCommand::Initialize {
                        params,
                        node_id,
                        result_tx,
                    } => {
                        let result = Self::handle_initialize(
                            py,
                            &node_type,
                            &params,
                            &node_id,
                            &mut instance,
                            &mut event_loop,
                        );
                        let _ = result_tx.send(result);
                    }
                    WorkerCommand::ProcessStreaming {
                        input,
                        session_id,
                        result_tx,
                    } => {
                        if let (Some(ref inst), Some(ref evloop)) = (&instance, &event_loop) {
                            Self::handle_process_streaming(
                                py, inst, evloop, input, session_id, result_tx,
                            );
                        } else {
                            let _ = result_tx
                                .send(Err(Error::Execution("Node not initialized".to_string())));
                        }
                    }
                    WorkerCommand::Cleanup { result_tx } => {
                        if let Some(ref inst) = instance {
                            let result = Self::handle_cleanup(py, inst);
                            let _ = result_tx.send(result);
                        }
                        instance = None;
                        event_loop = None;
                    }
                    WorkerCommand::Shutdown => {
                        tracing::info!("[Worker-{}] Shutdown command received", node_type);
                        break;
                    }
                }
            }

            tracing::info!(
                "[Worker-{}] Thread exiting, releasing persistent GIL",
                node_type
            );
        });
    }

    /// Handle initialization in worker thread (called with persistent GIL)
    fn handle_initialize(
        py: Python,
        node_type: &str,
        params: &Value,
        node_id: &str,
        instance: &mut Option<Py<PyAny>>,
        event_loop: &mut Option<Py<PyAny>>,
    ) -> Result<()> {
        tracing::info!("[Worker-{}] Initializing node: {}", node_type, node_id);

        // Load class
        let nodes_module = py
            .import("remotemedia.nodes")
            .map_err(|e| Error::Execution(format!("Failed to import remotemedia.nodes: {}", e)))?;
        let class = nodes_module
            .getattr(node_type)
            .map_err(|e| Error::Execution(format!("Failed to get class {}: {}", node_type, e)))?;

        // Merge node_id into params
        let mut params_with_id = params.clone();
        if let Some(obj) = params_with_id.as_object_mut() {
            obj.insert("node_id".to_string(), Value::String(node_id.to_string()));
        } else {
            params_with_id = serde_json::json!({"node_id": node_id});
        }

        // Create instance
        let py_params = json_to_python(py, &params_with_id)
            .map_err(|e| Error::Execution(format!("Failed to convert params: {}", e)))?;
        let kwargs = py_params
            .downcast::<PyDict>()
            .map_err(|e| Error::Execution(format!("Params not a dict: {}", e)))?;

        let inst = class
            .call((), Some(kwargs))
            .map_err(|e| Error::Execution(format!("Failed to instantiate: {}", e)))?;

        // Log params but sanitize large binary data to avoid verbose output
        let sanitized_params = sanitize_params_for_logging(&params_with_id);
        tracing::info!(
            "Instantiated CPython node: {} with params: {}",
            node_type,
            sanitized_params
        );

        // Call initialize() if exists
        if inst
            .hasattr("initialize")
            .map_err(|e| Error::Execution(format!("hasattr failed: {}", e)))?
        {
            tracing::info!("Calling initialize() on CPython node: {}", node_type);

            let init_result = inst
                .call_method0("initialize")
                .map_err(|e| Error::Execution(format!("initialize() failed: {}", e)))?;

            // Check if it's a coroutine
            let inspect = py
                .import("inspect")
                .map_err(|e| Error::Execution(format!("Failed to import inspect: {}", e)))?;
            let is_coroutine = inspect
                .call_method1("iscoroutine", (&init_result,))
                .and_then(|v| v.extract::<bool>())
                .map_err(|e| Error::Execution(format!("Failed to check coroutine: {}", e)))?;

            if is_coroutine {
                // Create event loop if not exists
                if event_loop.is_none() {
                    let asyncio = py.import("asyncio").map_err(|e| {
                        Error::Execution(format!("Failed to import asyncio: {}", e))
                    })?;
                    let loop_obj = asyncio.call_method0("new_event_loop").map_err(|e| {
                        Error::Execution(format!("Failed to create event loop: {}", e))
                    })?;
                    asyncio
                        .call_method1("set_event_loop", (&loop_obj,))
                        .map_err(|e| {
                            Error::Execution(format!("Failed to set event loop: {}", e))
                        })?;
                    *event_loop = Some(loop_obj.unbind());
                }

                // Run async initialize
                if let Some(ref evloop_py) = event_loop {
                    let evloop = evloop_py.bind(py);
                    evloop
                        .call_method1("run_until_complete", (init_result,))
                        .map_err(|e| Error::Execution(format!("Async initialize failed: {}", e)))?;
                }
            }
        }

        // Ensure event loop exists
        if event_loop.is_none() {
            let asyncio = py
                .import("asyncio")
                .map_err(|e| Error::Execution(format!("Failed to import asyncio: {}", e)))?;
            let loop_obj = asyncio
                .call_method0("new_event_loop")
                .map_err(|e| Error::Execution(format!("Failed to create event loop: {}", e)))?;
            asyncio
                .call_method1("set_event_loop", (&loop_obj,))
                .map_err(|e| Error::Execution(format!("Failed to set event loop: {}", e)))?;
            *event_loop = Some(loop_obj.unbind());
        }

        *instance = Some(inst.unbind());

        tracing::info!("CPython node {} initialized", node_type);
        Ok(())
    }

    /// Handle streaming in worker thread (called with persistent GIL - CUDA-safe!)
    fn handle_process_streaming(
        py: Python,
        instance: &Py<PyAny>,
        event_loop: &Py<PyAny>,
        input: RuntimeData,
        session_id: Option<String>,
        result_tx: tokio::sync::mpsc::UnboundedSender<Result<RuntimeData>>,
    ) {
        tracing::debug!("[Worker] Processing streaming request");

        let inst = instance.bind(py);

        // Convert input to Python
        let py_runtime_data_struct = runtime_data_to_py_with_session(input, session_id);
        let py_runtime_data = match Py::new(py, py_runtime_data_struct) {
            Ok(data) => data,
            Err(e) => {
                let _ = result_tx.send(Err(Error::Execution(format!(
                    "Failed to create RuntimeData: {}",
                    e
                ))));
                return;
            }
        };

        // Call process() to get async generator
        let process_result = match inst.call_method1("process", (py_runtime_data,)) {
            Ok(res) => res.unbind(), // Unbind for reuse across iterations
            Err(e) => {
                let _ = result_tx.send(Err(Error::Execution(format!("process() failed: {}", e))));
                return;
            }
        };

        // Define helper to get next item (REAL-TIME streaming, no buffering!)
        let code = std::ffi::CString::new(
            r#"
async def _get_next_item(agen):
    """Get next item from async generator without buffering."""
    import sys
    try:
        item = await anext(agen)
        sys.stdout.flush()
        return (True, item)
    except StopAsyncIteration:
        sys.stdout.flush()
        return (False, None)
    except Exception as e:
        print(f"[Worker] Error getting next item: {e}", flush=True)
        import traceback
        traceback.print_exc()
        raise
"#,
        )
        .unwrap();

        if let Err(e) = py.run(&code, None, None) {
            let _ = result_tx.send(Err(Error::Execution(format!(
                "Failed to define get_next: {}",
                e
            ))));
            return;
        }

        let locals = match py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None) {
            Ok(l) => l,
            Err(e) => {
                let _ = result_tx.send(Err(Error::Execution(format!(
                    "Failed to get locals: {}",
                    e
                ))));
                return;
            }
        };

        let get_next_fn = match locals.get_item("_get_next_item") {
            Ok(f) => f,
            Err(e) => {
                let _ = result_tx.send(Err(Error::Execution(format!(
                    "Failed to get get_next: {}",
                    e
                ))));
                return;
            }
        };

        let evloop = event_loop.bind(py);

        // REAL-TIME ITERATION: Get and send each item immediately (no buffering)
        let mut count = 0;
        loop {
            count += 1;

            // Bind generator for this iteration
            let gen = process_result.bind(py);

            // Get next item
            let get_next_coro = match get_next_fn.call1((gen,)) {
                Ok(c) => c,
                Err(e) => {
                    let _ = result_tx.send(Err(Error::Execution(format!(
                        "Failed to call get_next: {}",
                        e
                    ))));
                    return;
                }
            };

            let result_tuple = match evloop.call_method1("run_until_complete", (get_next_coro,)) {
                Ok(t) => t,
                Err(e) => {
                    let _ = result_tx.send(Err(Error::Execution(format!("anext failed: {}", e))));
                    return;
                }
            };

            // Check if exhausted
            let has_value: bool = match result_tuple.get_item(0).and_then(|v| v.extract()) {
                Ok(v) => v,
                Err(e) => {
                    let _ = result_tx.send(Err(Error::Execution(format!(
                        "Failed to extract has_value: {}",
                        e
                    ))));
                    return;
                }
            };

            if !has_value {
                tracing::debug!("[Worker] Generator exhausted after {} items", count - 1);
                break;
            }

            // Extract and send item IMMEDIATELY (real-time!)
            let py_item = match result_tuple.get_item(1) {
                Ok(item) => item,
                Err(e) => {
                    let _ =
                        result_tx.send(Err(Error::Execution(format!("Failed to get item: {}", e))));
                    return;
                }
            };

            match extract_runtime_data(py, &py_item) {
                Ok(runtime_data) => {
                    if result_tx.send(Ok(runtime_data)).is_err() {
                        tracing::warn!("[Worker] Channel closed, stopping");
                        return;
                    }
                }
                Err(e) => {
                    let _ = result_tx.send(Err(e));
                    return;
                }
            }

            if count % 10 == 0 {
                tracing::debug!("[Worker] Streamed {} items in real-time", count);
            }
        }

        tracing::debug!("[Worker] Streaming complete: {} items", count - 1);
    }

    /// Handle cleanup in worker thread
    fn handle_cleanup(py: Python, instance: &Py<PyAny>) -> Result<()> {
        let inst = instance.bind(py);

        if inst.hasattr("cleanup").unwrap_or(false) {
            tracing::info!("Calling cleanup() on CPython node");
            inst.call_method0("cleanup")
                .map_err(|e| Error::Execution(format!("cleanup() failed: {}", e)))?;
        }

        Ok(())
    }

    /// Process RuntimeData with streaming callback using dedicated worker thread
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

        // Create result channel
        let (result_tx, mut result_rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<RuntimeData>>();

        // Send processing command to dedicated worker thread
        let command = WorkerCommand::ProcessStreaming {
            input,
            session_id,
            result_tx,
        };

        self.command_tx
            .as_ref()
            .ok_or_else(|| Error::Execution("Worker thread not available".to_string()))?
            .send(command)
            .map_err(|e| Error::Execution(format!("Failed to send command to worker: {}", e)))?;

        // Receive results from worker thread and call callback
        let mut chunk_count = 0;

        while let Some(result) = result_rx.recv().await {
            match result {
                Ok(runtime_data) => {
                    chunk_count += 1;
                    tracing::info!(
                        "Yielded item {}: type={:?}",
                        chunk_count,
                        runtime_data.data_type()
                    );
                    tracing::info!("📤 Calling callback for chunk {}", chunk_count);

                    callback(runtime_data)?;

                    tracing::info!(
                        "✅ Callback completed successfully for item {}",
                        chunk_count
                    );
                    tokio::task::yield_now().await;
                }
                Err(e) => {
                    tracing::error!("Error from Python worker: {:?}", e);
                    return Err(e);
                }
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

        // Send initialize command to worker thread
        let (result_tx, result_rx) = std_mpsc::sync_channel::<Result<()>>(1);

        let command = WorkerCommand::Initialize {
            params: context.params.clone(),
            node_id: context.node_id.clone(),
            result_tx,
        };

        self.command_tx
            .as_ref()
            .ok_or_else(|| Error::Execution("Worker thread not available".to_string()))?
            .send(command)
            .map_err(|e| Error::Execution(format!("Failed to send init command: {}", e)))?;

        // Wait for initialization result
        let result = result_rx
            .recv()
            .map_err(|e| Error::Execution(format!("Worker thread closed: {}", e)))??;

        self.initialized = true;
        Ok(result)
    }

    async fn process(&mut self, _input: Value) -> Result<Vec<Value>> {
        Err(Error::Execution(
            "Non-streaming process not implemented for worker architecture".to_string(),
        ))
    }

    async fn cleanup(&mut self) -> Result<()> {
        if !self.initialized {
            return Ok(());
        }

        tracing::info!("Cleaning up CPython node: {}", self.node_type);

        // Send cleanup command
        let (result_tx, result_rx) = std_mpsc::sync_channel::<Result<()>>(1);

        let command = WorkerCommand::Cleanup { result_tx };

        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(command);
            let _ = result_rx.recv();
        }

        self.initialized = false;

        tracing::info!("CPython node {} cleanup complete", self.node_type);
        Ok(())
    }

    fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn info(&self) -> NodeInfo {
        NodeInfo {
            name: format!("CPython({})", self.node_type),
            version: "1.0.0".to_string(),
            description: Some(format!(
                "CPython-based executor for {} node (dedicated thread)",
                self.node_type
            )),
        }
    }
}

impl Drop for CPythonNodeExecutor {
    fn drop(&mut self) {
        // Send shutdown command to worker thread
        if let Some(ref tx) = self.command_tx {
            let _ = tx.send(WorkerCommand::Shutdown);
        }

        // Wait for worker thread to exit
        if let Some(handle) = self.worker_thread.take() {
            tracing::info!("[{}] Waiting for worker thread to exit", self.node_type);
            let _ = handle.join();
            tracing::info!("[{}] Worker thread exited", self.node_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cpython_executor_creation() {
        let executor = CPythonNodeExecutor::new("PassThroughNode");
        assert_eq!(executor.node_type, "PassThroughNode");
        assert!(!executor.initialized);
    }
}
