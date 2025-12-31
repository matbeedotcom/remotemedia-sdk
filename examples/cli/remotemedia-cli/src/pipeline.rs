//! Pipeline execution module
//!
//! Provides integration with runtime-core's PipelineRunner for
//! executing pipelines from the CLI.

use anyhow::{Context, Result};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::manifest::Manifest;
use remotemedia_runtime_core::transport::{PipelineExecutor, TransportData};
use std::collections::HashMap;
use std::sync::Arc;

use crate::pipeline_nodes::get_cli_node_factories;

/// Initialize a PipelineExecutor instance
///
/// Creates a new PipelineExecutor with all built-in nodes registered.
/// This is a relatively expensive operation, so the executor should be
/// reused across multiple pipeline executions when possible.
pub fn create_runner() -> Result<PipelineExecutor> {
    PipelineExecutor::new().map_err(|e| anyhow::anyhow!("Failed to create pipeline executor: {}", e))
}

/// Initialize a PipelineExecutor with CLI-specific nodes (MicInput, SpeakerOutput, etc.)
///
/// Creates a new PipelineExecutor with both built-in and CLI-specific streaming nodes.
/// Use this when your pipeline needs audio I/O nodes.
///
/// # Examples
///
/// ```ignore
/// use remotemedia_cli::pipeline::create_runner_with_cli_nodes;
///
/// let runner = create_runner_with_cli_nodes().await?;
/// // Now you can use MicInput, SpeakerOutput, SrtOutput nodes in pipelines
/// ```
pub async fn create_runner_with_cli_nodes() -> Result<PipelineExecutor> {
    let runner = PipelineExecutor::new()
        .map_err(|e| anyhow::anyhow!("Failed to create pipeline executor: {}", e))?;

    // Register CLI-specific streaming node factories
    for factory in get_cli_node_factories() {
        runner.register_factory(factory).await;
    }

    Ok(runner)
}

/// Parse a manifest from YAML content
///
/// Supports the standard RemoteMedia manifest format with:
/// - version (must be "v1")
/// - metadata.name
/// - nodes (list of node definitions)
/// - connections (list of node connections)
pub fn parse_manifest(content: &str) -> Result<Manifest> {
    // First try YAML parsing
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(content).context("Failed to parse manifest as YAML")?;

    // Convert to JSON for the manifest parser
    let json_str = serde_json::to_string(&yaml_value).context("Failed to convert YAML to JSON")?;

    // Parse into Manifest struct
    remotemedia_runtime_core::manifest::parse(&json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse manifest: {}", e))
}

/// Execute a pipeline with unary semantics (one input, one output)
///
/// # Arguments
/// * `runner` - The PipelineExecutor instance
/// * `manifest` - The parsed pipeline manifest
/// * `input` - Input data to feed to the first node
///
/// # Returns
/// The output from the last node in the pipeline
pub async fn execute_unary(
    runner: &PipelineExecutor,
    manifest: Arc<Manifest>,
    input: RuntimeData,
) -> Result<RuntimeData> {
    let transport_data = TransportData::new(input);

    let output = runner
        .execute_unary(manifest, transport_data)
        .await
        .map_err(|e| anyhow::anyhow!("Pipeline execution failed: {}", e))?;

    Ok(output.data)
}

/// Execute a pipeline with named inputs
///
/// Allows specifying inputs for specific nodes by ID.
///
/// # Arguments
/// * `runner` - The PipelineExecutor instance  
/// * `manifest` - The parsed pipeline manifest
/// * `inputs` - Map of node_id -> RuntimeData for input nodes
///
/// # Returns
/// Map of node_id -> RuntimeData for output nodes
pub async fn execute_with_inputs(
    runner: &PipelineExecutor,
    manifest: Arc<Manifest>,
    inputs: HashMap<String, RuntimeData>,
) -> Result<HashMap<String, RuntimeData>> {
    // For now, use the first input for unary execution
    // Full multi-input support would need executor changes
    let (first_node_id, first_input) = inputs
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No inputs provided"))?;

    let transport_data = TransportData::new(first_input);

    let output = runner
        .execute_unary(manifest, transport_data)
        .await
        .map_err(|e| anyhow::anyhow!("Pipeline execution failed: {}", e))?;

    // Return output mapped to the last node
    let mut outputs = HashMap::new();
    outputs.insert("output".to_string(), output.data);

    Ok(outputs)
}

/// Streaming session wrapper
///
/// Provides a convenient interface for streaming pipeline execution.
pub struct StreamingSession {
    handle: remotemedia_runtime_core::transport::SessionHandle,
}

impl StreamingSession {
    /// Create a new streaming session
    pub async fn new(runner: &PipelineExecutor, manifest: Arc<Manifest>) -> Result<Self> {
        let handle = runner
            .create_session(manifest)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create streaming session: {}", e))?;

        Ok(Self { handle })
    }

    /// Send input data to the pipeline
    pub async fn send(&mut self, data: RuntimeData) -> Result<()> {
        let transport_data = TransportData::new(data);
        self.handle
            .send_input(transport_data)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send data: {}", e))
    }

    /// Receive output from the pipeline
    ///
    /// Returns None if the session has ended.
    pub async fn recv(&mut self) -> Result<Option<RuntimeData>> {
        match self.handle.recv_output().await {
            Ok(Some(transport_data)) => Ok(Some(transport_data.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to receive data: {}", e)),
        }
    }

    /// Try to receive output from the pipeline without blocking
    ///
    /// Returns None if no output is immediately available.
    pub fn try_recv(&mut self) -> Result<Option<RuntimeData>> {
        match self.handle.try_recv_output() {
            Ok(Some(transport_data)) => Ok(Some(transport_data.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Failed to receive data: {}", e)),
        }
    }

    /// Receive output with a timeout
    ///
    /// Returns None if no output is available within the timeout.
    pub async fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Option<RuntimeData>> {
        match tokio::time::timeout(timeout, self.handle.recv_output()).await {
            Ok(Ok(Some(transport_data))) => Ok(Some(transport_data.data)),
            Ok(Ok(None)) => Ok(None),
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to receive data: {}", e)),
            Err(_) => Ok(None), // Timeout - no data available
        }
    }

    /// Close the streaming session
    pub async fn close(mut self) -> Result<()> {
        self.handle
            .close()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to close session: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest_yaml() {
        let yaml = r#"
version: v1
metadata:
  name: test-pipeline
nodes:
  - id: input
    node_type: AudioInput
    params: {}
connections: []
"#;

        let manifest = parse_manifest(yaml).unwrap();
        assert_eq!(manifest.version, "v1");
        assert_eq!(manifest.metadata.name, "test-pipeline");
        assert_eq!(manifest.nodes.len(), 1);
    }
}
