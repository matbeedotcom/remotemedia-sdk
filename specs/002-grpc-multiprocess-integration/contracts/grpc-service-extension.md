# gRPC Service Extension Contract

## Overview

Extends the existing gRPC `PipelineExecutionService` and `StreamingPipelineService` to support multiprocess Python node execution while maintaining backward compatibility with existing clients.

## Executor Registry API

### Initialization

```rust
/// Initialize executor registry with default mappings
pub fn initialize_registry() -> ExecutorRegistry {
    let mut registry = ExecutorRegistry::new();

    // Register Python nodes for multiprocess execution
    registry.register_pattern(
        PatternRule {
            pattern: ".*Node$".to_string(),  // Matches WhisperNode, LFM2Node, etc.
            executor_type: ExecutorType::Multiprocess,
            priority: 100,
        }
    );

    // Register native Rust nodes
    registry.register_explicit("AudioChunkerNode", ExecutorType::Native);
    registry.register_explicit("FastResampleNode", ExecutorType::Native);
    registry.register_explicit("SileroVADNode", ExecutorType::Native);

    // Set default for unmatched nodes
    registry.set_default(ExecutorType::Native);

    registry
}
```

### Runtime Usage

```rust
/// Determine executor for a given node type
pub fn get_executor_for_node(&self, node_type: &str) -> ExecutorType {
    // 1. Check explicit mappings first
    if let Some(executor_type) = self.node_type_mappings.get(node_type) {
        return *executor_type;
    }

    // 2. Check pattern rules (sorted by priority)
    for rule in &self.pattern_rules {
        if rule.matches(node_type) {
            return rule.executor_type;
        }
    }

    // 3. Fall back to default
    self.default_executor
}
```

---

## Manifest Parsing Extension

### Parse Multiprocess Configuration

```rust
/// Extract multiprocess config from manifest metadata
pub fn parse_multiprocess_config(
    manifest: &ProtoPipelineManifest
) -> Result<Option<MultiprocessConfig>, ServiceError> {
    let metadata = manifest.metadata.as_ref()
        .ok_or(ServiceError::Validation("Missing metadata".to_string()))?;

    // Check for multiprocess configuration in metadata
    if let Some(multiprocess_json) = metadata.additional_properties.get("multiprocess") {
        let config: MultiprocessConfig = serde_json::from_value(multiprocess_json.clone())
            .map_err(|e| ServiceError::Validation(format!("Invalid multiprocess config: {}", e)))?;

        // Validate against global limits
        validate_against_global_limits(&config)?;

        Ok(Some(config))
    } else {
        // Use defaults from runtime.toml
        Ok(None)
    }
}
```

### Validation

```rust
/// Validate configuration doesn't exceed global limits
fn validate_against_global_limits(config: &MultiprocessConfig) -> Result<(), ServiceError> {
    const GLOBAL_MAX_PROCESSES: usize = 100;
    const GLOBAL_MAX_CHANNEL_CAPACITY: usize = 10000;

    if let Some(max_processes) = config.max_processes_per_session {
        if max_processes > GLOBAL_MAX_PROCESSES {
            return Err(ServiceError::ResourceLimit(
                format!("Requested {} processes exceeds global limit of {}",
                    max_processes, GLOBAL_MAX_PROCESSES)
            ));
        }
    }

    if config.channel_capacity > GLOBAL_MAX_CHANNEL_CAPACITY {
        return Err(ServiceError::ResourceLimit(
            format!("Requested channel capacity {} exceeds global limit of {}",
                config.channel_capacity, GLOBAL_MAX_CHANNEL_CAPACITY)
        ));
    }

    Ok(())
}
```

---

## Extended ExecutePipeline RPC

### Request Flow with Multiprocess Support

