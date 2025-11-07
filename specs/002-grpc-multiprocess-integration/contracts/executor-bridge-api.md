# Executor Bridge API Contract

## Overview

The Executor Bridge provides a unified interface for routing node execution to different executor types (Native, Multiprocess, WASM) and handling data conversion at executor boundaries.

## Core Bridge Interface

### ExecutorBridge Trait

```rust
#[async_trait]
pub trait ExecutorBridge: Send + Sync {
    /// Execute a node using the appropriate executor
    async fn execute_node(
        &self,
        node_id: &str,
        node_type: &str,
        input_data: RuntimeData,
        params: &serde_json::Value,
    ) -> Result<RuntimeData, ServiceError>;

    /// Initialize a node (for stateful nodes like AI models)
    async fn initialize_node(
        &self,
        node_id: &str,
        node_type: &str,
        params: &serde_json::Value,
    ) -> Result<(), ServiceError>;

    /// Cleanup node resources
    async fn cleanup_node(&self, node_id: &str) -> Result<(), ServiceError>;

    /// Get executor type for this bridge
    fn executor_type(&self) -> ExecutorType;
}
```

---

## Bridge Implementations

### NativeExecutorBridge

Executes nodes in the current process using Rust code.

```rust
pub struct NativeExecutorBridge {
    node_registry: Arc<NodeRegistry>,
    metrics: Arc<BridgeMetrics>,
}

#[async_trait]
impl ExecutorBridge for NativeExecutorBridge {
    async fn execute_node(
        &self,
        node_id: &str,
        node_type: &str,
        input_data: RuntimeData,
        params: &serde_json::Value,
    ) -> Result<RuntimeData, ServiceError> {
        let start = Instant::now();

        // Look up node implementation
        let node = self.node_registry.get(node_type)
            .ok_or_else(|| ServiceError::Validation(format!("Unknown node type: {}", node_type)))?;

        // Execute directly in current process
        let output_data = node.process(input_data, params).await
            .map_err(|e| ServiceError::NodeExecution {
                node_id: node_id.to_string(),
                message: e.to_string(),
            })?;

        // Record metrics
        self.metrics.record_execution(node_id, start.elapsed());

        Ok(output_data)
    }

    async fn initialize_node(
        &self,
        node_id: &str,
        node_type: &str,
        params: &serde_json::Value,
    ) -> Result<(), ServiceError> {
        // Native nodes typically don't require initialization
        // (state is managed per-call)
        Ok(())
    }

    async fn cleanup_node(&self, node_id: &str) -> Result<(), ServiceError> {
        // No cleanup needed for stateless native nodes
        Ok(())
    }

    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Native
    }
}
```

---

### MultiprocessExecutorBridge

Executes nodes in separate Python processes using the multiprocess executor from spec 001.

```rust
pub struct MultiprocessExecutorBridge {
    executor: Arc<MultiprocessExecutor>,
    session_id: String,
    data_converter: Arc<IPCDataConverter>,
    metrics: Arc<BridgeMetrics>,
}

#[async_trait]
impl ExecutorBridge for MultiprocessExecutorBridge {
    async fn execute_node(
        &self,
        node_id: &str,
        node_type: &str,
        input_data: RuntimeData,
        params: &serde_json::Value,
    ) -> Result<RuntimeData, ServiceError> {
        let start = Instant::now();

        // Convert RuntimeData to shared memory format
        let ipc_data = self.data_converter.to_ipc(input_data).await
            .map_err(|e| ServiceError::Internal(format!("Data conversion failed: {}", e)))?;

        // Send to Python process via IPC channel
        self.executor.send_data(&self.session_id, node_id, ipc_data).await
            .map_err(|e| ServiceError::NodeExecution {
                node_id: node_id.to_string(),
                message: format!("IPC send failed: {}", e),
            })?;

        // Receive result from Python process
        let ipc_result = self.executor.receive_data(&self.session_id, node_id).await
            .map_err(|e| ServiceError::NodeExecution {
                node_id: node_id.to_string(),
                message: format!("IPC receive failed: {}", e),
            })?;

        // Convert back to RuntimeData
        let output_data = self.data_converter.from_ipc(ipc_result).await
            .map_err(|e| ServiceError::Internal(format!("Data conversion failed: {}", e)))?;

        // Record metrics
        self.metrics.record_execution(node_id, start.elapsed());

        Ok(output_data)
    }

    async fn initialize_node(
        &self,
        node_id: &str,
        node_type: &str,
        params: &serde_json::Value,
    ) -> Result<(), ServiceError> {
        // Spawn Python process for this node
        let node_ctx = NodeContext {
            node_id: node_id.to_string(),
            node_type: node_type.to_string(),
            params: params.clone(),
            session_id: Some(self.session_id.clone()),
            metadata: HashMap::new(),
        };

        self.executor.initialize(&node_ctx).await
            .map_err(|e| ServiceError::NodeExecution {
                node_id: node_id.to_string(),
                message: format!("Initialization failed: {}", e),
            })?;

        Ok(())
    }

    async fn cleanup_node(&self, node_id: &str) -> Result<(), ServiceError> {
        // Terminate Python process for this node
        self.executor.terminate_node(&self.session_id, node_id).await
            .map_err(|e| ServiceError::Internal(format!("Cleanup failed: {}", e)))?;

        Ok(())
    }

    fn executor_type(&self) -> ExecutorType {
        ExecutorType::Multiprocess
    }
}
```

