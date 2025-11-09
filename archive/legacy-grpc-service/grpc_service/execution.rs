//! Unary RPC handler for pipeline execution
//!
//! Implements PipelineExecutionService trait for ExecutePipeline RPC.
//! Provides manifest-to-runtime conversion and result serialization.

#![cfg(feature = "grpc-transport")]

use crate::grpc_service::{
    auth::{check_auth, AuthConfig},
    executor_registry::{ExecutorRegistry, ExecutorType},
    generated::{
        pipeline_execution_service_server::PipelineExecutionService,
        AudioBuffer as ProtoAudioBuffer, AudioFormat as ProtoAudioFormat,
        Connection as ProtoConnection, ErrorResponse, ErrorType, ExecuteRequest, ExecuteResponse,
        ExecutionMetrics as ProtoExecutionMetrics, ExecutionResult as ProtoExecutionResult,
        ExecutionStatus, NodeMetrics as ProtoNodeMetrics, NodeResult, NodeStatus,
        PipelineManifest as ProtoPipelineManifest, VersionRequest, VersionResponse,
    },
    limits::ResourceLimits,
    metrics::ServiceMetrics,
    version::VersionManager,
    ServiceError,
};
use crate::{
    audio::AudioBuffer,
    executor::{executor_bridge::*, Executor, ExecutorConfig},
    manifest::Manifest,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
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
    /// Executor registry for routing nodes to appropriate executors
    executor_registry: Arc<ExecutorRegistry>,
    /// Multiprocess executor for Python nodes
    #[cfg(feature = "multiprocess")]
    multiprocess_executor: Arc<crate::python::multiprocess::MultiprocessExecutor>,
}

impl ExecutionServiceImpl {
    /// Create new execution service with pre-configured executor
    #[cfg(feature = "multiprocess")]
    pub fn new(
        auth_config: AuthConfig,
        limits: ResourceLimits,
        version: VersionManager,
        metrics: Arc<ServiceMetrics>,
        executor: Arc<Executor>,
        executor_registry: Arc<ExecutorRegistry>,
        multiprocess_executor: Arc<crate::python::multiprocess::MultiprocessExecutor>,
    ) -> Self {
        // T042: Use shared executor with registered nodes from server initialization
        info!("Using shared executor for request handling (nodes already registered)");
        info!("Multiprocess executor enabled for Python nodes");

        Self {
            auth_config,
            limits,
            version,
            metrics,
            executor,
            executor_registry,
            multiprocess_executor,
        }
    }

    /// Create new execution service (non-multiprocess version)
    #[cfg(not(feature = "multiprocess"))]
    pub fn new(
        auth_config: AuthConfig,
        limits: ResourceLimits,
        version: VersionManager,
        metrics: Arc<ServiceMetrics>,
        executor: Arc<Executor>,
        executor_registry: Arc<ExecutorRegistry>,
    ) -> Self {
        info!("Using shared executor for request handling (nodes already registered)");

        Self {
            auth_config,
            limits,
            version,
            metrics,
            executor,
            executor_registry,
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

        serde_json::from_str(&json_str)
            .map_err(|e| ServiceError::Validation(format!("Failed to parse manifest: {}", e)))
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
                        let i_sample =
                            (sample * 2147483648.0).clamp(-2147483648.0, 2147483647.0) as i32;
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
    fn validate_manifest(&self, manifest: &Manifest) -> Result<(), ServiceError> {
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
            proto_to_runtime_ms: 0.0,     // TODO: Track conversion time
            runtime_to_proto_ms: 0.0,     // TODO: Track conversion time
            data_type_breakdown: HashMap::new(), // TODO: Track data types
        }
    }
}

/// Session execution context for managing executor instances and node assignments
///
/// Public for testing purposes
pub struct SessionExecutionContext {
    /// Session identifier
    session_id: String,

    /// Node ID → Executor type assignments (interior mutability for Arc usage)
    node_assignments: RwLock<HashMap<String, ExecutorType>>,

    /// Executor bridges for this session
    executor_bridges: Arc<RwLock<HashMap<ExecutorType, Arc<dyn ExecutorBridge>>>>,

