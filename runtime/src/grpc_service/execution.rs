//! Unary RPC handler for pipeline execution
//!
//! Implements PipelineExecutionService trait for ExecutePipeline RPC.
//! Provides manifest-to-runtime conversion and result serialization.

#![cfg(feature = "grpc-transport")]

use crate::grpc_service::{
    auth::{check_auth, AuthConfig},
    generated::{
        pipeline_execution_service_server::PipelineExecutionService, ExecuteRequest,
        ExecuteResponse, ExecutionStatus, VersionRequest, VersionResponse,
        AudioBuffer as ProtoAudioBuffer, AudioFormat as ProtoAudioFormat,
        ErrorResponse, ErrorType, ExecutionMetrics as ProtoExecutionMetrics,
        NodeMetrics as ProtoNodeMetrics, NodeResult, NodeStatus,
        PipelineManifest as ProtoPipelineManifest, Connection as ProtoConnection,
        ExecutionResult as ProtoExecutionResult,
    },
    limits::ResourceLimits,
    metrics::ServiceMetrics,
    version::VersionManager,
    ServiceError,
};
use crate::{
    audio::AudioBuffer,
    executor::{Executor, ExecutorConfig},
    manifest::Manifest,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status};
use tracing::{error, info};

/// ExecutePipeline service implementation
pub struct ExecutionServiceImpl {
    auth_config: AuthConfig,
    limits: ResourceLimits,
    version: VersionManager,
    metrics: Arc<ServiceMetrics>,
    /// T042: Shared executor to avoid Python re-initialization overhead
    executor: Arc<Executor>,
}

impl ExecutionServiceImpl {
    /// Create new execution service
    pub fn new(
        auth_config: AuthConfig,
        limits: ResourceLimits,
        version: VersionManager,
        metrics: Arc<ServiceMetrics>,
    ) -> Self {
        // T042: Initialize Python and executor once at service creation
        let executor = Arc::new(Executor::new());
        info!("Initialized shared executor for request handling");
        
        Self {
            auth_config,
            limits,
            version,
            metrics,
            executor,
        }
    }

