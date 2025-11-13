//! IPC channel management using iceoryx2
//!
//! Provides zero-copy shared memory channels for inter-process communication
//! between Python nodes using the iceoryx2 publish-subscribe pattern.

use crate::{Error, Result};
#[cfg(feature = "multiprocess")]
use iceoryx2::prelude::*;
#[cfg(feature = "multiprocess")]
use iceoryx2::service::port_factory::publish_subscribe::PortFactory;
#[cfg(feature = "multiprocess")]
use iceoryx2_bb_log::set_log_level;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

use super::data_transfer::RuntimeData;

/// Global shared channel registry instance
/// All MultiprocessExecutor instances will use this single registry to ensure
/// they all share the same iceoryx2 Node and can communicate with each other
static GLOBAL_REGISTRY: OnceLock<Arc<ChannelRegistry>> = OnceLock::new();

/// Maximum payload size for IPC transfers (1 MB)
/// This is the maximum size of a single RuntimeData message that can be transferred
/// Matches the global iceoryx2 config: publish_subscribe.max_slice_len = 1048576
const MAX_SLICE_LEN: usize = 1 * 1024 * 1024;

/// Channel statistics
#[derive(Debug, Default, Clone)]
pub struct ChannelStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_transferred: u64,
    pub last_activity: Option<std::time::Instant>,
}

/// Handle to a shared memory channel
#[derive(Debug, Clone)]
pub struct ChannelHandle {
    /// Unique channel name
    pub name: String,

    /// Maximum buffer capacity
    pub capacity: usize,

    /// Channel statistics
    pub stats: Arc<RwLock<ChannelStats>>,

    /// Backpressure enabled flag
    pub backpressure_enabled: bool,
}

/// Channel registry for managing IPC channels
pub struct ChannelRegistry {
    #[cfg(feature = "multiprocess")]
    /// iceoryx2 node instance
    node: Option<Node<ipc::Service>>,

    /// Active channels
    channels: Arc<RwLock<HashMap<String, ChannelHandle>>>,

    #[cfg(feature = "multiprocess")]
    /// Active services (must be kept alive or they get dropped/deleted)
    services: Arc<RwLock<HashMap<String, PortFactory<ipc::Service, [u8], ()>>>>,
}

impl ChannelRegistry {
    /// Get or create the global shared channel registry
    /// This ensures all nodes/executors use the same iceoryx2 Node instance
    pub fn global() -> Arc<Self> {
        GLOBAL_REGISTRY
            .get_or_init(|| {
                #[cfg(feature = "multiprocess")]
                {
                    // Set log level first
                    set_log_level(LogLevel::Info);

                    // Create iceoryx2 node directly here, not via a mutable method
                    match NodeBuilder::new().create::<ipc::Service>() {
                        Ok(node) => {
                            tracing::info!("Global iceoryx2 node created successfully");
                            Arc::new(Self {
                                node: Some(node),
                                channels: Arc::new(RwLock::new(HashMap::new())),
                                services: Arc::new(RwLock::new(HashMap::new())),
                            })
                        }
                        Err(e) => {
                            tracing::error!("Failed to create global iceoryx2 node: {:?}. IPC operations will fail.", e);
                            // Return registry without node - operations will fail with clear error
                            Arc::new(Self {
                                node: None,
                                channels: Arc::new(RwLock::new(HashMap::new())),
                                services: Arc::new(RwLock::new(HashMap::new())),
                            })
                        }
                    }
                }

                #[cfg(not(feature = "multiprocess"))]
                {
                    Arc::new(Self::new_internal())
                }
            })
            .clone()
    }

    /// Create a new channel registry (internal use only)
    /// External code should use `global()` to get the shared instance
    fn new_internal() -> Self {
        Self {
            #[cfg(feature = "multiprocess")]
            node: None,
            channels: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "multiprocess")]
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new channel registry
    ///
    /// **DEPRECATED**: Use `ChannelRegistry::global()` instead to ensure all nodes
    /// share the same iceoryx2 Node instance for proper IPC communication.
    pub fn new() -> Self {
        Self::new_internal()
    }

