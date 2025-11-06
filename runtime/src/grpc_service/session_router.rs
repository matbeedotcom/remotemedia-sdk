//! Session-level Async Router for Persistent Streaming
//!
//! This module implements a persistent router that runs for the entire session,
//! continuously processing chunks from the client and routing them through the pipeline.

use crate::data::{RuntimeData, convert_runtime_to_proto_data};
use crate::grpc_service::streaming::StreamSession;
use crate::grpc_service::generated::{StreamResponse, stream_response::Response as StreamResponseType, ChunkResult};
use crate::nodes::StreamingNode;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{info, error, debug, warn};
use tonic::Status;

#[cfg(feature = "multiprocess")]
use crate::python::multiprocess::MultiprocessExecutor;

/// Represents a data packet flowing through the pipeline
#[derive(Clone, Debug)]
pub struct DataPacket {
    /// The actual data
    pub data: RuntimeData,
    /// Source node ID
    pub from_node: String,
    /// Target node ID (if specified)
    pub to_node: Option<String>,
    /// Session ID
    pub session_id: String,
    /// Sequence number for ordering
    pub sequence: u64,
    /// Sub-sequence for streaming outputs
    pub sub_sequence: u64,
}

/// Session-persistent router that runs for the entire streaming session
pub struct SessionRouter {
    /// Session ID
    session_id: String,

    /// Registry for creating nodes
    registry: Arc<crate::nodes::StreamingNodeRegistry>,

    /// Session state
    session: Arc<Mutex<StreamSession>>,

    /// Channel to send results to client
    client_tx: mpsc::Sender<Result<StreamResponse, Status>>,

    /// Channel to receive new chunks from client
    input_rx: mpsc::UnboundedReceiver<DataPacket>,

    /// Channel to send new chunks to the router
    input_tx: mpsc::UnboundedSender<DataPacket>,

    /// Channel to receive shutdown signal
    shutdown_rx: mpsc::Receiver<()>,

    /// Channel to send shutdown signal (held externally)
    _shutdown_tx: mpsc::Sender<()>,

    /// Active node tasks
    node_tasks: HashMap<String, JoinHandle<()>>,

    /// Node input channels
    node_inputs: HashMap<String, mpsc::UnboundedSender<DataPacket>>,

    /// Whether the router is running
    running: bool,

    /// Multiprocess executor for IPC communication (optional)
    #[cfg(feature = "multiprocess")]
    multiprocess_executor: Option<Arc<MultiprocessExecutor>>,
}