    /// Convert protobuf PipelineManifest to runtime Manifest
    fn deserialize_manifest(
        &self,
        proto_manifest: &ProtoPipelineManifest,
    ) -> Result<Manifest, ServiceError> {
        // Convert to JSON string for existing Manifest parser
        let json_str = serde_json::json!({
            "version": proto_manifest.version,
            "metadata": {
                "name": proto_manifest.metadata.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| "test".to_string()),
                "description": proto_manifest.metadata.as_ref().and_then(|m| Some(m.description.clone())),
                "created_at": proto_manifest.metadata.as_ref().and_then(|m| Some(m.created_at.clone()))
            },
            "nodes": proto_manifest.nodes.iter().map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "node_type": n.node_type,
                    "params": serde_json::from_str::<serde_json::Value>(&n.params)
                        .unwrap_or(serde_json::json!({})),
                    "runtime_hint": match n.runtime_hint {
                        0 => "auto", // Unspecified -> Auto
                        1 => "rust_python",
                        2 => "cpython",
                        3 => "cpython_wasm",
                        4 => "auto",
                        _ => "auto",
                    }
                })
            }).collect::<Vec<_>>(),
            "connections": proto_manifest.connections.iter().map(|c| {
                serde_json::json!({
                    "from": c.from,
                    "to": c.to
                })
            }).collect::<Vec<_>>()
        })
        .to_string();

        serde_json::from_str(&json_str).map_err(|e| {
            ServiceError::Validation(format!("Failed to parse manifest: {}", e))
        })
    }

    /// Convert protobuf AudioBuffer to runtime AudioBuffer
    fn convert_audio_buffer(
        &self,
        proto_buffer: &ProtoAudioBuffer,
    ) -> Result<AudioBuffer, ServiceError> {
        // Validate buffer size
        self.limits
            .check_audio_samples(proto_buffer.num_samples)
            .map_err(|e| ServiceError::ResourceLimit(e.to_string()))?;

        // Convert format and decode bytes to f32 samples
        let samples: Vec<f32> = match ProtoAudioFormat::try_from(proto_buffer.format) {
            Ok(ProtoAudioFormat::F32) => {
                // Convert bytes to f32 (assumes little-endian)
                proto_buffer
                    .samples
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect()
            }
            Ok(ProtoAudioFormat::I16) => {
                // Convert bytes to i16, then to f32 normalized (-1.0 to 1.0)
                proto_buffer
                    .samples
                    .chunks_exact(2)
                    .map(|chunk| {
                        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                        sample as f32 / 32768.0
                    })
                    .collect()
            }
            Ok(ProtoAudioFormat::I32) => {
                // Convert bytes to i32, then to f32 normalized
                proto_buffer
                    .samples
                    .chunks_exact(4)
                    .map(|chunk| {
                        let sample = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                        sample as f32 / 2147483648.0
                    })
                    .collect()
            }
            _ => {
                return Err(ServiceError::Validation(format!(
                    "Unsupported audio format: {}",
                    proto_buffer.format
                )))
            }
        };

        // Determine format enum for AudioBuffer
        let format = match ProtoAudioFormat::try_from(proto_buffer.format) {
            Ok(ProtoAudioFormat::F32) => crate::audio::AudioFormat::F32,
            Ok(ProtoAudioFormat::I16) => crate::audio::AudioFormat::I16,
            Ok(ProtoAudioFormat::I32) => crate::audio::AudioFormat::I32,
            _ => crate::audio::AudioFormat::F32,
        };

        // Create runtime AudioBuffer
        Ok(AudioBuffer::new(
            Arc::new(samples),
            proto_buffer.sample_rate,
            proto_buffer.channels as u16,
            format,
        ))
    }

    /// Convert runtime AudioBuffer to protobuf AudioBuffer
    fn serialize_audio_buffer(&self, buffer: &AudioBuffer) -> ProtoAudioBuffer {
        let format = match buffer.format() {
            crate::audio::AudioFormat::F32 => ProtoAudioFormat::F32 as i32,
            crate::audio::AudioFormat::I16 => ProtoAudioFormat::I16 as i32,
            crate::audio::AudioFormat::I32 => ProtoAudioFormat::I32 as i32,
        };

        // Convert f32 samples to bytes based on format
        let samples: Vec<u8> = match buffer.format() {
            crate::audio::AudioFormat::F32 => {
                // Convert f32 to bytes (little-endian)
                buffer
                    .as_slice()
                    .iter()
                    .flat_map(|&sample| sample.to_le_bytes())
                    .collect()
            }
            crate::audio::AudioFormat::I16 => {
                // Convert f32 to i16, then to bytes
                buffer
                    .as_slice()
                    .iter()
                    .flat_map(|&sample| {
                        let i_sample = (sample * 32768.0).clamp(-32768.0, 32767.0) as i16;
                        i_sample.to_le_bytes()
                    })
                    .collect()
            }
            crate::audio::AudioFormat::I32 => {
                // Convert f32 to i32, then to bytes
                buffer
                    .as_slice()
                    .iter()
                    .flat_map(|&sample| {
                        let i_sample = (sample * 2147483648.0).clamp(-2147483648.0, 2147483647.0) as i32;
                        i_sample.to_le_bytes()
                    })
                    .collect()
            }
        };

        ProtoAudioBuffer {
            samples,
            sample_rate: buffer.sample_rate(),
            channels: buffer.channels() as u32,
            format,
            num_samples: buffer.len_samples() as u64,
        }
    }

    /// Validate manifest structure
    fn validate_manifest(
        &self,
        manifest: &Manifest,
    ) -> Result<(), ServiceError> {
        // Check version
        if manifest.version.is_empty() {
            return Err(ServiceError::Validation(
                "Manifest version is required".to_string(),
            ));
        }

        // Check for duplicate node IDs
        let mut seen_ids = std::collections::HashSet::new();
        for node in &manifest.nodes {
            if !seen_ids.insert(&node.id) {
                return Err(ServiceError::Validation(format!(
                    "Duplicate node ID: {}",
                    node.id
                )));
            }
        }

        // Validate node types are supported
        for node in &manifest.nodes {
            if !self.version.is_node_type_supported(&node.node_type) {
                return Err(ServiceError::Validation(format!(
                    "Unsupported node type '{}'. Supported types: {:?}",
                    node.node_type,
                    self.version.to_proto().supported_node_types
                )));
            }
        }

        // TODO: Check for cycles in connections (DAG validation)
        // This would require a topological sort implementation

        Ok(())
    }

    /// Collect execution metrics
    fn collect_metrics(&self, start_time: Instant, memory_used: u64) -> ProtoExecutionMetrics {
        let wall_time_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        ProtoExecutionMetrics {
            wall_time_ms,
            cpu_time_ms: wall_time_ms, // Simplified for now
            memory_used_bytes: memory_used,
            node_metrics: HashMap::new(), // TODO: Collect per-node metrics
            serialization_time_ms: 0.0,   // TODO: Measure serialization time
        }
    }
}