```rust
#[tonic::async_trait]
impl PipelineExecutionService for ExecutionServiceImpl {
    async fn execute_pipeline(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<ExecuteResponse>, Status> {
        // 1. Extract and validate manifest
        let execute_req = request.into_inner();
        let manifest = execute_req.manifest
            .ok_or_else(|| Status::invalid_argument("Missing manifest"))?;

        // 2. Parse multiprocess configuration
        let multiprocess_config = self.parse_multiprocess_config(&manifest)?;

        // 3. Create session execution context
        let session_id = generate_session_id();
        let mut session_ctx = SessionExecutionContext::new(
            session_id.clone(),
            multiprocess_config,
        );

        // 4. Assign nodes to executors
        for node in &manifest.nodes {
            let executor_type = self.registry.get_executor_for_node(&node.node_type);
            session_ctx.assign_node(node.id.clone(), executor_type);
        }

        // 5. Initialize executors for this session
        session_ctx.initialize_executors(&self.executor_factory).await?;

        // 6. Create data bridges for inter-executor connections
        for connection in &manifest.connections {
            let source_executor = session_ctx.get_node_executor(&connection.from);
            let target_executor = session_ctx.get_node_executor(&connection.to);

            if source_executor != target_executor {
                session_ctx.create_data_bridge(
                    connection.from.clone(),
                    connection.to.clone(),
                    source_executor,
                    target_executor,
                ).await?;
            }
        }

        // 7. Execute pipeline
        let result = session_ctx.execute_pipeline(manifest).await?;

        // 8. Cleanup
        session_ctx.terminate().await?;

        Ok(Response::new(result))
    }
}
```

---

## Session Context Management

### Initialization

```rust
impl SessionExecutionContext {
    /// Create new session with optional multiprocess configuration
    pub fn new(
        session_id: String,
        multiprocess_config: Option<MultiprocessConfig>,
    ) -> Self {
        Self {
            session_id,
            executors: HashMap::new(),
            node_assignments: HashMap::new(),
            data_bridges: Vec::new(),
            status: SessionStatus::Initializing,
            created_at: Instant::now(),
            multiprocess_config,
        }
    }

    /// Initialize executors based on node assignments
    pub async fn initialize_executors(
        &mut self,
        factory: &ExecutorFactory,
    ) -> Result<(), ServiceError> {
        // Determine which executor types are needed
        let required_executors: HashSet<ExecutorType> =
            self.node_assignments.values().cloned().collect();

        for executor_type in required_executors {
            let executor = match executor_type {
                ExecutorType::Native => factory.create_native_executor(),
                ExecutorType::Multiprocess => {
                    let config = self.multiprocess_config.clone()
                        .unwrap_or_else(MultiprocessConfig::from_default_file)?;
                    factory.create_multiprocess_executor(config)
                },
                ExecutorType::Wasm => factory.create_wasm_executor(),
            }?;

            self.executors.insert(executor_type, executor);
        }

        self.status = SessionStatus::Ready;
        Ok(())
    }
}
```

### Cleanup

```rust
impl SessionExecutionContext {
    /// Terminate session and cleanup all resources
    pub async fn terminate(&mut self) -> Result<(), ServiceError> {
        self.status = SessionStatus::Terminating;

        // 1. Close all data bridges
        for bridge in &mut self.data_bridges {
            bridge.close().await?;
        }

        // 2. Terminate executors
        for (executor_type, executor) in &self.executors {
            match executor_type {
                ExecutorType::Multiprocess => {
                    // Terminate all Python processes
                    executor.terminate_session(&self.session_id).await?;
                },
                _ => {
                    // Native/WASM cleanup
                    executor.cleanup(&self.session_id).await?;
                }
            }
        }

        self.status = SessionStatus::Terminated;
        Ok(())
    }
}
```

---

## Progress Streaming Extension

### Initialize Progress Updates