impl SessionRouter {
    /// Create a new session router
    /// Returns (router, shutdown_sender) - the shutdown_sender should be stored to trigger shutdown
    pub fn new(
        session_id: String,
        registry: Arc<crate::nodes::StreamingNodeRegistry>,
        session: Arc<Mutex<StreamSession>>,
        client_tx: mpsc::Sender<Result<StreamResponse, Status>>,
    ) -> (Self, mpsc::Sender<()>) {
        let (input_tx, input_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let shutdown_tx_clone = shutdown_tx.clone();

        let router = Self {
            session_id,
            registry,
            session,
            client_tx,
            input_rx,
            input_tx,
            shutdown_rx,
            _shutdown_tx: shutdown_tx,
            node_tasks: HashMap::new(),
            node_inputs: HashMap::new(),
            running: false,
            #[cfg(feature = "multiprocess")]
            multiprocess_executor: None,
        };

        (router, shutdown_tx_clone)
    }

    /// Get the input sender for feeding chunks from the client
    pub fn get_input_sender(&self) -> mpsc::UnboundedSender<DataPacket> {
        self.input_tx.clone()
    }

    /// Set the multiprocess executor for IPC communication
    #[cfg(feature = "multiprocess")]
    pub fn set_multiprocess_executor(&mut self, executor: Arc<MultiprocessExecutor>) {
        self.multiprocess_executor = Some(executor);
    }

    /// Pre-initialize all nodes in the manifest before streaming starts
    ///
    /// This eliminates cold-start latency by loading all models upfront.
    /// Any initialization errors are caught early before streaming begins.
    ///
    /// Sends real-time status updates to the client during initialization.
    pub async fn pre_initialize_all_nodes(&mut self) -> Result<(), Status> {
        let node_specs: Vec<(String, String)> = {
            let session = self.session.lock().await;
            session.manifest.nodes.iter()
                .map(|n| (n.id.clone(), n.node_type.clone()))
                .collect()
        };

        let total_nodes = node_specs.len();
        info!("🔥 Pre-initializing {} nodes for session '{}'...", total_nodes, self.session_id);
        info!("   Node list: {:?}", node_specs.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>());

        // Send initialization start message to client (non-blocking fire-and-forget)
        let _ = self.client_tx.try_send(Ok({
            use crate::data::RuntimeData;
            use crate::grpc_service::generated::{StreamResponse, stream_response::Response as StreamResponseType};

            let status_text = format!("[_system] status=initializing message=Initializing {} nodes...", total_nodes);
            let status_data = RuntimeData::Text(status_text);
            let proto_data = convert_runtime_to_proto_data(status_data);

            let mut data_outputs = HashMap::new();
            data_outputs.insert("_status".to_string(), proto_data);

            StreamResponse {
                response: Some(StreamResponseType::Result(ChunkResult {
                    sequence: 0,
                    data_outputs,
                    processing_time_ms: 0.0,
                    total_items_processed: 0,
                })),
            }
        }));

        for (idx, (node_id, node_type)) in node_specs.iter().enumerate() {
            let progress = ((idx + 1) * 100) / total_nodes;

            info!("   📦 [{}/{}] Initializing {} (type: {})...",
                  idx + 1, total_nodes, node_id, node_type);

            // Send "initializing" status to client
            self.send_status_update(
                &node_id,
                "initializing",
                &format!("Loading {} ({}/{})", node_type, idx + 1, total_nodes)
            );

            match self.get_or_create_node(&node_id).await {
                Ok(node) => {
                    info!("   📦 [{}/{}] Node created, calling initialize()...",
                          idx + 1, total_nodes);

                    // 🔥 Actually call initialize() to load models
                    match node.initialize().await {
                        Ok(_) => {
                            info!("   ✅ [{}/{}] {} initialized successfully ({}% complete)",
                                  idx + 1, total_nodes, node_id, progress);

                            // Query node status
                            let status = node.get_status();

                            // Send "ready" status to client
                            self.send_status_update(
                                &node_id,
                                status.as_str(),
                                &format!("{} ready ({}/{})", node_type, idx + 1, total_nodes)
                            );
                        }
                        Err(init_err) => {
                            error!("   ❌ [{}/{}] Failed to initialize {}: {}",
                                   idx + 1, total_nodes, node_id, init_err);

                            // Send "error" status to client
                            self.send_status_update(
                                &node_id,
                                "error",
                                &format!("Initialization failed: {}", init_err)
                            );

                            return Err(Status::internal(
                                format!("Failed to initialize node '{}': {}", node_id, init_err)
                            ));
                        }
                    }
                }
                Err(e) => {
                    error!("   ❌ [{}/{}] Failed to initialize {}: {}",
                           idx + 1, total_nodes, node_id, e);

                    // Send "error" status to client
                    self.send_status_update(
                        &node_id,
                        "error",
                        &format!("Failed to initialize {}: {}", node_type, e)
                    );

                    return Err(Status::internal(
                        format!("Failed to pre-initialize node '{}': {}", node_id, e)
                    ));
                }
            }
        }

        info!("✅ All {} nodes pre-initialized and ready for streaming", total_nodes);

        // Send completion message to client
        self.send_status_update("_system", "ready",
            &format!("All {} nodes ready for streaming", total_nodes));

        Ok(())
    }

    /// Send a status update message to the client (non-blocking)
    fn send_status_update(&self, node_id: &str, status: &str, message: &str) {
        use crate::data::RuntimeData;
        use crate::grpc_service::generated::{StreamResponse, stream_response::Response as StreamResponseType};

        // Create status message as text
        let status_text = format!("[{}] status={} message={}", node_id, status, message);
        let status_data = RuntimeData::Text(status_text);

        // Convert to proto
        let proto_data = convert_runtime_to_proto_data(status_data);

        // Create ChunkResult with status info
        let mut data_outputs = HashMap::new();
        data_outputs.insert("_status".to_string(), proto_data);

        let chunk_result = ChunkResult {
            sequence: 0, // Status updates use sequence 0
            data_outputs,
            processing_time_ms: 0.0,
            total_items_processed: 0,
        };

        let response = StreamResponse {
            response: Some(StreamResponseType::Result(chunk_result)),
        };

        // Send to client (non-blocking, ignore errors)
        let _ = self.client_tx.try_send(Ok(response));
    }

    /// Start the router - this runs until the session ends
    pub fn start(mut self) -> JoinHandle<()> {
        info!("🚀 Starting session router for session '{}'", self.session_id);

        self.running = true;
        let session_id = self.session_id.clone();

        // Spawn the main routing task
        tokio::spawn(async move {
            if let Err(e) = self.run().await {
                error!("Session router failed: {}", e);
            }
            info!("🛑 Session router stopped for session '{}'", session_id);
        })
    }

    /// Main routing loop - runs until session ends
    async fn run(&mut self) -> Result<(), Status> {
        info!("📡 Session router running - waiting for chunks from client...");

        // Process incoming packets from the client or shutdown signal
        loop {
            tokio::select! {
                packet = self.input_rx.recv() => {
                    match packet {
                        Some(packet) => {
                            debug!("📥 Router received packet from '{}' (seq: {})",
                                   packet.from_node, packet.sequence);

                            // Route the packet through the pipeline
                            if let Err(e) = self.route_packet(packet).await {
                                error!("Failed to route packet: {}", e);
                                // Continue processing other packets even if one fails
                            }
                        }
                        None => {
                            info!("✅ Session router input channel closed - shutting down");
                            break;
                        }
                    }
                }
                _ = self.shutdown_rx.recv() => {
                    info!("🛑 Session router received shutdown signal - stopping all processing");
                    break;
                }
            }
        }

        // Shutdown all node tasks
        self.shutdown_nodes().await;

        Ok(())
    }

    /// Route a packet through the pipeline
    async fn route_packet(&mut self, packet: DataPacket) -> Result<(), Status> {
        // If to_node is specified, route directly to that node (for client input)
        let downstream_nodes = if let Some(ref to_node) = packet.to_node {
            vec![to_node.clone()]
        } else {
            // Find downstream nodes for this packet based on manifest connections
            let nodes = self.get_downstream_nodes(&packet.from_node).await?;
            if nodes.is_empty() {
                // Terminal node - send to client
                self.send_to_client(packet).await?;
                return Ok(());
            }
            nodes
        };

        // Route to downstream nodes
        for next_node_id in downstream_nodes {
            // info!("🔀 Routing from '{}' → '{}'", packet.from_node, next_node_id);

            // Get or create the node task
            if !self.node_tasks.contains_key(&next_node_id) {
                self.start_node_task(next_node_id.clone()).await?;
            }

            // Check if this is a Python node - all Python nodes use multiprocessing
            #[cfg(feature = "multiprocess")]
            {
                let node_type = self.get_node_type(&next_node_id).await;
                let is_multiprocess = self.registry.is_python_node(&node_type);
                
                if is_multiprocess {
                    if let Some(ref executor) = self.multiprocess_executor {
                        debug!("📡 Routing to multiprocess node '{}' via IPC (using dedicated thread)", next_node_id);

                        // Send data via the dedicated IPC thread (no 50ms delay!)
                        let executor_clone = executor.clone();
                        let node_id_clone = next_node_id.clone();
                        let session_id_clone = packet.session_id.clone();
                        let data_clone = packet.data.clone();

                        // Send via dedicated IPC thread (async, no blocking)
                        tokio::spawn(async move {
                            match executor_clone.send_to_node_async(&node_id_clone, &session_id_clone, data_clone).await {
                                Ok(_) => {
                                    debug!("✅ Sent data to multiprocess node '{}' via dedicated IPC thread", node_id_clone);
                                }
                                Err(e) => {
                                    error!("Failed to send IPC data to node '{}': {}", node_id_clone, e);
                                }
                            }
                        });
                    } else {
                        warn!("Multiprocess node '{}' detected but no multiprocess executor available", next_node_id);
                    }
                }
            }

            // Also send via in-memory channel (for native nodes or as fallback)
            if let Some(node_input) = self.node_inputs.get(&next_node_id) {
                let packet_clone = packet.clone();
                if let Err(e) = node_input.send(packet_clone) {
                    error!("Failed to send packet to node '{}': {}", next_node_id, e);
                }
            } else {
                error!("No input channel for node '{}'", next_node_id);
            }
        }

        Ok(())
    }

    /// Start a task for a node
    async fn start_node_task(&mut self, node_id: String) -> Result<(), Status> {
        info!("🎯 Starting task for node '{}'", node_id);

        // Get or create the node
        let node = self.get_or_create_node(&node_id).await?;
        let is_streaming = self.registry.is_multi_output_streaming(&node.node_type());

        // Create input channel for this node
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<DataPacket>();
        self.node_inputs.insert(node_id.clone(), input_tx);

        // Clone what we need for the task
        let node_id_clone = node_id.clone();
        let session_id = self.session_id.clone();
        let router_tx = self.input_tx.clone();  // Send outputs back to router

        // Spawn the node task
        let task = tokio::spawn(async move {
            info!("⚡ Node '{}' task started (streaming: {})", node_id_clone, is_streaming);

            while let Some(packet) = input_rx.recv().await {
                debug!("📦 Node '{}' processing packet (seq: {})", node_id_clone, packet.sequence);

                if is_streaming {
                    // Streaming node - may produce multiple outputs
                    let mut output_count = 0;
                    let node_id_for_cb = node_id_clone.clone();
                    let session_id_for_cb = session_id.clone();
                    let router_tx_for_cb = router_tx.clone();
                    let packet_sequence = packet.sequence;

                    let result = node.process_streaming_async(
                        packet.data,
                        Some(session_id.clone()),
                        Box::new(move |output| {
                            output_count += 1;
                            let output_packet = DataPacket {
                                data: output,
                                from_node: node_id_for_cb.clone(),
                                to_node: None,
                                session_id: session_id_for_cb.clone(),
                                sequence: packet_sequence,
                                sub_sequence: output_count,
                            };

                            // Send output back to router for further routing
                            if let Err(e) = router_tx_for_cb.send(output_packet) {
                                error!("Failed to send output from '{}': {}", node_id_for_cb, e);
                                return Err(crate::Error::Execution("Channel closed".into()));
                            }
                            Ok(())
                        })
                    ).await;

                    match result {
                        Ok(count) => {
                            debug!("✅ Node '{}' produced {} outputs", node_id_clone, count);
                        }
                        Err(e) => {
                            error!("Streaming node '{}' failed: {}", node_id_clone, e);
                        }
                    }
                } else {
                    // Non-streaming node - single output
                    match node.process_async(packet.data).await {
                        Ok(output) => {
                            let output_packet = DataPacket {
                                data: output,
                                from_node: node_id_clone.clone(),
                                to_node: None,
                                session_id: session_id.clone(),
                                sequence: packet.sequence,
                                sub_sequence: 0,
                            };

                            // Send output back to router
                            if let Err(e) = router_tx.send(output_packet) {
                                error!("Failed to send output from '{}': {}", node_id_clone, e);
                            }
                        }
                        Err(e) => {
                            error!("Node '{}' failed: {}", node_id_clone, e);
                        }
                    }
                }
            }

            info!("⚡ Node '{}' task completed", node_id_clone);
        });

        self.node_tasks.insert(node_id, task);
        Ok(())
    }

    /// Get or create a node
    async fn get_or_create_node(&self, node_id: &str) -> Result<Arc<Box<dyn StreamingNode>>, Status> {
        let mut session = self.session.lock().await;

        // Check cache first
        if let Some(cached) = session.node_cache.get(node_id) {
            let cached_node = cached.clone();
            session.cache_hits += 1;
            return Ok(cached_node);
        }

        // Create new node
        session.cache_misses += 1;
        let spec = session.manifest.nodes.iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| Status::internal(format!("Node spec not found for '{}'", node_id)))?;

        // Pass session_id for multiprocess execution
        let session_id = Some(session.session_id.clone());
        let node = self.registry.create_node(&spec.node_type, node_id.to_string(), &spec.params, session_id)
            .map_err(|e| Status::internal(format!("Failed to create node '{}': {}", node_id, e)))?;

        let arc_node = Arc::new(node);
        session.node_cache.insert(node_id.to_string(), arc_node.clone());

        Ok(arc_node)
    }