---

## Data Conversion API

### IPCDataConverter

Handles conversion between RuntimeData (native memory) and iceoryx2 shared memory.

```rust
pub struct IPCDataConverter {
    channel_registry: Arc<ChannelRegistry>,
}

impl IPCDataConverter {
    /// Convert RuntimeData to shared memory format
    pub async fn to_ipc(&self, data: RuntimeData) -> Result<IPCMessage, ConversionError> {
        match data.data_type {
            DataType::Audio => {
                // Extract audio buffer
                let audio = data.as_audio()
                    .ok_or(ConversionError::TypeMismatch)?;

                // Allocate shared memory sample
                let mut sample = self.channel_registry
                    .get_publisher(&data.session_id)?
                    .loan_uninit()?;

                // Copy audio data to shared memory
                unsafe {
                    let sample_ptr = sample.as_mut_ptr();
                    std::ptr::copy_nonoverlapping(
                        audio.samples.as_ptr(),
                        sample_ptr as *mut f32,
                        audio.num_samples,
                    );
                }

                // Send shared memory reference
                sample.send()?;

                Ok(IPCMessage::SharedMemoryRef(sample.id()))
            },
            DataType::Text => {
                // Text can be small enough to inline
                Ok(IPCMessage::Inline(data.payload))
            },
            DataType::Tensor => {
                // Large tensors use shared memory
                self.copy_to_shared_memory(data).await
            },
            DataType::Json => {
                // JSON metadata is inlined
                Ok(IPCMessage::Inline(data.payload))
            },
        }
    }

    /// Convert shared memory format to RuntimeData
    pub async fn from_ipc(&self, msg: IPCMessage) -> Result<RuntimeData, ConversionError> {
        match msg {
            IPCMessage::SharedMemoryRef(sample_id) => {
                // Get shared memory sample
                let sample = self.channel_registry
                    .get_subscriber(&self.session_id)?
                    .receive()?;

                // Copy from shared memory to RuntimeData
                let audio = AudioBuffer {
                    samples: Arc::new(unsafe {
                        std::slice::from_raw_parts(
                            sample.as_ptr() as *const f32,
                            sample.len() / std::mem::size_of::<f32>(),
                        ).to_vec()
                    }),
                    sample_rate: sample.metadata().sample_rate,
                    channels: sample.metadata().channels,
                    num_samples: sample.len() / std::mem::size_of::<f32>(),
                    format: AudioFormat::F32,
                };

                Ok(RuntimeData::audio(audio))
            },
            IPCMessage::Inline(payload) => {
                // Inline data is already in RuntimeData format
                Ok(RuntimeData {
                    data_type: DataType::from_header(&payload[0..64])?,
                    payload,
                    session_id: self.session_id.clone(),
                    timestamp: SystemTime::now(),
                })
            },
        }
    }
}
```

---

## DataBridge Implementation

### Cross-Executor Data Flow

```rust
pub struct DataBridge {
    source_bridge: Arc<dyn ExecutorBridge>,
    target_bridge: Arc<dyn ExecutorBridge>,
    converter: Arc<IPCDataConverter>,
    buffer: Arc<RwLock<VecDeque<RuntimeData>>>,
    metrics: Arc<DataBridgeMetrics>,
}

impl DataBridge {
    /// Transfer data from source executor to target executor
    pub async fn transfer(
        &self,
        data: RuntimeData,
    ) -> Result<RuntimeData, ServiceError> {
        let start = Instant::now();

        // Convert data based on executor types
        let converted_data = match (
            self.source_bridge.executor_type(),
            self.target_bridge.executor_type()
        ) {
            (ExecutorType::Native, ExecutorType::Multiprocess) => {
                // Native → IPC conversion
                let ipc_msg = self.converter.to_ipc(data).await?;
                self.converter.from_ipc(ipc_msg).await?
            },
            (ExecutorType::Multiprocess, ExecutorType::Native) => {
                // IPC → Native conversion
                let ipc_msg = self.converter.to_ipc(data).await?;
                self.converter.from_ipc(ipc_msg).await?
            },
            (ExecutorType::Multiprocess, ExecutorType::Multiprocess) => {
                // No conversion needed (both use shared memory)
                data
            },
            (ExecutorType::Native, ExecutorType::Native) => {
                // No conversion needed (same process)
                data
            },
            _ => {
                return Err(ServiceError::Internal("Unsupported executor combination".to_string()));
            }
        };

        // Record metrics
        let latency = start.elapsed();
        self.metrics.record_transfer(data.payload.len(), latency);

        Ok(converted_data)
    }

    /// Handle backpressure when target buffer is full
    pub async fn transfer_with_backpressure(
        &self,
        data: RuntimeData,
    ) -> Result<RuntimeData, ServiceError> {
        // Check buffer capacity
        loop {
            let buffer = self.buffer.read().await;
            if buffer.len() < buffer.capacity() {
                drop(buffer);
                break;
            }
            drop(buffer);

            // Buffer full, wait for space
            tokio::time::sleep(Duration::from_micros(100)).await;
        }

        // Transfer data
        let converted = self.transfer(data).await?;

        // Add to buffer
        self.buffer.write().await.push_back(converted.clone());

        Ok(converted)
    }

    /// Close bridge and flush remaining data
    pub async fn close(&mut self) -> Result<(), ServiceError> {
        // Drain buffer
        let mut buffer = self.buffer.write().await;
        buffer.clear();

        Ok(())
    }
}
```