    /// Initialize iceoryx2 runtime
    #[cfg(feature = "multiprocess")]
    pub fn initialize(&mut self) -> Result<()> {
        // Skip if already initialized
        if self.node.is_some() {
            tracing::debug!("iceoryx2 node already initialized");
            return Ok(());
        }

        // Set log level
        set_log_level(LogLevel::Info);

        // Create iceoryx2 node
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|e| Error::IpcError(format!("Failed to create iceoryx2 node: {:?}", e)))?;

        self.node = Some(node);

        tracing::info!("iceoryx2 runtime initialized");
        Ok(())
    }

    /// Ensure the registry is initialized (for use in spawned threads)
    #[cfg(feature = "multiprocess")]
    pub async fn ensure_initialized(&self) -> Result<()> {
        if self.node.is_none() {
            return Err(Error::IpcError(
                "iceoryx2 node not initialized. Call ChannelRegistry::global() first on main thread.".to_string()
            ));
        }
        Ok(())
    }

    /// Create a new shared memory channel
    #[cfg(feature = "multiprocess")]
    pub async fn create_channel(
        &self,
        name: &str,
        capacity: usize,
        backpressure: bool,
    ) -> Result<ChannelHandle> {
        let node = self
            .node
            .as_ref()
            .ok_or_else(|| Error::IpcError("Node not initialized".to_string()))?;

        // Create service name from channel name
        let service_name = ServiceName::new(name)
            .map_err(|e| Error::IpcError(format!("Invalid service name: {:?}", e)))?;

        // Create publish-subscribe service for byte slices
        let service = node
            .service_builder(&service_name)
            .publish_subscribe::<[u8]>()
            .max_publishers(10)
            .max_subscribers(10)
            .history_size(capacity)
            .subscriber_max_buffer_size(capacity)
            .open_or_create()
            .map_err(|e| Error::IpcError(format!("Failed to create service: {:?}", e)))?;

        // Store the service to keep it alive
        let mut services = self.services.write().await;
        services.insert(name.to_string(), service);

        // Create channel handle
        let handle = ChannelHandle {
            name: name.to_string(),
            capacity,
            stats: Arc::new(RwLock::new(ChannelStats::default())),
            backpressure_enabled: backpressure,
        };

        let mut channels = self.channels.write().await;
        channels.insert(name.to_string(), handle.clone());

        tracing::info!(
            "Created iceoryx2 channel: {} (capacity: {})",
            name,
            capacity
        );
        Ok(handle)
    }

    /// Destroy a channel and cleanup resources
    #[cfg(feature = "multiprocess")]
    pub async fn destroy_channel(&self, channel: ChannelHandle) -> Result<()> {
        let mut channels = self.channels.write().await;
        channels.remove(&channel.name);

        tracing::info!("Destroyed channel: {}", channel.name);
        Ok(())
    }

    /// Create a publisher for a channel
    #[cfg(feature = "multiprocess")]
    pub async fn create_publisher(&self, channel_name: &str) -> Result<Publisher> {
        let services = self.services.read().await;
        let service = services
            .get(channel_name)
            .ok_or_else(|| Error::IpcError(format!("Channel not found: {}", channel_name)))?;

        // Create publisher from service with dynamic allocation support
        // Start with 512KB to handle typical audio buffers without reallocation
        let publisher = service
            .publisher_builder()
            .initial_max_slice_len(512 * 1024) // 512KB - large enough for most audio chunks
            .allocation_strategy(AllocationStrategy::PowerOfTwo)
            .create()
            .map_err(|e| Error::IpcError(format!("Failed to create publisher: {:?}", e)))?;

        let channels = self.channels.read().await;
        let handle = channels.get(channel_name).ok_or_else(|| {
            Error::IpcError(format!("Channel handle not found: {}", channel_name))
        })?;

        Ok(Publisher {
            channel_name: channel_name.to_string(),
            inner: publisher,
            stats: handle.stats.clone(),
            backpressure: handle.backpressure_enabled,
            _lifetime: std::marker::PhantomData,
        })
    }

    /// Create a subscriber for a channel
    #[cfg(feature = "multiprocess")]
    pub async fn create_subscriber(&self, channel_name: &str) -> Result<Subscriber> {
        let services = self.services.read().await;
        let service = services
            .get(channel_name)
            .ok_or_else(|| Error::IpcError(format!("Channel not found: {}", channel_name)))?;

        let channels = self.channels.read().await;
        let handle = channels.get(channel_name).ok_or_else(|| {
            Error::IpcError(format!("Channel handle not found: {}", channel_name))
        })?;

        // Create subscriber with buffer size to receive historical messages
        let subscriber = service
            .subscriber_builder()
            .buffer_size(handle.capacity) // Request history
            .create()
            .map_err(|e| Error::IpcError(format!("Failed to create subscriber: {:?}", e)))?;

        tracing::info!(
            "Created subscriber for '{}' with buffer_size={} to receive historical messages",
            channel_name,
            handle.capacity
        );

        Ok(Subscriber {
            channel_name: channel_name.to_string(),
            inner: subscriber,
            stats: handle.stats.clone(),
        })
    }

    /// Create a raw subscriber for control channels (bypasses RuntimeData deserialization)
    /// Used for simple control messages like READY signals
    #[cfg(feature = "multiprocess")]
    pub async fn create_raw_subscriber(
        &self,
        channel_name: &str,
    ) -> Result<iceoryx2::port::subscriber::Subscriber<ipc::Service, [u8], ()>> {
        let services = self.services.read().await;
        let service = services
            .get(channel_name)
            .ok_or_else(|| Error::IpcError(format!("Channel not found: {}", channel_name)))?;

        let channels = self.channels.read().await;
        let handle = channels.get(channel_name).ok_or_else(|| {
            Error::IpcError(format!("Channel handle not found: {}", channel_name))
        })?;

        // Create subscriber with buffer size to receive historical messages
        let subscriber = service
            .subscriber_builder()
            .buffer_size(handle.capacity) // Request history
            .create()
            .map_err(|e| Error::IpcError(format!("Failed to create raw subscriber: {:?}", e)))?;

        tracing::info!(
            "Created raw subscriber for '{}' with buffer_size={} to receive historical messages",
            channel_name,
            handle.capacity
        );

        Ok(subscriber)
    }
}