    /// Get downstream nodes for a given node
    async fn get_downstream_nodes(&self, from_node_id: &str) -> Result<Vec<String>, Status> {
        let session = self.session.lock().await;

        let downstream: Vec<String> = session.manifest.connections.iter()
            .filter(|c| c.from == from_node_id)
            .map(|c| c.to.clone())
            .collect();

        Ok(downstream)
    }

    /// Get node type from node ID
    async fn get_node_type(&self, node_id: &str) -> String {
        let session = self.session.lock().await;
        session.manifest.nodes.iter()
            .find(|n| n.id == node_id)
            .map(|n| n.node_type.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Send a packet to the client
    async fn send_to_client(&self, packet: DataPacket) -> Result<(), Status> {
        debug!("📤 Sending to client from '{}' (seq: {}.{})",
               packet.from_node, packet.sequence, packet.sub_sequence);

        let output_buffer = convert_runtime_to_proto_data(packet.data);

        let mut data_outputs = HashMap::new();
        data_outputs.insert(packet.from_node, output_buffer);

        let chunk_result = ChunkResult {
            sequence: packet.sequence,
            data_outputs,
            processing_time_ms: 0.0,
            total_items_processed: packet.sub_sequence,
        };

        let response = StreamResponse {
            response: Some(StreamResponseType::Result(chunk_result)),
        };

        self.client_tx.send(Ok(response))
            .await
            .map_err(|_| Status::internal("Failed to send to client"))?;

        Ok(())
    }

    /// Shutdown all node tasks
    async fn shutdown_nodes(&mut self) {
        info!("Shutting down {} node tasks", self.node_tasks.len());

        // Close all node input channels
        self.node_inputs.clear();

        // Wait for all tasks to complete
        for (node_id, task) in self.node_tasks.drain() {
            debug!("Waiting for node '{}' to shutdown", node_id);
            let _ = task.await;
        }

        info!("All node tasks shut down");
    }

    /// Feed a chunk from the client into the router
    pub fn feed_chunk(
        &self,
        data: RuntimeData,
        from_node_id: String,
        sequence: u64,
    ) -> Result<(), String> {
        let packet = DataPacket {
            data,
            from_node: from_node_id,
            to_node: None,
            session_id: self.session_id.clone(),
            sequence,
            sub_sequence: 0,
        };

        self.input_tx.send(packet)
            .map_err(|e| format!("Failed to feed chunk: {}", e))
    }
}