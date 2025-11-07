# MultiprocessExecutor Contract

## NodeExecutor Trait Implementation

### Core Executor

```rust
use async_trait::async_trait;
use runtime::node_executor::{NodeExecutor, NodeContext, ExecutorError};

pub struct MultiprocessExecutor {
    /// Process manager for spawning/monitoring
    process_manager: ProcessManager,

    /// IPC channel registry
    channel_registry: ChannelRegistry,

    /// Active sessions and their processes
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,

    /// Runtime configuration
    config: RuntimeConfig,

    /// Event bus for process events
    event_bus: EventBus<ProcessEvent>,
}

#[async_trait]
impl NodeExecutor for MultiprocessExecutor {
    type Config = MultiprocessConfig;

    async fn initialize(&mut self, config: Self::Config) -> Result<(), ExecutorError> {
        // Initialize iceoryx2 runtime
        self.process_manager.initialize().await?;
        self.channel_registry.initialize()?;

        // Setup process event monitoring
        self.setup_event_monitoring().await?;

        Ok(())
    }

    async fn execute_node(
        &self,
        node_type: &str,
        node_id: &str,
        context: NodeContext,
    ) -> Result<NodeHandle, ExecutorError> {
        // Spawn process for node
        let process = self.spawn_node_process(
            node_type,
            node_id,
            &context,
        ).await?;

        // Create IPC channels for node I/O
        let channels = self.setup_node_channels(
            node_id,
            &context.inputs,
            &context.outputs,
        ).await?;

        // Return handle for node control
        Ok(NodeHandle::Multiprocess(MultiprocessHandle {
            process_id: process.id,
            node_id: node_id.to_string(),
            channels,
            executor: self.clone(),
        }))
    }

    async fn connect_nodes(
        &self,
        from_node: &str,
        from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> Result<(), ExecutorError> {
        // Create shared memory channel between nodes
        let channel = self.channel_registry.create_channel(
            &format!("{}_{}_{}", from_node, to_node, from_port),
            self.config.channel_capacity,
        )?;

        // Bind publisher/subscriber
        self.bind_publisher(from_node, from_port, &channel)?;
        self.bind_subscriber(to_node, to_port, &channel)?;

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), ExecutorError> {
        // Terminate all processes gracefully
        for (_, session) in self.sessions.write().await.drain() {
            self.terminate_session(session).await?;
        }

        // Cleanup IPC resources
        self.channel_registry.cleanup()?;

        Ok(())
    }
}
```

### Process Management

```rust
struct ProcessManager {
    /// Child process handles
    processes: Arc<RwLock<HashMap<u32, ProcessHandle>>>,

    /// Process spawn configuration
    spawn_config: SpawnConfig,

    /// Process monitor task
    monitor_handle: Option<JoinHandle<()>>,
}

impl ProcessManager {
    async fn spawn_node(
        &self,
        node_type: &str,
        node_id: &str,
        config: &NodeConfig,
    ) -> Result<ProcessHandle, ProcessError> {
        // Build command for Python subprocess
        let mut command = Command::new(&self.spawn_config.python_executable);

        // Add multiprocess runner module
        command.args([
            "-m", "remotemedia.core.multiprocess.runner",
            "--node-type", node_type,
            "--node-id", node_id,
        ]);

        // Set environment variables
        for (key, value) in &config.environment {
            command.env(key, value);
        }

        // Configure process group for cleanup
        command.process_group(0);

        // Spawn with stdout/stderr capture
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Create process handle
        let handle = ProcessHandle {
            id: child.id(),
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
            child: Arc::new(Mutex::new(child)),
            status: ProcessStatus::Initializing,
            started_at: Instant::now(),
        };

        // Register for monitoring
        self.processes.write().await.insert(handle.id, handle.clone());

        Ok(handle)
    }

    async fn monitor_processes(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(100));

        loop {
            interval.tick().await;

            let processes = self.processes.read().await;
            for (pid, handle) in processes.iter() {
                // Check process status via try_wait (non-blocking)
                if let Ok(Some(status)) = handle.child.lock().await.try_wait() {
                    // Process exited, handle cleanup
                    self.handle_process_exit(*pid, status).await;
                }
            }
        }
    }

    async fn handle_process_exit(&self, pid: u32, status: ExitStatus) {
        // Determine exit reason
        let reason = match status.code() {
            Some(0) => ExitReason::Normal,
            Some(code) => ExitReason::Error(code),
            None => ExitReason::Killed, // Terminated by signal
        };

        // Emit process exit event
        self.event_bus.emit(ProcessEvent::ProcessExited {
            pid,
            reason,
            timestamp: Instant::now(),
        }).await;

        // Remove from active processes
        self.processes.write().await.remove(&pid);
    }
}
```