/// Publisher for sending data to a channel
#[cfg(feature = "multiprocess")]
pub struct Publisher<'a> {
    channel_name: String,
    inner: iceoryx2::port::publisher::Publisher<ipc::Service, [u8], ()>,
    stats: Arc<RwLock<ChannelStats>>,
    backpressure: bool,
    _lifetime: std::marker::PhantomData<&'a ()>,
}

#[cfg(feature = "multiprocess")]
impl<'a> Publisher<'a> {
    /// Publish data to the channel (synchronous - iceoryx2 is lock-free)
    pub fn publish(&self, data: RuntimeData) -> Result<()> {
        // Serialize RuntimeData to bytes
        let bytes = data.to_bytes();

        tracing::info!(
            "[IPC Publisher] Channel '{}' publishing {} bytes (type: {:?})",
            self.channel_name,
            bytes.len(),
            data.data_type
        );

        if bytes.len() > MAX_SLICE_LEN {
            return Err(Error::IpcError(format!(
                "Message too large: {} bytes (max: {})",
                bytes.len(),
                MAX_SLICE_LEN
            )));
        }

        // Loan uninitialized memory
        let sample = self
            .inner
            .loan_slice_uninit(bytes.len())
            .map_err(|e| Error::IpcError(format!("Failed to loan memory: {:?}", e)))?;

        // Write payload and send
        let sample = sample.write_from_slice(&bytes);
        sample
            .send()
            .map_err(|e| Error::IpcError(format!("Failed to send sample: {:?}", e)))?;

        tracing::info!(
            "[IPC Publisher] Channel '{}' successfully sent {} bytes",
            self.channel_name,
            bytes.len()
        );

        // Update stats
        let mut stats = self.stats.blocking_write();
        stats.messages_sent += 1;
        stats.bytes_transferred += bytes.len() as u64;
        stats.last_activity = Some(std::time::Instant::now());

        Ok(())
    }

