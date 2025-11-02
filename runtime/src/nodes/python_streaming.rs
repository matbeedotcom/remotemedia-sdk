//! Python StreamingNode wrapper
//!
//! This module provides a wrapper that allows Python nodes to participate
//! in streaming pipelines via the AsyncStreamingNode trait.

use crate::data::RuntimeData;
use crate::nodes::{AsyncStreamingNode, NodeExecutor, NodeContext};
use crate::Error;
use pyo3::Python;
use serde_json::Value;
use tokio::sync::Mutex;

/// Wrapper that adapts a Python node to the AsyncStreamingNode trait
pub struct PythonStreamingNode {
    node_id: String,
    node_type: String,
    params: Value,
    executor: Mutex<Option<Box<dyn NodeExecutor>>>,
}

impl PythonStreamingNode {
    /// Create a new Python streaming node
    pub fn new(node_id: String, node_type: &str, params: &Value) -> Result<Self, Error> {
        Ok(Self {
            node_id,
            node_type: node_type.to_string(),
            params: params.clone(),
            executor: Mutex::new(None),
        })
    }

    /// Create from an existing executor (to reuse cached Python instances)
    pub fn from_executor(node_id: String, node_type: &str, params: &Value, executor: Box<dyn NodeExecutor>) -> Result<Self, Error> {
        Ok(Self {
            node_id,
            node_type: node_type.to_string(),
            params: params.clone(),
            executor: Mutex::new(Some(executor)),
        })
    }

    pub async fn ensure_initialized(&self) -> Result<(), Error> {
        let mut executor_guard = self.executor.lock().await;
        if executor_guard.is_none() {
            // Create the Python executor
            let mut py_executor: Box<dyn NodeExecutor> =
                Box::new(crate::python::CPythonNodeExecutor::new(&self.node_type));

            // Create context for initialization
            let context = NodeContext {
                node_id: self.node_id.clone(),
                node_type: self.node_type.clone(),
                params: self.params.clone(),
                session_id: None,
                metadata: std::collections::HashMap::new(),
            };

            // Initialize
            py_executor.initialize(&context).await?;

            *executor_guard = Some(py_executor);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for PythonStreamingNode {
    fn node_type(&self) -> &str {
        &self.node_type
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // IMPORTANT: For single-output nodes, use the streaming iteration and take first item
        // This avoids complex GIL deadlock issues with loop.run_until_complete()

        // Ensure the Python node is initialized
        self.ensure_initialized().await?;

        // Get the executor and process through it
        let mut executor_guard = self.executor.lock().await;
        let executor = executor_guard.as_mut()
            .ok_or_else(|| Error::Execution("Python node not initialized".to_string()))?;

        // Downcast to CPythonNodeExecutor
        let py_executor = executor.as_any_mut()
            .downcast_mut::<crate::python::CPythonNodeExecutor>()
            .ok_or_else(|| Error::Execution("Failed to downcast to CPythonNodeExecutor".to_string()))?;

        // Use streaming iteration but only take the first item
        let mut result: Option<RuntimeData> = None;
        py_executor.process_runtime_data_streaming(data, None, |output| {
            result = Some(output);
            // Return error to stop iteration after first item
            Err(crate::Error::Execution("StopIteration".to_string()))
        }).await.ok(); // Ignore the error since we use it to stop

        result.ok_or_else(|| Error::Execution("No output from Python node".to_string()))
    }

    // Override the trait's process_streaming method to use true streaming
    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        // Ensure the Python node is initialized
        self.ensure_initialized().await?;

        // Get the executor
        let mut executor_guard = self.executor.lock().await;
        let executor = executor_guard.as_mut()
            .ok_or_else(|| Error::Execution("Python node not initialized".to_string()))?;

        // Downcast to CPythonNodeExecutor
        let py_executor = executor.as_any_mut()
            .downcast_mut::<crate::python::CPythonNodeExecutor>()
            .ok_or_else(|| Error::Execution("Failed to downcast to CPythonNodeExecutor".to_string()))?;

        // Call the streaming method with callback and session_id
        py_executor.process_runtime_data_streaming(data, session_id, callback).await
    }
}