### Channel Management

```rust
struct ChannelRegistry {
    /// iceoryx2 node for IPC
    iox_node: Node,

    /// Active channels
    channels: Arc<RwLock<HashMap<String, ChannelHandle>>>,

    /// Channel metrics
    metrics: ChannelMetrics,
}

impl ChannelRegistry {
    fn create_channel(
        &self,
        name: &str,
        capacity: usize,
    ) -> Result<ChannelHandle, ChannelError> {
        // Create iceoryx2 service
        let service = self.iox_node
            .service_builder(name)
            .publish_subscribe::<RuntimeData>()
            .max_publishers(10)
            .max_subscribers(10)
            .history_size(capacity)
            .create()?;

        let handle = ChannelHandle {
            name: name.to_string(),
            service: Arc::new(service),
            capacity,
            publishers: Arc::new(RwLock::new(Vec::new())),
            subscribers: Arc::new(RwLock::new(Vec::new())),
            stats: ChannelStats::default(),
        };

        self.channels.write().await.insert(name.to_string(), handle.clone());

        Ok(handle)
    }

    fn create_publisher(
        &self,
        channel: &ChannelHandle,
    ) -> Result<Publisher<RuntimeData>, ChannelError> {
        let publisher = channel.service.publisher()
            .max_loaned_samples(10)
            .create()?;

        channel.publishers.write().await.push(publisher.id());

        Ok(publisher)
    }

    fn create_subscriber(
        &self,
        channel: &ChannelHandle,
    ) -> Result<Subscriber<RuntimeData>, ChannelError> {
        let subscriber = channel.service.subscriber()
            .queue_capacity(10)
            .create()?;

        channel.subscribers.write().await.push(subscriber.id());

        Ok(subscriber)
    }
}
```

### Session State Management

