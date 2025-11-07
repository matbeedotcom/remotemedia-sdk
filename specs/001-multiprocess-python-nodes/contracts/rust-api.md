# Rust API Contract

## Process Management API

### Create Process

```rust
/// Spawn a new Python node process
pub async fn spawn_node_process(
    node_type: &str,
    session_id: &str,
    config: NodeConfig,
) -> Result<NodeProcess, ProcessError> {
    // Implementation
}

pub struct NodeConfig {
    pub environment: HashMap<String, String>,
    pub working_directory: PathBuf,
    pub python_path: Option<PathBuf>,
    pub init_timeout: Duration,
    pub max_memory: Option<usize>,
}

pub enum ProcessError {
    SpawnFailed(String),
    InitTimeout,
    ResourceLimit,
    InvalidConfig(String),
}
```

### Monitor Process

```rust
/// Register process exit handler
pub fn on_process_exit(
    process_id: u32,
    handler: impl Fn(ExitStatus) + Send + 'static,
) -> Result<(), ProcessError> {
    // Implementation
}

pub struct ExitStatus {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub timestamp: SystemTime,
    pub reason: ExitReason,
}

pub enum ExitReason {
    Normal,
    Crash,
    Killed,
    Timeout,
}
```

### Terminate Process

```rust
/// Gracefully terminate a process
pub async fn terminate_process(
    process_id: u32,
    grace_period: Duration,
) -> Result<(), ProcessError> {
    // Implementation
}

/// Force kill a process
pub async fn kill_process(process_id: u32) -> Result<(), ProcessError> {
    // Implementation
}
```

## IPC Channel API

### Create Channel

```rust
/// Create a shared memory channel
pub fn create_channel(
    name: &str,
    capacity: usize,
    backpressure: bool,
) -> Result<SharedMemoryChannel, ChannelError> {
    // Implementation
}

pub enum ChannelError {
    AlreadyExists,
    InvalidCapacity,
    SystemLimit,
    IceoryxError(String),
}
```

### Publish Data

```rust
/// Publish data to channel (blocking if backpressure enabled)
pub fn publish(
    channel: &SharedMemoryChannel,
    data: RuntimeData,
) -> Result<(), PublishError> {
    // Implementation
}

pub enum PublishError {
    ChannelFull,
    InvalidData,
    NotPublisher,
    ChannelClosed,
}
```

### Subscribe Data

```rust
/// Subscribe to channel data
pub fn subscribe(
    channel: &SharedMemoryChannel,
) -> Result<Receiver<RuntimeData>, SubscribeError> {
    // Implementation
}

pub enum SubscribeError {
    AlreadySubscribed,
    ChannelClosed,
    NotAuthorized,
}
```

## Session Management API

### Create Session

```rust
/// Create a new pipeline session
pub async fn create_session(
    session_id: &str,
    pipeline: PipelineConfig,
) -> Result<Session, SessionError> {
    // Implementation
}

pub struct Session {
    pub id: String,
    pub processes: Vec<NodeProcess>,
    pub channels: Vec<SharedMemoryChannel>,
    pub status: SessionStatus,
    pub created_at: SystemTime,
}

pub enum SessionStatus {
    Initializing,
    Ready,
    Running,
    Stopping,
    Stopped,
    Error(String),
}
```

### Initialize Pipeline

```rust
/// Initialize all nodes in session
pub async fn initialize_pipeline(
    session: &mut Session,
    progress_handler: impl Fn(InitProgress) + Send + 'static,
) -> Result<(), InitError> {
    // Implementation
}

pub struct InitProgress {
    pub node_type: String,
    pub status: InitStatus,
    pub progress: f32, // 0.0 to 1.0
    pub message: String,
}

pub enum InitStatus {
    Starting,
    LoadingModel,
    Connecting,
    Ready,
    Failed(String),
}
```

### Cleanup Session

```rust
/// Terminate session and cleanup resources
pub async fn cleanup_session(
    session_id: &str,
) -> Result<(), CleanupError> {
    // Implementation
}

pub enum CleanupError {
    ProcessesStillRunning,
    ChannelCleanupFailed,
    ResourceLeak(String),
}
```

## Data Transfer API

### Zero-Copy Transfer

```rust
/// Get zero-copy loan for writing
pub fn loan_uninit(
    channel: &SharedMemoryChannel,
    size: usize,
) -> Result<SampleMut<RuntimeData>, LoanError> {
    // Implementation
}

/// Send loaned sample
pub fn send(sample: SampleMut<RuntimeData>) -> Result<(), SendError> {
    // Implementation
}

/// Receive with zero-copy
pub fn receive(
    receiver: &Receiver<RuntimeData>,
) -> Result<Sample<RuntimeData>, ReceiveError> {
    // Implementation
}
```

## Configuration API

### Runtime Configuration

```rust
pub struct RuntimeConfig {
    /// Maximum processes per session (None = unlimited)
    pub max_processes_per_session: Option<usize>,

    /// Shared memory segment size
    pub shm_segment_size: usize,

    /// Process init timeout
    pub init_timeout: Duration,

    /// Cleanup grace period
    pub cleanup_grace_period: Duration,

    /// Enable debug logging
    pub debug_mode: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_processes_per_session: None,
            shm_segment_size: 256 * 1024 * 1024, // 256MB
            init_timeout: Duration::from_secs(30),
            cleanup_grace_period: Duration::from_secs(5),
            debug_mode: false,
        }
    }
}
```