```rust
/// Send initialization progress for multiprocess nodes
pub async fn stream_init_progress(
    &self,
    session_id: &str,
    response_tx: mpsc::Sender<Result<StreamResponse, Status>>,
) -> Result<(), ServiceError> {
    // Get multiprocess executor for this session
    if let Some(mp_executor) = self.executors.get(&ExecutorType::Multiprocess) {
        // Subscribe to initialization progress
        let mut progress_rx = mp_executor.get_init_progress(session_id).await?;

        while let Some(progress) = progress_rx.recv().await {
            // Convert to gRPC message
            let progress_msg = StreamResponse {
                result: Some(stream_response::Result::InitProgress(InitProgressUpdate {
                    node_id: progress.node_id,
                    status: match progress.status {
                        InitStatus::Starting => "starting",
                        InitStatus::LoadingModel => "loading_model",
                        InitStatus::Ready => "ready",
                        InitStatus::Error => "error",
                    }.to_string(),
                    progress: progress.progress,
                    message: progress.message,
                })),
            };

            // Send to client
            response_tx.send(Ok(progress_msg)).await
                .map_err(|_| ServiceError::Internal("Failed to send progress".to_string()))?;
        }
    }

    Ok(())
}
```

---

## Error Handling

### Multiprocess-Specific Errors

```rust
/// Handle process crash during execution
pub async fn handle_process_crash(
    &mut self,
    node_id: &str,
    exit_code: i32,
) -> Result<(), ServiceError> {
    // Log the crash
    error!("Process for node {} crashed with exit code {}", node_id, exit_code);

    // Update session status
    self.status = SessionStatus::Error(format!(
        "Node {} process crashed (exit code: {})", node_id, exit_code
    ));

    // Terminate entire pipeline (fail-fast)
    self.terminate().await?;

    // Return error to client
    Err(ServiceError::NodeExecution {
        node_id: node_id.to_string(),
        message: format!("Process crashed with exit code {}", exit_code),
    })
}
```

### Timeout Handling

```rust
/// Handle node initialization timeout
pub async fn handle_init_timeout(
    &mut self,
    node_id: &str,
    timeout_secs: u64,
) -> Result<(), ServiceError> {
    error!("Node {} failed to initialize within {}s", node_id, timeout_secs);

    self.status = SessionStatus::Error(format!(
        "Node {} initialization timeout", node_id
    ));

    self.terminate().await?;

    Err(ServiceError::NodeExecution {
        node_id: node_id.to_string(),
        message: format!("Initialization timeout after {}s", timeout_secs),
    })
}
```

---

## Backward Compatibility

### Existing Clients

**No changes required** for clients that:
- Use manifest.v1.json format
- Call existing ExecutePipeline or StreamPipeline RPCs
- Do not use multiprocess configuration

**Behavior**:
- Native Rust nodes execute as before
- Python nodes that match multiprocess patterns automatically use multiprocess executor
- No performance regression for non-Python pipelines

### Opt-In Multiprocess

Clients can opt-in to multiprocess execution by:
1. Adding multiprocess configuration to manifest metadata
2. Ensuring Python nodes are registered in python-client SDK
3. No code changes to RPC invocation

**Example Manifest**:
```json
{
  "version": "v1",
  "metadata": {
    "name": "speech-to-speech",
    "multiprocess": {
      "max_processes_per_session": 10,
      "channel_capacity": 100,
      "init_timeout_secs": 30
    }
  },
  "nodes": [...],
  "connections": [...]
}
```

---

## Testing Contract

### Integration Test Requirements

1. **Manifest Parsing**: Parse manifest with multiprocess config
2. **Executor Routing**: Verify correct executor assigned per node type
3. **Session Lifecycle**: Create, execute, terminate session
4. **Process Management**: Verify processes spawned and cleaned up
5. **Data Bridge**: Test data transfer across executor boundaries
6. **Error Handling**: Test crash recovery and timeout handling
7. **Backward Compatibility**: Run existing manifests without changes

### Test Fixtures

```rust
// Example test manifest with mixed executors
const TEST_MANIFEST: &str = r#"
{
  "version": "v1",
  "metadata": {
    "name": "test-pipeline",
    "multiprocess": {
      "max_processes_per_session": 3,
      "channel_capacity": 50
    }
  },
  "nodes": [
    { "id": "input", "node_type": "AudioChunkerNode", "params": {} },
    { "id": "asr", "node_type": "WhisperNode", "params": {} },
    { "id": "tts", "node_type": "VibeVoiceNode", "params": {} }
  ],
  "connections": [
    { "from": "input", "to": "asr" },
    { "from": "asr", "to": "tts" }
  ]
}
"#;
```