```rust
struct SessionState {
    /// Session identifier
    session_id: String,

    /// Pipeline configuration
    pipeline: PipelineConfig,

    /// Active node processes
    node_processes: HashMap<String, ProcessHandle>,

    /// IPC channels for this session
    channels: HashMap<String, ChannelHandle>,

    /// Session status
    status: SessionStatus,

    /// Creation timestamp
    created_at: Instant,
}

impl MultiprocessExecutor {
    pub async fn create_session(
        &self,
        session_id: String,
        pipeline: PipelineConfig,
    ) -> Result<SessionHandle, ExecutorError> {
        // Validate pipeline configuration
        self.validate_pipeline(&pipeline)?;

        // Check process limits
        if let Some(limit) = self.config.max_processes_per_session {
            if pipeline.nodes.len() > limit {
                return Err(ExecutorError::ResourceLimit(
                    format!("Pipeline exceeds limit of {} processes", limit)
                ));
            }
        }

        // Create session state
        let mut session = SessionState {
            session_id: session_id.clone(),
            pipeline: pipeline.clone(),
            node_processes: HashMap::new(),
            channels: HashMap::new(),
            status: SessionStatus::Initializing,
            created_at: Instant::now(),
        };

        // Spawn all node processes
        for node_config in &pipeline.nodes {
            let process = self.process_manager.spawn_node(
                &node_config.node_type,
                &node_config.node_id,
                &node_config.config,
            ).await?;

            session.node_processes.insert(
                node_config.node_id.clone(),
                process,
            );
        }

        // Create inter-node channels
        for connection in &pipeline.connections {
            let channel = self.channel_registry.create_channel(
                &format!("{}_{}", connection.from_node, connection.to_node),
                self.config.channel_capacity,
            )?;

            session.channels.insert(channel.name.clone(), channel);
        }

        // Wait for all nodes to be ready
        self.wait_for_initialization(&mut session).await?;

        session.status = SessionStatus::Ready;

        // Store session
        self.sessions.write().await.insert(session_id.clone(), session);

        Ok(SessionHandle {
            session_id,
            executor: self.clone(),
        })
    }

    async fn wait_for_initialization(
        &self,
        session: &mut SessionState,
    ) -> Result<(), ExecutorError> {
        let timeout = Duration::from_secs(self.config.init_timeout_secs);
        let start = Instant::now();

        for (node_id, process) in &session.node_processes {
            // Poll process status until ready
            loop {
                if start.elapsed() > timeout {
                    return Err(ExecutorError::InitTimeout(node_id.clone()));
                }

                // Check process health
                if !process.is_alive() {
                    return Err(ExecutorError::ProcessCrashed(node_id.clone()));
                }

                // Check if node reported ready via IPC
                if self.is_node_ready(node_id).await? {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        Ok(())
    }

    pub async fn terminate_session(
        &self,
        session_id: &str,
    ) -> Result<(), ExecutorError> {
        let mut sessions = self.sessions.write().await;

        if let Some(mut session) = sessions.remove(session_id) {
            session.status = SessionStatus::Terminating;

            // Graceful shutdown of all processes
            for (_, process) in session.node_processes.drain() {
                self.process_manager.terminate_process(
                    process,
                    Duration::from_secs(5),
                ).await?;
            }

            // Cleanup channels
            for (_, channel) in session.channels.drain() {
                self.channel_registry.destroy_channel(channel)?;
            }

            Ok(())
        } else {
            Err(ExecutorError::SessionNotFound(session_id.to_string()))
        }
    }
}
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum MultiprocessError {
    #[error("Failed to spawn process: {0}")]
    SpawnError(String),

    #[error("Process {node_id} crashed with exit code {code}")]
    ProcessCrashed { node_id: String, code: i32 },

    #[error("Initialization timeout for node {0}")]
    InitTimeout(String),

    #[error("Channel error: {0}")]
    ChannelError(#[from] ChannelError),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("Pipeline terminated due to node failure: {0}")]
    PipelineTerminated(String),
}

impl From<MultiprocessError> for ExecutorError {
    fn from(err: MultiprocessError) -> Self {
        ExecutorError::Custom(Box::new(err))
    }
}
```

### Integration with Runtime

```rust
// In runtime initialization
let executor = MultiprocessExecutor::new(MultiprocessConfig {
    max_processes_per_session: Some(10),
    channel_capacity: 100,
    init_timeout_secs: 30,
    python_executable: PathBuf::from("python"),
    enable_backpressure: true,
});

// Register executor for Python nodes
runtime.register_executor("python", Box::new(executor));

// When creating a pipeline with Python nodes
let pipeline = Pipeline::new()
    .add_node(NodeConfig {
        node_type: "lfm2_audio",
        node_id: "s2s",
        executor: Some("python"), // Uses MultiprocessExecutor
        config: json!({
            "model": "large",
            "device": "cuda",
        }),
    })
    .add_node(NodeConfig {
        node_type: "vibe_voice",
        node_id: "tts",
        executor: Some("python"), // Uses MultiprocessExecutor
        config: json!({
            "voice": "sarah",
        }),
    });

// Runtime automatically uses MultiprocessExecutor for these nodes
runtime.create_session("session_123", pipeline).await?;
```