    /// Session creation time
    created_at: Instant,
}

impl SessionExecutionContext {
    /// Create a new session execution context
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            node_assignments: RwLock::new(HashMap::new()),
            executor_bridges: Arc::new(RwLock::new(HashMap::new())),
            created_at: Instant::now(),
        }
    }

    /// Assign a node to an executor type
    pub async fn assign_node(&self, node_id: String, executor_type: ExecutorType) {
        tracing::debug!(
            "Session {}: Assigning node '{}' to executor {:?}",
            self.session_id,
            node_id,
            executor_type
        );
        self.node_assignments
            .write()
            .await
            .insert(node_id, executor_type);
    }

    /// Get executor type for a node
    pub async fn get_node_executor(&self, node_id: &str) -> Option<ExecutorType> {
        self.node_assignments.read().await.get(node_id).copied()
    }

    /// Initialize executors for assigned nodes
    #[cfg(feature = "multiprocess")]
    pub async fn initialize_executors(
        &self,
        native_executor: Arc<Executor>,
        multiprocess_executor: Arc<crate::python::multiprocess::MultiprocessExecutor>,
    ) -> Result<(), ServiceError> {
        let mut bridges = self.executor_bridges.write().await;

        // Determine which executor types are needed
        let mut needs_native = false;
        let mut needs_multiprocess = false;

        for executor_type in self.node_assignments.read().await.values() {
            match executor_type {
                ExecutorType::Native => needs_native = true,
                #[cfg(feature = "multiprocess")]
                ExecutorType::Multiprocess => needs_multiprocess = true,
                _ => {}
            }
        }

        // Create native bridge if needed
        if needs_native {
            let bridge =
                Arc::new(NativeExecutorBridge::new(native_executor)) as Arc<dyn ExecutorBridge>;
            bridges.insert(ExecutorType::Native, bridge);
            tracing::info!(
                "Session {}: Native executor bridge created",
                self.session_id
            );
        }

        // Create multiprocess bridge if needed
        if needs_multiprocess {
            // Create session in multiprocess executor
            multiprocess_executor
                .create_session(self.session_id.clone())
                .await
                .map_err(|e| {
                    ServiceError::Internal(format!("Failed to create multiprocess session: {}", e))
                })?;

            let bridge = Arc::new(MultiprocessExecutorBridge::new(
                multiprocess_executor,
                self.session_id.clone(),
            )) as Arc<dyn ExecutorBridge>;
            bridges.insert(ExecutorType::Multiprocess, bridge);
            tracing::info!(
                "Session {}: Multiprocess executor bridge created",
                self.session_id
            );
        }

        Ok(())
    }

    /// Initialize executors (non-multiprocess version for when multiprocess feature is disabled)
    #[cfg(not(feature = "multiprocess"))]
    pub async fn initialize_executors(
        &self,
        native_executor: Arc<Executor>,
    ) -> Result<(), ServiceError> {
        let mut bridges = self.executor_bridges.write().await;

        // Only native executor available
        let bridge =
            Arc::new(NativeExecutorBridge::new(native_executor)) as Arc<dyn ExecutorBridge>;
        bridges.insert(ExecutorType::Native, bridge);
        tracing::info!(
            "Session {}: Native executor bridge created",
            self.session_id
        );

        Ok(())
    }

    /// Initialize all assigned nodes
    pub async fn initialize_nodes(&self, manifest: &Manifest) -> Result<(), ServiceError> {
        let node_count = self.node_assignments.read().await.len();
        tracing::info!(
            "Session {}: Initializing {} nodes",
            self.session_id,
            node_count
        );

        let bridges = self.executor_bridges.read().await;
        let assignments = self.node_assignments.read().await;

        for node in &manifest.nodes {
            if let Some(&executor_type) = assignments.get(&node.id) {
                if let Some(bridge) = bridges.get(&executor_type) {
                    bridge
                        .initialize_node(&node.id, &node.node_type, &node.params)
                        .await
                        .map_err(|e| ServiceError::NodeExecution {
                            node_id: node.id.clone(),
                            message: format!("Initialization failed: {}", e),
                        })?;
                }
            }
        }

        tracing::info!("Session {}: All nodes initialized", self.session_id);
        Ok(())
    }

    /// Cleanup session resources
    #[cfg(feature = "multiprocess")]
    pub async fn cleanup(
        &self,
        multiprocess_executor: Option<Arc<crate::python::multiprocess::MultiprocessExecutor>>,
    ) -> Result<(), ServiceError> {
        tracing::info!("Session {}: Cleaning up resources", self.session_id);

        let bridges = self.executor_bridges.read().await;
        let assignments = self.node_assignments.read().await;

        // Cleanup nodes in each bridge
        for (node_id, &executor_type) in assignments.iter() {
            if let Some(bridge) = bridges.get(&executor_type) {
                let _ = bridge.cleanup_node(node_id).await;
            }
        }

        // Terminate multiprocess session if exists
        if let Some(mp_executor) = multiprocess_executor {
            mp_executor
                .terminate_session(&self.session_id)
                .await
                .map_err(|e| {
                    ServiceError::Internal(format!(
                        "Failed to terminate multiprocess session: {}",
                        e
                    ))
                })?;
            tracing::info!(
                "Session {}: Multiprocess session terminated",
                self.session_id
            );
        }

        Ok(())
    }

    /// Cleanup session resources (non-multiprocess version)
    #[cfg(not(feature = "multiprocess"))]
    pub async fn cleanup(&self) -> Result<(), ServiceError> {
        tracing::info!("Session {}: Cleaning up resources", self.session_id);

        let bridges = self.executor_bridges.read().await;
        let assignments = self.node_assignments.read().await;

        // Cleanup nodes in each bridge
        for (node_id, &executor_type) in assignments.iter() {
            if let Some(bridge) = bridges.get(&executor_type) {
                let _ = bridge.cleanup_node(node_id).await;
            }
        }

        Ok(())
    }

    /// Get session age
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }

    /// Get session ID (for logging and testing)
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Execute pipeline using assigned executors
    ///
    /// Routes execution through appropriate executor bridges based on node assignments.
    /// For multiprocess nodes, this ensures they execute in separate Python processes.
    #[cfg(feature = "multiprocess")]
    pub async fn execute_pipeline(
        &self,
        manifest: &Manifest,
        runtime_inputs: HashMap<String, crate::data::RuntimeData>,
        native_executor: Arc<Executor>,
        _multiprocess_executor: Arc<crate::python::multiprocess::MultiprocessExecutor>,
    ) -> Result<HashMap<String, crate::data::RuntimeData>, ServiceError> {
        tracing::info!(
            "Session {}: Executing pipeline with {} nodes",
            self.session_id,
            manifest.nodes.len()
        );

        // Check if we have any multiprocess nodes
        let has_multiprocess = {
            let assignments = self.node_assignments.read().await;
            assignments
                .values()
                .any(|&et| et == ExecutorType::Multiprocess)
        };

        if has_multiprocess {
            tracing::info!("Session {}: Pipeline has MULTIPROCESS Python nodes - enabling concurrent execution", self.session_id);
        }

        // Execute with session ID - this enables multiprocess execution for Python nodes
        native_executor
            .execute_with_runtime_data_and_session(
                manifest,
                runtime_inputs,
                Some(self.session_id.clone()),
            )
            .await
            .map_err(|e| ServiceError::NodeExecution {
                node_id: "pipeline".to_string(),
                message: format!("Pipeline execution failed: {}", e),
            })
    }

    /// Execute pipeline (non-multiprocess version)
    #[cfg(not(feature = "multiprocess"))]
    pub async fn execute_pipeline(
        &self,
        manifest: &Manifest,
        runtime_inputs: HashMap<String, crate::data::RuntimeData>,
        native_executor: Arc<Executor>,
    ) -> Result<HashMap<String, crate::data::RuntimeData>, ServiceError> {
        tracing::info!(
            "Session {}: Executing pipeline with native executor only",
            self.session_id
        );

        native_executor
            .execute_with_runtime_data(manifest, runtime_inputs)
            .await
            .map_err(|e| ServiceError::NodeExecution {
                node_id: "pipeline".to_string(),
                message: format!("Pipeline execution failed: {}", e),
            })
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
                    outcome: Some(
                        crate::grpc_service::generated::execute_response::Outcome::Error(
                            error_response,
                        ),
                    ),
                };

                return Ok(Response::new(response));
            }
        };

        // Create session execution context and assign nodes to executors (spec 002)
        let session_id = format!("grpc_session_{}", uuid::Uuid::new_v4());
        let session_ctx = Arc::new(SessionExecutionContext::new(session_id.clone()));

        // Assign each node to appropriate executor based on node type
        for node in &manifest.nodes {
            let executor_type = self
                .executor_registry
                .get_executor_for_node(&node.node_type);
            session_ctx
                .assign_node(node.id.clone(), executor_type)
                .await;
            tracing::info!(
                "Node '{}' (type: {}) assigned to {:?} executor",
                node.id,
                node.node_type,
                executor_type
            );
        }

        // Initialize executors for this session
        #[cfg(feature = "multiprocess")]
        session_ctx
            .initialize_executors(
                Arc::clone(&self.executor),
                Arc::clone(&self.multiprocess_executor),
            )
            .await
            .map_err(|e| Status::internal(format!("Failed to initialize executors: {}", e)))?;

        #[cfg(not(feature = "multiprocess"))]
        session_ctx
            .initialize_executors(Arc::clone(&self.executor))
            .await
            .map_err(|e| Status::internal(format!("Failed to initialize executors: {}", e)))?;

        // Initialize all nodes
        session_ctx
            .initialize_nodes(&manifest)
            .await
            .map_err(|e| Status::internal(format!("Failed to initialize nodes: {}", e)))?;

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
                outcome: Some(
                    crate::grpc_service::generated::execute_response::Outcome::Error(
                        error_response,
                    ),
                ),
            };

            return Ok(Response::new(response));
        }

        // Convert data inputs to RuntimeData (supports all types: audio, video, tensor, json, text, binary)
        let mut runtime_inputs = HashMap::new();
        for (node_id, data_buffer) in req.data_inputs {
            match crate::data::convert_proto_to_runtime_data(data_buffer) {
                Ok(runtime_data) => {
                    runtime_inputs.insert(node_id.clone(), runtime_data);
                }
                Err(e) => {
                    self.metrics
                        .record_request_end("ExecutePipeline", "error", start_time);
                    self.metrics.record_error("validation");

                    let error_response = ErrorResponse {
                        error_type: ErrorType::Validation as i32,
                        message: format!(
                            "Input data conversion failed for node '{}': {}",
                            node_id, e
                        ),
                        failing_node_id: node_id.clone(),
                        context: "Data buffer conversion failed".to_string(),
                        stack_trace: String::new(),
                    };

                    let response = ExecuteResponse {
                        outcome: Some(
                            crate::grpc_service::generated::execute_response::Outcome::Error(
                                error_response,
                            ),
                        ),
                    };

                    return Ok(Response::new(response));
                }
            }
        }

        // Store manifest metadata for logging before moving values
        let num_nodes = manifest.nodes.len();
        let num_connections = manifest.connections.len();

        // T038: Spawn each pipeline execution in its own tokio task
        // This enables true concurrent execution across CPU cores
        // Use SessionExecutionContext to route to appropriate executors (spec 002)
        let executor = Arc::clone(&self.executor);

        #[cfg(feature = "multiprocess")]
        let mp_executor = Arc::clone(&self.multiprocess_executor);

        let session_ctx_clone = Arc::clone(&session_ctx);

        #[cfg(feature = "multiprocess")]
        let execution_task = tokio::spawn(async move {
            session_ctx_clone
                .execute_pipeline(&manifest, runtime_inputs, executor, mp_executor)
                .await
        });

        #[cfg(not(feature = "multiprocess"))]
        let execution_task = tokio::spawn(async move {
            session_ctx_clone
                .execute_pipeline(&manifest, runtime_inputs, executor)
                .await
        });

        // Execute pipeline
        info!(
            nodes = num_nodes,
            connections = num_connections,
            "Executing pipeline in isolated task"
        );

        let result_buffers = match execution_task.await {
            Ok(Ok(buffers)) => buffers,
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
                    outcome: Some(
                        crate::grpc_service::generated::execute_response::Outcome::Error(
                            error_response,
                        ),
                    ),
                };

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
                    outcome: Some(
                        crate::grpc_service::generated::execute_response::Outcome::Error(
                            error_response,
                        ),
                    ),
                };

                return Ok(Response::new(response));
            }
        };

        // Serialize outputs to protobuf format (convert RuntimeData to DataBuffer)
        let mut data_outputs = HashMap::new();
        for (node_id, runtime_data) in result_buffers {
            let data_buffer = crate::data::convert_runtime_to_proto_data(runtime_data);
            data_outputs.insert(node_id, data_buffer);
        }

        info!(
            outputs = data_outputs.len(),
            "Pipeline execution completed with RuntimeData"
        );

        // Collect metrics
        let metrics = self.collect_metrics(start_time, 10_000_000); // TODO: Measure actual memory

        let exec_result_proto = ProtoExecutionResult {
            data_outputs,
            metrics: Some(metrics),
            node_results: vec![], // TODO: Include per-node results
            status: ExecutionStatus::Success as i32,
        };

        let response = ExecuteResponse {
            outcome: Some(
                crate::grpc_service::generated::execute_response::Outcome::Result(
                    exec_result_proto,
                ),
            ),
        };

        self.metrics
            .record_request_end("ExecutePipeline", "success", start_time);

        // Cleanup session resources (spec 002)
        #[cfg(feature = "multiprocess")]
        session_ctx
            .cleanup(Some(Arc::clone(&self.multiprocess_executor)))
            .await
            .map_err(|e| Status::internal(format!("Session cleanup failed: {}", e)))?;

        #[cfg(not(feature = "multiprocess"))]
        session_ctx
            .cleanup()
            .await
            .map_err(|e| Status::internal(format!("Session cleanup failed: {}", e)))?;

        tracing::info!(
            "Session {} completed and cleaned up",
            session_ctx.session_id()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder() {
        // Placeholder test - gRPC integration tests are in tests/grpc_integration/
        assert!(true);
    }
}