#[tonic::async_trait]
impl PipelineExecutionService for ExecutionServiceImpl {
    async fn execute_pipeline(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        let start_time = self.metrics.record_request_start("ExecutePipeline");

        // Check authentication
        check_auth(&request, &self.auth_config)?;

        let req = request.into_inner();

        // Validate request
        let proto_manifest = req
            .manifest
            .ok_or_else(|| Status::invalid_argument("Manifest is required"))?;

        // Deserialize manifest
        let manifest = match self.deserialize_manifest(&proto_manifest) {
            Ok(m) => m,
            Err(e) => {
                self.metrics
                    .record_request_end("ExecutePipeline", "error", start_time);
                self.metrics.record_error("validation");
                
                // T038: Return validation errors as Error outcome, not gRPC Status
                let error_response = ErrorResponse {
                    error_type: ErrorType::Validation as i32,
                    message: e.to_string(),
                    failing_node_id: String::new(),
                    context: "Manifest deserialization failed".to_string(),
                    stack_trace: String::new(),
                };
                
                let response = ExecuteResponse {
                    outcome: Some(crate::grpc_service::generated::execute_response::Outcome::Error(error_response)),
                };
                
                return Ok(Response::new(response));
            }
        };

        // Validate manifest
        if let Err(e) = self.validate_manifest(&manifest) {
            self.metrics
                .record_request_end("ExecutePipeline", "error", start_time);
            self.metrics.record_error("validation");
            
            // T038: Return validation errors as Error outcome, not gRPC Status
            let error_response = ErrorResponse {
                error_type: ErrorType::Validation as i32,
                message: e.to_string(),
                failing_node_id: String::new(),
                context: "Manifest validation failed".to_string(),
                stack_trace: String::new(),
            };
            
            let response = ExecuteResponse {
                outcome: Some(crate::grpc_service::generated::execute_response::Outcome::Error(error_response)),
            };
            
            return Ok(Response::new(response));
        }

        // Convert audio inputs
        let mut audio_inputs = HashMap::new();
        for (node_id, proto_buffer) in &req.audio_inputs {
            match self.convert_audio_buffer(proto_buffer) {
                Ok(buffer) => {
                    audio_inputs.insert(node_id.clone(), buffer);
                }
                Err(e) => {
                    self.metrics
                        .record_request_end("ExecutePipeline", "error", start_time);
                    self.metrics.record_error("validation");
                    
                    // T038: Return audio input conversion errors as Error outcome
                    let error_response = ErrorResponse {
                        error_type: ErrorType::Validation as i32,
                        message: format!("Audio input conversion failed for node '{}': {}", node_id, e),
                        failing_node_id: node_id.clone(),
                        context: "Audio buffer conversion failed".to_string(),
                        stack_trace: String::new(),
                    };
                    
                    let response = ExecuteResponse {
                        outcome: Some(crate::grpc_service::generated::execute_response::Outcome::Error(error_response)),
                    };
                    
                    return Ok(Response::new(response));
                }
            }
        }

        // T042: Use shared executor with per-request task spawning for true parallelism
        // Clone the Arc for the spawned task
        let executor = Arc::clone(&self.executor);
        
        // Store manifest metadata for logging before moving manifest
        let num_nodes = manifest.nodes.len();
        let num_connections = manifest.connections.len();
        
        // T038: Spawn each pipeline execution in its own tokio task
        // This enables true concurrent execution across CPU cores
        let execution_task = tokio::spawn(async move {
            // Convert audio_inputs HashMap to Vec<Value>
            let input_values: Vec<serde_json::Value> = audio_inputs
                .into_iter()
                .map(|(node_id, _buffer)| serde_json::json!({ "node_id": node_id }))
                .collect();

            executor.execute_with_input(&manifest, input_values).await
        });

        // Execute pipeline
        info!(
            nodes = num_nodes,
            connections = num_connections,
            "Executing pipeline in isolated task"
        );

        let exec_result = match execution_task.await {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                // Execution error
                error!(error = %e, "Pipeline execution failed");
                self.metrics
                    .record_request_end("ExecutePipeline", "error", start_time);
                self.metrics.record_error("execution");

                let error_response = ErrorResponse {
                    error_type: ErrorType::NodeExecution as i32,
                    message: e.to_string(),
                    failing_node_id: String::new(),
                    context: String::new(),
                    stack_trace: String::new(),
                };

                let response = ExecuteResponse {
                    outcome: Some(crate::grpc_service::generated::execute_response::Outcome::Error(error_response)),
                };

                self.metrics
                    .record_request_end("ExecutePipeline", "success", start_time);
                return Ok(Response::new(response));
            }
            Err(join_err) => {
                // Task panicked or was cancelled
                error!(error = %join_err, "Execution task failed");
                self.metrics
                    .record_request_end("ExecutePipeline", "error", start_time);
                self.metrics.record_error("execution");

                let error_response = ErrorResponse {
                    error_type: ErrorType::Internal as i32,
                    message: format!("Execution task failed: {}", join_err),
                    failing_node_id: String::new(),
                    context: String::new(),
                    stack_trace: String::new(),
                };

                let response = ExecuteResponse {
                    outcome: Some(crate::grpc_service::generated::execute_response::Outcome::Error(error_response)),
                };

                return Ok(Response::new(response));
            }
        };