---

## Metrics & Observability

### Bridge Metrics

```rust
pub struct BridgeMetrics {
    /// Total executions per node
    execution_count: Arc<RwLock<HashMap<String, u64>>>,

    /// Total execution time per node
    execution_time_us: Arc<RwLock<HashMap<String, u64>>>,

    /// Data transfers per bridge
    transfer_count: AtomicU64,

    /// Total bytes transferred
    bytes_transferred: AtomicU64,

    /// Conversion latency (microseconds)
    conversion_latency_us: AtomicU64,
}

impl BridgeMetrics {
    pub fn record_execution(&self, node_id: &str, duration: Duration) {
        let mut count = self.execution_count.blocking_write();
        *count.entry(node_id.to_string()).or_insert(0) += 1;

        let mut time = self.execution_time_us.blocking_write();
        *time.entry(node_id.to_string()).or_insert(0) += duration.as_micros() as u64;
    }

    pub fn record_transfer(&self, bytes: usize, latency: Duration) {
        self.transfer_count.fetch_add(1, Ordering::Relaxed);
        self.bytes_transferred.fetch_add(bytes as u64, Ordering::Relaxed);
        self.conversion_latency_us.fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
    }

    pub fn get_stats(&self) -> BridgeStats {
        BridgeStats {
            total_executions: self.execution_count.blocking_read().values().sum(),
            total_transfers: self.transfer_count.load(Ordering::Relaxed),
            total_bytes: self.bytes_transferred.load(Ordering::Relaxed),
            avg_conversion_latency_us: self.conversion_latency_us.load(Ordering::Relaxed) /
                self.transfer_count.load(Ordering::Relaxed).max(1),
        }
    }
}
```

---

## Error Handling

### Conversion Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: DataType,
        actual: DataType,
    },

    #[error("Shared memory allocation failed: {0}")]
    AllocationFailed(String),

    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("IPC error: {0}")]
    IPCError(String),
}
```

### Bridge Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("Executor not initialized: {0}")]
    ExecutorNotInitialized(ExecutorType),

    #[error("Data conversion failed: {0}")]
    ConversionFailed(#[from] ConversionError),

    #[error("Backpressure timeout after {0:?}")]
    BackpressureTimeout(Duration),

    #[error("Bridge closed")]
    BridgeClosed,
}
```

---

## Testing Contract

### Unit Test Requirements

1. **Data Conversion**: Test RuntimeData ↔ IPC message conversion
2. **Bridge Routing**: Verify correct bridge used for executor type
3. **Metrics Collection**: Validate metrics accuracy
4. **Error Handling**: Test conversion and bridge errors
5. **Backpressure**: Test buffer management under load

### Integration Test Requirements

1. **End-to-End Transfer**: Native → Multiprocess → Native data flow
2. **Zero-Copy Verification**: Verify shared memory usage for IPC
3. **Latency Measurement**: Measure conversion overhead (<2ms target)
4. **Concurrent Bridges**: Multiple bridges active simultaneously
5. **Cleanup Verification**: Resources released after bridge closure

### Test Fixtures

```rust
// Mock RuntimeData for testing
fn create_test_audio_data() -> RuntimeData {
    RuntimeData {
        data_type: DataType::Audio,
        payload: vec![0.0f32; 1024].into_iter()
            .flat_map(|f| f.to_le_bytes())
            .collect(),
        session_id: "test-session".to_string(),
        timestamp: SystemTime::now(),
    }
}

// Mock bridges for testing
async fn create_test_bridges() -> (Arc<NativeExecutorBridge>, Arc<MultiprocessExecutorBridge>) {
    let native = Arc::new(NativeExecutorBridge::new_test());
    let multiprocess = Arc::new(MultiprocessExecutorBridge::new_test());
    (native, multiprocess)
}
```