    /// Try to publish without blocking (returns false if would block)
    pub fn try_publish(&self, data: RuntimeData) -> Result<bool> {
        // For now, just call publish - backpressure handling can be added later
        self.publish(data)?;
        Ok(true)
    }

    /// Send raw bytes directly (like READY signal) - bypasses RuntimeData serialization
    pub fn send(&self, bytes: &[u8]) -> Result<()> {
        tracing::info!(
            "[IPC Publisher] Channel '{}' sending raw {} bytes",
            self.channel_name,
            bytes.len()
        );

        if bytes.len() > MAX_SLICE_LEN {
            return Err(Error::IpcError(format!(
                "Message too large: {} bytes (max: {})",
                bytes.len(),
                MAX_SLICE_LEN
            )));
        }

        // Loan uninitialized memory
        let sample = self
            .inner
            .loan_slice_uninit(bytes.len())
            .map_err(|e| Error::IpcError(format!("Failed to loan memory: {:?}", e)))?;

        // Write payload and send
        let sample = sample.write_from_slice(bytes);
        sample
            .send()
            .map_err(|e| Error::IpcError(format!("Failed to send sample: {:?}", e)))?;

        tracing::info!(
            "[IPC Publisher] Channel '{}' successfully sent raw {} bytes",
            self.channel_name,
            bytes.len()
        );

        // Update stats
        let mut stats = self.stats.blocking_write();
        stats.messages_sent += 1;
        stats.bytes_transferred += bytes.len() as u64;
        stats.last_activity = Some(std::time::Instant::now());

        Ok(())
    }
}

/// Subscriber for receiving data from a channel
#[cfg(feature = "multiprocess")]
pub struct Subscriber {
    channel_name: String,
    inner: iceoryx2::port::subscriber::Subscriber<ipc::Service, [u8], ()>,
    stats: Arc<RwLock<ChannelStats>>,
}

#[cfg(feature = "multiprocess")]
impl Subscriber {
    /// Receive data from the channel (synchronous - iceoryx2 is lock-free)
    pub fn receive(&self) -> Result<Option<RuntimeData>> {
        // Try to receive a sample
        match self.inner.receive() {
            Ok(Some(sample)) => {
                let bytes = sample.payload();

                tracing::info!(
                    "[IPC Subscriber] Channel '{}' received {} bytes",
                    self.channel_name,
                    bytes.len()
                );

                // Deserialize RuntimeData from bytes
                let data = RuntimeData::from_bytes(bytes)
                    .map_err(|e| Error::IpcError(format!("Failed to deserialize: {}", e)))?;

                // Update stats
                let mut stats = self.stats.blocking_write();
                stats.messages_received += 1;
                stats.bytes_transferred += bytes.len() as u64;
                stats.last_activity = Some(std::time::Instant::now());

                Ok(Some(data))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Error::IpcError(format!("Failed to receive: {:?}", e))),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "multiprocess")]
    async fn test_channel_creation() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        // Use unique name to avoid conflicts with other tests
        let channel_name = format!("test/channel/create/{}", std::process::id());
        let channel = registry
            .create_channel(&channel_name, 100, true)
            .await
            .unwrap();

        assert_eq!(channel.name, channel_name);
        assert_eq!(channel.capacity, 100);
        assert!(channel.backpressure_enabled);

        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    #[cfg(feature = "multiprocess")]
    async fn test_publish_subscribe() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        // Use unique name with timestamp to avoid conflicts
        let channel_name = format!(
            "test/channel/pubsub/{}/{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let channel = registry
            .create_channel(&channel_name, 10, false)
            .await
            .unwrap();

        // Create publisher and subscriber
        let publisher = registry.create_publisher(&channel_name).await.unwrap();
        let subscriber = registry.create_subscriber(&channel_name).await.unwrap();

        // Publish data
        let data = RuntimeData::text("Hello, IPC!", "test_session");
        publisher.publish(data).unwrap();

        // Small delay to ensure message is received
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Receive data
        let received = subscriber.receive().unwrap();
        assert!(received.is_some());

        let received_data = received.unwrap();
        assert_eq!(
            String::from_utf8_lossy(&received_data.payload),
            "Hello, IPC!"
        );

        registry.destroy_channel(channel).await.unwrap();
    }
}
