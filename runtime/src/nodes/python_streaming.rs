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
        // Ensure the Python node is initialized
        self.ensure_initialized().await?;

        // Get the executor and process through it
        let mut executor_guard = self.executor.lock().await;
        let executor = executor_guard.as_mut()
            .ok_or_else(|| Error::Execution("Python node not initialized".to_string()))?;

        // Downcast to CPythonNodeExecutor to access process_runtime_data
        let py_executor = executor.as_any_mut()
            .downcast_mut::<crate::python::CPythonNodeExecutor>()
            .ok_or_else(|| Error::Execution("Failed to downcast to CPythonNodeExecutor".to_string()))?;

        // Call the new runtime data method
        py_executor.process_runtime_data(data).await
    }
}

impl PythonStreamingNode {
    /// Process input and stream chunks via callback
    ///
    /// This method is used for true streaming where one input can produce multiple outputs.
    /// It iterates the Python async generator and calls the callback for each chunk as it arrives.
    pub async fn process_streaming<F>(&self, data: RuntimeData, callback: F) -> Result<usize, Error>
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

        // Call the new streaming method with callback
        py_executor.process_runtime_data_streaming(data, callback).await
    }
}
