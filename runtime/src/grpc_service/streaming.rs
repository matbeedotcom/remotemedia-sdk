//! Bidirectional streaming RPC handler for StreamPipeline
//!
//! This module implements the StreamingPipelineService trait for real-time
//! chunk-by-chunk audio processing with <50ms latency per chunk.
//!
//! # Architecture
//!
//! - **StreamingServiceImpl**: Main service implementation with session management
//! - **StreamSession**: Per-session state (manifest, executor, metrics, sequence tracking)
//! - **stream_pipeline**: Bidirectional stream handler loop
//!
//! # Flow
//!
//! 1. Client sends StreamInit with manifest → Server responds with StreamReady
//! 2. Client sends AudioChunk messages → Server processes and returns ChunkResult
//! 3. Periodic StreamMetrics sent every 10 chunks
//! 4. Client sends StreamControl::CLOSE → Server flushes and sends StreamClosed
//!
//! # Performance
//!
//! - Target: <50ms average latency per chunk (User Story 3)
//! - Bounded buffer to prevent memory bloat
//! - Backpressure via STREAM_ERROR_BUFFER_OVERFLOW

use crate::audio::AudioBuffer as RuntimeAudioBuffer;
use crate::executor::Executor;
use crate::manifest::Manifest;
use crate::grpc_service::generated::{
    AudioChunk, AudioBuffer as ProtoAudioBuffer, ChunkResult, ErrorResponse, ErrorType, ExecutionMetrics, StreamClosed,
    StreamControl, StreamInit, StreamMetrics, StreamReady, StreamRequest, StreamResponse,
    stream_control::Command, stream_request::Request as StreamRequestType,
    stream_response::Response as StreamResponseType,
};
use crate::grpc_service::metrics::ServiceMetrics;
use crate::grpc_service::{ServiceConfig, ServiceError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Maximum number of chunks buffered before backpressure
const MAX_BUFFER_CHUNKS: usize = 10;

/// Maximum session idle time before timeout (seconds)
const SESSION_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Frequency of metrics updates (every N chunks)
const METRICS_UPDATE_INTERVAL: u64 = 10;

/// Streaming pipeline service implementation
pub struct StreamingServiceImpl {
    /// Active streaming sessions (keyed by session_id)
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<StreamSession>>>>>,
    
    /// Service configuration
    config: ServiceConfig,
    
    /// Global executor (shared across sessions)
    executor: Arc<Executor>,
    
    /// Prometheus metrics
    metrics: Arc<ServiceMetrics>,
}

impl StreamingServiceImpl {
    /// Create new streaming service instance
    pub fn new(config: ServiceConfig, executor: Arc<Executor>, metrics: Arc<ServiceMetrics>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
            executor,
            metrics,
        }
    }

    /// Get number of active sessions
    pub async fn active_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

/// Per-session state for streaming execution
struct StreamSession {
    /// Unique session identifier
    session_id: String,
    
    /// Parsed pipeline manifest
    manifest: Manifest,
    
    /// Expected next sequence number
    next_sequence: u64,
    
    /// Total chunks processed
    chunks_processed: u64,
    
    /// Total items processed (samples, frames, tokens, etc.)
    total_items: u64,

    /// Data type distribution
    data_type_counts: HashMap<String, u64>,
    
    /// Total chunks dropped (backpressure)
    chunks_dropped: u64,
    
    /// Cumulative processing time (milliseconds)
    cumulative_processing_time_ms: f64,
    
    /// Peak memory usage (bytes)
    peak_memory_bytes: u64,
    
    /// Current buffer occupancy (items)
    buffer_items: u64,
    
    /// Session creation time
    created_at: Instant,
    
    /// Last activity time (for timeout detection)
    last_activity: Instant,
    
    /// Recommended chunk size (samples)
    recommended_chunk_size: u64,
}

impl StreamSession {
    /// Create new session from StreamInit request
    fn new(session_id: String, manifest: Manifest, recommended_chunk_size: u64) -> Self {
        let now = Instant::now();
        Self {
            session_id,
            manifest,
            next_sequence: 0,
            chunks_processed: 0,
            total_items: 0,
            data_type_counts: HashMap::new(),
            chunks_dropped: 0,
            cumulative_processing_time_ms: 0.0,
            peak_memory_bytes: 0,
            buffer_items: 0,
            created_at: now,
            last_activity: now,
            recommended_chunk_size,
        }
    }

    /// Update last activity timestamp
    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if session has timed out
    fn is_timed_out(&self) -> bool {
        self.last_activity.elapsed().as_secs() > SESSION_TIMEOUT_SECS
    }

    /// Validate sequence number (detect gaps or out-of-order)
    fn validate_sequence(&mut self, sequence: u64) -> Result<(), ServiceError> {
        if sequence < self.next_sequence {
            return Err(ServiceError::Validation(format!(
                "Out-of-order chunk: expected sequence {}, got {}",
                self.next_sequence, sequence
            )));
        }
        
        if sequence > self.next_sequence {
            let gap = sequence - self.next_sequence;
            warn!(
                session_id = %self.session_id,
                expected = self.next_sequence,
                received = sequence,
                gap = gap,
                "Missing chunks detected"
            );
            // For now, accept the chunk but log the gap
            // In production, might want to return STREAM_ERROR_INVALID_SEQUENCE
        }
        
        self.next_sequence = sequence + 1;
        Ok(())
    }

    /// Record processing metrics for a chunk
    fn record_chunk_metrics(&mut self, processing_time_ms: f64, items: u64, memory_bytes: u64, data_type: &str) {
        self.chunks_processed += 1;
        self.total_items += items;
        self.cumulative_processing_time_ms += processing_time_ms;

        // Update data type breakdown
        *self.data_type_counts.entry(data_type.to_string()).or_insert(0) += 1;

        if memory_bytes > self.peak_memory_bytes {
            self.peak_memory_bytes = memory_bytes;
        }

        self.touch();
    }

    /// Calculate average latency across all processed chunks
    fn average_latency_ms(&self) -> f64 {
        if self.chunks_processed == 0 {
            0.0
        } else {
            self.cumulative_processing_time_ms / self.chunks_processed as f64
        }
    }

    /// Generate StreamMetrics message
    fn create_metrics(&self) -> StreamMetrics {
        StreamMetrics {
            session_id: self.session_id.clone(),
            chunks_processed: self.chunks_processed,
            average_latency_ms: self.average_latency_ms(),
            total_items: self.total_items,
            buffer_items: self.buffer_items,
            chunks_dropped: self.chunks_dropped,
            peak_memory_bytes: self.peak_memory_bytes,
            data_type_breakdown: self.data_type_counts.clone(),
        }
    }

    /// Generate final ExecutionMetrics for StreamClosed
    fn create_final_metrics(&self) -> ExecutionMetrics {
        ExecutionMetrics {
            wall_time_ms: self.created_at.elapsed().as_secs_f64() * 1000.0,
            cpu_time_ms: self.cumulative_processing_time_ms, // Approximate
            memory_used_bytes: self.peak_memory_bytes,
            node_metrics: HashMap::new(), // TODO: Populate from executor
            serialization_time_ms: 0.0, // Not tracked for streaming
            proto_to_runtime_ms: 0.0, // Not tracked yet
            runtime_to_proto_ms: 0.0, // Not tracked yet
            data_type_breakdown: self.data_type_counts.clone(),
        }
    }
}

#[tonic::async_trait]
impl crate::grpc_service::StreamingPipelineService for StreamingServiceImpl {
    type StreamPipelineStream = tokio_stream::wrappers::ReceiverStream<Result<StreamResponse, Status>>;

    async fn stream_pipeline(
        &self,
        request: Request<Streaming<StreamRequest>>,
    ) -> Result<Response<Self::StreamPipelineStream>, Status> {
        info!("StreamPipeline RPC invoked");

        // Preview feature header validation (from initial request metadata)
        if let Some(hdr_val) = request.metadata().get("x-preview-features") {
            let val = hdr_val.to_str().unwrap_or("").to_lowercase();
            let has_gpt5 = val.split(',').any(|s| s.trim() == "gpt5-codex");
            if has_gpt5 {
                if !self.config.enable_gpt5_codex_preview {
                    return Err(Status::failed_precondition(
                        "Preview feature 'gpt5-codex' is disabled on this server",
                    ));
                } else {
                    info!("Preview feature enabled: gpt5-codex");
                }
            }
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let mut stream = request.into_inner();
        let sessions = self.sessions.clone();
        let executor = self.executor.clone();
        let metrics = self.metrics.clone();

        // Spawn async task to handle bidirectional streaming
        tokio::spawn(async move {
            let result = handle_stream(&mut stream, tx.clone(), sessions, executor, metrics).await;
            
            if let Err(e) = result {
                error!(error = %e, "Stream handling error");
                let error_response = ErrorResponse {
                    error_type: ErrorType::Internal as i32,
                    message: e.to_string(),
                    failing_node_id: String::new(),
                    context: String::new(),
                    stack_trace: String::new(),
                };
                let response = StreamResponse {
                    response: Some(StreamResponseType::Error(error_response)),
                };
                let _ = tx.send(Ok(response)).await;
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

/// Handle bidirectional stream (runs in async task)
async fn handle_stream(
    stream: &mut Streaming<StreamRequest>,
    tx: tokio::sync::mpsc::Sender<Result<StreamResponse, Status>>,
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<StreamSession>>>>>,
    executor: Arc<Executor>,
    metrics: Arc<ServiceMetrics>,
) -> Result<(), ServiceError> {
    let mut session: Option<Arc<Mutex<StreamSession>>> = None;
    let mut session_id = String::new();

    // Main stream loop
    while let Some(request_result) = stream.message().await.map_err(|e| {
        ServiceError::Internal(format!("Stream receive error: {}", e))
    })? {
        match request_result.request {
            Some(StreamRequestType::Init(init)) => {
                // Handle StreamInit (must be first message)
                if session.is_some() {
                    return Err(ServiceError::Validation(
                        "StreamInit already received".to_string(),
                    ));
                }

                debug!("Processing StreamInit");
                let (new_session_id, ready) = handle_stream_init(init, &sessions, executor.clone()).await?;
                session_id = new_session_id.clone();
                session = Some(sessions.read().await.get(&session_id).unwrap().clone());

                // Record metrics
                metrics.record_stream_start();

                // Send StreamReady response
                let response = StreamResponse {
                    response: Some(StreamResponseType::Ready(ready)),
                };
                tx.send(Ok(response)).await.map_err(|_| {
                    ServiceError::Internal("Failed to send StreamReady".to_string())
                })?;
            }

            Some(StreamRequestType::AudioChunk(chunk)) => {
                // Handle AudioChunk
                let sess = session.as_ref().ok_or_else(|| {
                    ServiceError::Validation("StreamInit required before AudioChunk".to_string())
                })?;

                let chunk_start = Instant::now();
                debug!(sequence = chunk.sequence, "Processing AudioChunk");
                
                let result = handle_audio_chunk(chunk, sess.clone(), executor.clone()).await;
                
                match result {
                    Ok(chunk_result) => {
                        let latency = chunk_start.elapsed().as_secs_f64();
                        metrics.record_chunk_processed(&session_id, latency);

                        // Send ChunkResult response
                        let response = StreamResponse {
                            response: Some(StreamResponseType::Result(chunk_result.clone())),
                        };
                        tx.send(Ok(response)).await.map_err(|_| {
                            ServiceError::Internal("Failed to send ChunkResult".to_string())
                        })?;

                        // Send periodic metrics
                        let sess_lock = sess.lock().await;
                        if sess_lock.chunks_processed % METRICS_UPDATE_INTERVAL == 0 {
                            let stream_metrics = sess_lock.create_metrics();
                            drop(sess_lock);
                            
                            let metrics_response = StreamResponse {
                                response: Some(StreamResponseType::Metrics(stream_metrics)),
                            };
                            tx.send(Ok(metrics_response)).await.map_err(|_| {
                                ServiceError::Internal("Failed to send StreamMetrics".to_string())
                            })?;
                        }
                    }
                    Err(e) => {
                        metrics.record_chunk_error(&session_id);
                        return Err(e);
                    }
                }
            }

            Some(StreamRequestType::DataChunk(data_chunk)) => {
                // Handle DataChunk (generic streaming)
                let sess = session.as_ref().ok_or_else(|| {
                    ServiceError::Validation("StreamInit required before DataChunk".to_string())
                })?;

                let chunk_start = Instant::now();
                debug!(sequence = data_chunk.sequence, "Processing DataChunk");

                let result = handle_data_chunk(data_chunk, sess.clone(), executor.clone()).await;

                match result {
                    Ok(chunk_result) => {
                        let latency = chunk_start.elapsed().as_secs_f64();
                        metrics.record_chunk_processed(&session_id, latency);

                        // Send ChunkResult response
                        let response = StreamResponse {
                            response: Some(StreamResponseType::Result(chunk_result.clone())),
                        };
                        tx.send(Ok(response)).await.map_err(|_| {
                            ServiceError::Internal("Failed to send ChunkResult".to_string())
                        })?;

                        // Send periodic metrics
                        let sess_lock = sess.lock().await;
                        if sess_lock.chunks_processed % METRICS_UPDATE_INTERVAL == 0 {
                            let stream_metrics = sess_lock.create_metrics();
                            drop(sess_lock);

                            let metrics_response = StreamResponse {
                                response: Some(StreamResponseType::Metrics(stream_metrics)),
                            };
                            tx.send(Ok(metrics_response)).await.map_err(|_| {
                                ServiceError::Internal("Failed to send StreamMetrics".to_string())
                            })?;
                        }
                    }
                    Err(e) => {
                        metrics.record_chunk_error(&session_id);
                        return Err(e);
                    }
                }
            }

            Some(StreamRequestType::Control(control)) => {
                // Handle StreamControl (CLOSE or CANCEL)
                let sess = session.as_ref().ok_or_else(|| {
                    ServiceError::Validation("StreamInit required before StreamControl".to_string())
                })?;

                debug!(command = control.command, "Processing StreamControl");
                let closed = handle_stream_control(control, sess.clone()).await?;

                // Send StreamClosed response
                let response = StreamResponse {
                    response: Some(StreamResponseType::Closed(closed)),
                };
                tx.send(Ok(response)).await.map_err(|_| {
                    ServiceError::Internal("Failed to send StreamClosed".to_string())
                })?;

                // Cleanup session and metrics
                sessions.write().await.remove(&session_id);
                metrics.record_stream_end();
                info!(session_id = %session_id, "Session closed");
                break; // Exit stream loop
            }

            None => {
                warn!("Received empty StreamRequest");
            }
        }
    }

    // If we exit loop without explicit close, cleanup
    if !session_id.is_empty() {
        sessions.write().await.remove(&session_id);
        metrics.record_stream_end();
        info!(session_id = %session_id, "Session disconnected");
    }

    Ok(())
}

/// Handle StreamInit message
async fn handle_stream_init(
    init: StreamInit,
    sessions: &Arc<RwLock<HashMap<String, Arc<Mutex<StreamSession>>>>>,
    _executor: Arc<Executor>,
) -> Result<(String, StreamReady), ServiceError> {
    // Validate client version (basic check)
    if init.client_version.is_empty() {
        return Err(ServiceError::Validation("client_version required".to_string()));
    }

    // Deserialize manifest
    let manifest_proto = init.manifest.ok_or_else(|| {
        ServiceError::Validation("manifest required in StreamInit".to_string())
    })?;
    
    let manifest = deserialize_manifest_from_proto(&manifest_proto)?;

    // Generate unique session ID
    let session_id = Uuid::new_v4().to_string();

    // Determine recommended chunk size (use client's suggestion or default)
    let recommended_chunk_size = if init.expected_chunk_size > 0 {
        init.expected_chunk_size
    } else {
        4096 // Default: 4096 samples (~256ms at 16kHz)
    };

    // Create session
    let session = Arc::new(Mutex::new(StreamSession::new(
        session_id.clone(),
        manifest,
        recommended_chunk_size,
    )));

    // Store session
    sessions.write().await.insert(session_id.clone(), session);

    info!(
        session_id = %session_id,
        chunk_size = recommended_chunk_size,
        "StreamSession created"
    );

    // Return StreamReady
    let ready = StreamReady {
        session_id: session_id.clone(),
        recommended_chunk_size,
        max_buffer_latency_ms: 100, // 100ms max buffer latency
    };

    Ok((session_id, ready))
}

/// Handle AudioChunk message
async fn handle_audio_chunk(
    chunk: AudioChunk,
    session: Arc<Mutex<StreamSession>>,
    executor: Arc<Executor>,
) -> Result<ChunkResult, ServiceError> {
    let start_time = Instant::now();

    // Lock session to get manifest and validate
    let manifest = {
        let mut sess = session.lock().await;

        // Validate sequence number
        sess.validate_sequence(chunk.sequence)?;

        // Clone manifest for execution
        sess.manifest.clone()
    };

    // Deserialize audio buffer
    let buffer_proto = chunk.buffer.ok_or_else(|| {
        ServiceError::Validation("AudioChunk.buffer required".to_string())
    })?;

    let audio_buffer = convert_proto_to_runtime_audio(&buffer_proto)?;
    let samples = buffer_proto.num_samples;

    // Build audio inputs map for the chunk
    // The chunk's node_id tells us which node should receive this audio
    let mut audio_inputs = HashMap::new();
    audio_inputs.insert(chunk.node_id.clone(), audio_buffer);

    // Execute pipeline with fast audio path
    let result_buffers = executor
        .execute_fast_pipeline(&manifest, audio_inputs)
        .await
        .map_err(|e| ServiceError::Internal(format!("Pipeline execution failed: {}", e)))?;

    let processing_time_ms = start_time.elapsed().as_secs_f64() * 1000.0;

    // Lock session again to record metrics and get total items
    let total_items = {
        let mut sess = session.lock().await;
        sess.record_chunk_metrics(processing_time_ms, samples, 0, "audio"); // TODO: Track memory
        sess.total_items
    };

    // Convert result buffers to proto format
    // Convert audio buffers to DataBuffer format
    let mut data_outputs = HashMap::new();
    for (node_id, buffer) in result_buffers {
        let proto_buffer = convert_runtime_to_proto_audio(&buffer);
        // Wrap audio buffer in DataBuffer
        let data_buffer = crate::grpc_service::generated::DataBuffer {
            data_type: Some(crate::grpc_service::generated::data_buffer::DataType::Audio(proto_buffer)),
            metadata: HashMap::new(),
        };
        data_outputs.insert(node_id, data_buffer);
    }

    let result = ChunkResult {
        sequence: chunk.sequence,
        data_outputs,
        processing_time_ms,
        total_items_processed: total_items,
    };

    Ok(result)
}

/// Handle DataChunk message (generic streaming)
async fn handle_data_chunk(
    chunk: crate::grpc_service::generated::DataChunk,
    session: Arc<Mutex<StreamSession>>,
    _executor: Arc<Executor>,
) -> Result<ChunkResult, ServiceError> {
    let start_time = Instant::now();

    // Lock session to validate and get manifest
    let (manifest, node_type, params) = {
        let mut sess = session.lock().await;
        sess.validate_sequence(chunk.sequence)?;

        // Get node info from manifest
        let node_spec = sess.manifest.nodes.iter()
            .find(|n| n.id == chunk.node_id)
            .ok_or_else(|| ServiceError::Validation(
                format!("Node '{}' not found in manifest", chunk.node_id)
            ))?;

        (sess.manifest.clone(), node_spec.node_type.clone(), node_spec.params.clone())
    };

    // Convert DataBuffer(s) to RuntimeData
    // Support either single buffer OR named_buffers (for multi-input nodes)
    use crate::data::convert_proto_to_runtime_data;

    let (runtime_data_map, data_type, item_count) = if !chunk.named_buffers.is_empty() {
        // Multi-input mode: convert all named buffers
        let mut map = HashMap::new();
        let mut total_items = 0u64;
        let mut types = Vec::new();

        for (name, data_buffer) in chunk.named_buffers {
            let runtime_data = convert_proto_to_runtime_data(data_buffer)
                .map_err(|e| ServiceError::Validation(format!("Data conversion failed for '{}': {}", name, e)))?;

            types.push(runtime_data.type_name());
            total_items += runtime_data.item_count() as u64;
            map.insert(name, runtime_data);
        }

        let combined_type = if types.len() == 1 {
            types[0].to_string()
        } else {
            format!("multi[{}]", types.join("+"))
        };

        (map, combined_type, total_items)
    } else if let Some(data_buffer) = chunk.buffer {
        // Single-input mode: use buffer field
        let runtime_data = convert_proto_to_runtime_data(data_buffer)
            .map_err(|e| ServiceError::Validation(format!("Data conversion failed: {}", e)))?;

        let data_type = runtime_data.type_name().to_string();
        let item_count = runtime_data.item_count() as u64;

        // Wrap in map with default key for backward compatibility
        let mut map = HashMap::new();
        map.insert("input".to_string(), runtime_data);

        (map, data_type, item_count)
    } else {
        return Err(ServiceError::Validation(
            "DataChunk must have either 'buffer' or 'named_buffers' set".to_string()
        ));
    };

    // Process data through node based on node type
    // For single-input nodes, extract the first (or "input") value
    // For multi-input nodes, pass the entire HashMap
    let output_data = match node_type.as_str() {
        "CalculatorNode" => {
            use crate::nodes::calculator::CalculatorNode;
            let node = CalculatorNode::new(chunk.node_id.clone(), &params.to_string())
                .map_err(|e| ServiceError::Internal(format!("Failed to create CalculatorNode: {}", e)))?;

            // Single-input node: extract first value
            let input_data = runtime_data_map.get("input")
                .or_else(|| runtime_data_map.values().next())
                .ok_or_else(|| ServiceError::Validation("No input data provided".to_string()))?
                .clone();

            node.process(input_data)
                .map_err(|e| ServiceError::Internal(format!("Node execution failed: {}", e)))?
        }
        "VideoProcessorNode" => {
            use crate::nodes::video_processor::VideoProcessorNode;
            let node = VideoProcessorNode::new(chunk.node_id.clone(), &params.to_string())
                .map_err(|e| ServiceError::Internal(format!("Failed to create VideoProcessorNode: {}", e)))?;

            // Single-input node: extract first value
            let input_data = runtime_data_map.get("input")
                .or_else(|| runtime_data_map.values().next())
                .ok_or_else(|| ServiceError::Validation("No input data provided".to_string()))?
                .clone();

            node.process(input_data)
                .map_err(|e| ServiceError::Internal(format!("Node execution failed: {}", e)))?
        }
        "PassThrough" => {
            // Simple passthrough for testing - return first input
            runtime_data_map.get("input")
                .or_else(|| runtime_data_map.values().next())
                .ok_or_else(|| ServiceError::Validation("No input data provided".to_string()))?
                .clone()
        }
        "SynchronizedAudioVideoNode" => {
            // Multi-input node: pass entire HashMap
            use crate::nodes::sync_av::SynchronizedAudioVideoNode;
            let node = SynchronizedAudioVideoNode::new(chunk.node_id.clone(), &params.to_string())
                .map_err(|e| ServiceError::Internal(format!("Failed to create SynchronizedAudioVideoNode: {}", e)))?;

            node.process_multi(runtime_data_map)
                .map_err(|e| ServiceError::Internal(format!("Node execution failed: {}", e)))?
        }
        other => {
            return Err(ServiceError::Internal(
                format!("Node type '{}' not supported for generic streaming yet", other)
            ));
        }
    };

    // Convert runtime data back to proto
    use crate::data::convert_runtime_to_proto_data;
    let mut data_outputs = HashMap::new();
    let output_buffer = convert_runtime_to_proto_data(output_data);
    data_outputs.insert(chunk.node_id.clone(), output_buffer);

    let processing_time_ms = start_time.elapsed().as_secs_f64() * 1000.0;

    // Lock session again to record metrics
    let total_items = {
        let mut sess = session.lock().await;
        sess.record_chunk_metrics(processing_time_ms, item_count, 0, &data_type);
        sess.total_items
    };

    let result = ChunkResult {
        sequence: chunk.sequence,
        data_outputs,
        processing_time_ms,
        total_items_processed: total_items,
    };

    Ok(result)
}

/// Handle StreamControl message
async fn handle_stream_control(
    control: StreamControl,
    session: Arc<Mutex<StreamSession>>,
) -> Result<StreamClosed, ServiceError> {
    let sess = session.lock().await;
    
    let command = Command::try_from(control.command)
        .map_err(|_| ServiceError::Validation(format!("Invalid command: {}", control.command)))?;

    let reason = match command {
        Command::Close => "Client requested close",
        Command::Cancel => "Client requested cancel",
        Command::Unspecified => "Unspecified",
    };

    let closed = StreamClosed {
        session_id: sess.session_id.clone(),
        final_metrics: Some(sess.create_final_metrics()),
        reason: reason.to_string(),
    };

    info!(
        session_id = %sess.session_id,
        chunks_processed = sess.chunks_processed,
        reason = reason,
        "Stream closing"
    );

    Ok(closed)
}

/// Helper: Deserialize protobuf PipelineManifest to runtime Manifest
fn deserialize_manifest_from_proto(
    proto: &crate::grpc_service::generated::PipelineManifest,
) -> Result<Manifest, ServiceError> {
    // Convert to JSON for existing Manifest parser
    let json_str = serde_json::json!({
        "version": proto.version,
        "metadata": serde_json::json!({
            "name": proto.metadata.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
            "description": proto.metadata.as_ref().map(|m| m.description.clone()).unwrap_or_default(),
            "created_at": proto.metadata.as_ref().map(|m| m.created_at.clone()).unwrap_or_else(|| "2025-10-28T00:00:00Z".to_string()),
        }),
        "nodes": proto.nodes.iter().map(|n| {
            serde_json::json!({
                "id": n.id,
                "node_type": n.node_type,
                "parameters": serde_json::from_str::<serde_json::Value>(&n.params)
                    .unwrap_or(serde_json::json!({})),
                "runtime_hint": match n.runtime_hint {
                    0 => "auto",
                    1 => "rust_python",
                    2 => "cpython",
                    3 => "cpython_wasm",
                    _ => "auto",
                },
                "metadata": serde_json::json!({
                    "name": n.node_type,
                    "description": "",
                    "created_at": "2025-10-28T00:00:00Z",
                })
            })
        }).collect::<Vec<_>>(),
        "connections": proto.connections.iter().map(|c| {
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

/// Helper: Convert protobuf AudioBuffer to runtime AudioBuffer
fn convert_proto_to_runtime_audio(
    proto: &ProtoAudioBuffer,
) -> Result<RuntimeAudioBuffer, ServiceError> {
    use crate::audio::AudioFormat;
    use crate::grpc_service::generated::AudioFormat as ProtoAudioFormat;

    // Convert format and decode bytes to f32 samples
    let samples: Vec<f32> = match ProtoAudioFormat::try_from(proto.format) {
        Ok(ProtoAudioFormat::F32) => {
            // Convert bytes to f32 (little-endian)
            proto
                .samples
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        }
        Ok(ProtoAudioFormat::I16) => {
            // Convert bytes to i16, then to f32 normalized (-1.0 to 1.0)
            proto
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
            proto
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
                proto.format
            )));
        }
    };

    // Create runtime AudioBuffer
    Ok(RuntimeAudioBuffer::new(
        Arc::new(samples),
        proto.sample_rate,
        proto.channels as u16,
        AudioFormat::F32,
    ))
}

/// Helper: Convert runtime AudioBuffer to protobuf AudioBuffer
fn convert_runtime_to_proto_audio(buffer: &RuntimeAudioBuffer) -> ProtoAudioBuffer {
    use crate::audio::AudioFormat;
    use crate::grpc_service::generated::AudioFormat as ProtoAudioFormat;

    let format = match buffer.format() {
        AudioFormat::F32 => ProtoAudioFormat::F32 as i32,
        AudioFormat::I16 => ProtoAudioFormat::I16 as i32,
        AudioFormat::I32 => ProtoAudioFormat::I32 as i32,
    };

    // Convert f32 samples to bytes based on format
    let samples: Vec<u8> = match buffer.format() {
        AudioFormat::F32 => {
            // Convert f32 to bytes (little-endian)
            buffer
                .as_slice()
                .iter()
                .flat_map(|&sample| sample.to_le_bytes())
                .collect()
        }
        AudioFormat::I16 => {
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
        AudioFormat::I32 => {
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