        // Serialize audio outputs
        // ExecutionResult.outputs is a serde_json::Value containing the final pipeline outputs
        // For audio pipelines, this is typically the final node's audio output
        // TODO: Implement proper Value -> AudioBuffer extraction based on output schema
        let mut audio_outputs = HashMap::new();
        
        // For now, we acknowledge that output extraction needs pipeline-specific handling
        // The outputs Value structure depends on the node types in the pipeline
        info!(
            status = %exec_result.status,
            "Pipeline execution completed - output serialization requires pipeline-specific schema"
        );

        // Collect metrics
        let metrics = self.collect_metrics(start_time, 10_000_000); // TODO: Measure actual memory

        let exec_result_proto = ProtoExecutionResult {
            audio_outputs,
            data_outputs: HashMap::new(), // TODO: Support data outputs
            metrics: Some(metrics),
            node_results: vec![], // TODO: Include per-node results
            status: ExecutionStatus::Success as i32,
        };

        let response = ExecuteResponse {
            outcome: Some(crate::grpc_service::generated::execute_response::Outcome::Result(exec_result_proto)),
        };

        self.metrics
            .record_request_end("ExecutePipeline", "success", start_time);

        info!(
            outputs = if let Some(crate::grpc_service::generated::execute_response::Outcome::Result(ref r)) = response.outcome {
                r.audio_outputs.len()
            } else { 0 },
            "Pipeline execution completed"
        );

        Ok(Response::new(response))
    }

    async fn get_version(
        &self,
        _request: Request<VersionRequest>,
    ) -> Result<Response<VersionResponse>, Status> {
        let version_info = self.version.to_proto();
        Ok(Response::new(VersionResponse {
            version_info: Some(version_info),
            compatible: true,
            compatibility_message: String::from("Compatible"),
        }))
    }
}

