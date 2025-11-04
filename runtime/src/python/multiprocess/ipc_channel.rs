//! IPC channel management using iceoryx2
//!
//! Provides zero-copy shared memory channels for inter-process communication
//! between Python nodes using the iceoryx2 publish-subscribe pattern.

#[cfg(feature = "multiprocess")]
use iceoryx2::prelude::*;
#[cfg(feature = "multiprocess")]
use iceoryx2_bb_log::set_log_level;
use crate::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::data_transfer::RuntimeData;

/// Maximum payload size for IPC transfers (10 MB)
/// This is the maximum size of a single RuntimeData message that can be transferred
const MAX_SLICE_LEN: usize = 10 * 1024 * 1024;

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
}

impl ChannelRegistry {
    /// Create a new channel registry
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "multiprocess")]
            node: None,
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize iceoryx2 runtime
    #[cfg(feature = "multiprocess")]
    pub fn initialize(&mut self) -> Result<()> {
        // Set log level
        set_log_level(LogLevel::Info);

        // Create custom config with increased limits for AI model data transfer
        let mut config = Config::global_config().clone();

        // Configure publish-subscribe defaults
        // Note: max_slice_len must be configured per-publisher using initial_max_slice_len() in v0.7.0
        config.defaults.publish_subscribe.subscriber_max_buffer_size = 100;
        config.defaults.publish_subscribe.subscriber_max_borrowed_samples = 100;
        config.defaults.publish_subscribe.publisher_max_loaned_samples = 16;

        // Create iceoryx2 node with custom config
        let node = NodeBuilder::new()
            .config(&config)
            .create::<ipc::Service>()
            .map_err(|e| Error::Execution(format!("Failed to create iceoryx2 node: {:?}", e)))?;

        self.node = Some(node);

        tracing::info!("iceoryx2 runtime initialized");
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
        let node = self.node.as_ref()
            .ok_or_else(|| Error::Execution("iceoryx2 not initialized".to_string()))?;

        // Create service name
        let service_name = name.try_into()
            .map_err(|e| Error::Execution(format!("Invalid service name: {:?}", e)))?;

        // Create publish-subscribe service for RuntimeData
        let _service = node
            .service_builder(&service_name)
            .publish_subscribe::<[u8]>()  // Using byte array for serialized RuntimeData
            .history_size(0)  // No history needed for real-time streaming
            .subscriber_max_buffer_size(capacity)
            .subscriber_max_borrowed_samples(capacity)
            .open_or_create()
            .map_err(|e| Error::Execution(format!("Failed to create service: {:?}", e)))?;

        // Create channel handle
        let handle = ChannelHandle {
            name: name.to_string(),
            capacity,
            stats: Arc::new(RwLock::new(ChannelStats::default())),
            backpressure_enabled: backpressure,
        };

        // Store handle
        let mut channels = self.channels.write().await;
        channels.insert(name.to_string(), handle.clone());

        tracing::info!("Created channel: {} with capacity: {}", name, capacity);
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
        let node = self.node.as_ref()
            .ok_or_else(|| Error::Execution("iceoryx2 not initialized".to_string()))?;

        let channels = self.channels.read().await;
        let channel = channels.get(channel_name)
            .ok_or_else(|| Error::Execution(format!("Channel {} not found", channel_name)))?;

        // Create service name
        let service_name = channel_name.try_into()
            .map_err(|e| Error::Execution(format!("Invalid service name: {:?}", e)))?;

        // Open the service
        let service = node
            .service_builder(&service_name)
            .publish_subscribe::<[u8]>()
            .history_size(0)
            .subscriber_max_buffer_size(channel.capacity)
            .subscriber_max_borrowed_samples(channel.capacity)
            .open_or_create()
            .map_err(|e| Error::Execution(format!("Failed to open service: {:?}", e)))?;

        // Configure publisher to allow multiple samples and large payloads
        let iox_publisher = service
            .publisher_builder()
            .max_loaned_samples(16)  // Allow up to 16 samples to be loaned simultaneously
            .initial_max_slice_len(MAX_SLICE_LEN)  // Set maximum payload size for slices
            .create()
            .map_err(|e| Error::Execution(format!("Failed to create publisher: {:?}", e)))?;

        Ok(Publisher {
            channel_name: channel_name.to_string(),
            inner: iox_publisher,
            stats: channel.stats.clone(),
            backpressure: channel.backpressure_enabled,
            _lifetime: std::marker::PhantomData,
        })
    }

    /// Create a subscriber for a channel
    #[cfg(feature = "multiprocess")]
    pub async fn create_subscriber(&self, channel_name: &str) -> Result<Subscriber> {
        let node = self.node.as_ref()
            .ok_or_else(|| Error::Execution("iceoryx2 not initialized".to_string()))?;

        let channels = self.channels.read().await;
        let channel = channels.get(channel_name)
            .ok_or_else(|| Error::Execution(format!("Channel {} not found", channel_name)))?;

        // Create service name
        let service_name = channel_name.try_into()
            .map_err(|e| Error::Execution(format!("Invalid service name: {:?}", e)))?;

        // Open the service
        let service = node
            .service_builder(&service_name)
            .publish_subscribe::<[u8]>()
            .history_size(0)
            .subscriber_max_buffer_size(channel.capacity)
            .subscriber_max_borrowed_samples(channel.capacity)
            .open_or_create()
            .map_err(|e| Error::Execution(format!("Failed to open service: {:?}", e)))?;

        let iox_subscriber = service
            .subscriber_builder()
            .create()
            .map_err(|e| Error::Execution(format!("Failed to create subscriber: {:?}", e)))?;

        Ok(Subscriber {
            channel_name: channel_name.to_string(),
            inner: iox_subscriber,
            stats: channel.stats.clone(),
        })
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
    /// Publish data to the channel
    pub async fn publish(&self, data: RuntimeData) -> Result<()> {
        let bytes = data.to_bytes();
        let size = bytes.len();

        tracing::info!("Attempting to loan {} bytes for channel: {}", size, self.channel_name);

        // Get a loan for zero-copy transfer
        let mut sample = self.inner
            .loan_slice_uninit(size)
            .map_err(|e| Error::Execution(format!("Failed to loan {} bytes: {:?}", size, e)))?;

        // Write data to the sample (initialize the MaybeUninit slice)
        let payload = sample.payload_mut();
        for (i, &byte) in bytes.iter().enumerate() {
            payload[i].write(byte);
        }

        // Assume initialized and send
        let sample = unsafe { sample.assume_init() };
        sample.send()
            .map_err(|e| Error::Execution(format!("Failed to send sample: {:?}", e)))?;

        // Update stats
        let mut stats = self.stats.write().await;
        stats.messages_sent += 1;
        stats.bytes_transferred += size as u64;
        stats.last_activity = Some(std::time::Instant::now());

        tracing::trace!("Published {} bytes to channel: {}", size, self.channel_name);
        Ok(())
    }

    /// Try to publish without blocking (returns false if would block)
    pub async fn try_publish(&self, data: RuntimeData) -> Result<bool> {
        // For now, just call publish - backpressure handling can be added later
        self.publish(data).await?;
        Ok(true)
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
    /// Receive data from the channel
    pub async fn receive(&self) -> Result<Option<RuntimeData>> {
        // Try to receive a sample
        let sample = self.inner
            .receive()
            .map_err(|e| Error::Execution(format!("Failed to receive sample: {:?}", e)))?;

        if let Some(sample) = sample {
            // Get the bytes and deserialize
            let bytes = sample.payload();
            let data = RuntimeData::from_bytes(bytes)
                .map_err(|e| Error::Execution(format!("Failed to deserialize data: {}", e)))?;

            // Update stats
            let mut stats = self.stats.write().await;
            stats.messages_received += 1;
            stats.bytes_transferred += bytes.len() as u64;
            stats.last_activity = Some(std::time::Instant::now());

            tracing::trace!("Received {} bytes from channel: {}", bytes.len(), self.channel_name);
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }
}

// Placeholder implementations for when multiprocess feature is disabled
#[cfg(not(feature = "multiprocess"))]
impl ChannelRegistry {
    pub fn initialize(&mut self) -> Result<()> {
        Err(Error::Execution("Multiprocess support not enabled".to_string()))
    }

    pub fn create_channel(
        &self,
        _name: &str,
        _capacity: usize,
        _backpressure: bool,
    ) -> Result<ChannelHandle> {
        Err(Error::Execution("Multiprocess support not enabled".to_string()))
    }

    pub fn destroy_channel(&self, _channel: ChannelHandle) -> Result<()> {
        Err(Error::Execution("Multiprocess support not enabled".to_string()))
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

        let channel = registry.create_channel(
            "test_channel",
            100,
            true,
        ).await.unwrap();

        assert_eq!(channel.name, "test_channel");
        assert_eq!(channel.capacity, 100);
        assert!(channel.backpressure_enabled);

        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    #[cfg(feature = "multiprocess")]
    async fn test_publish_subscribe() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry.create_channel("test_channel", 10, false).await.unwrap();

        // Create publisher and subscriber
        let publisher = registry.create_publisher("test_channel").await.unwrap();
        let subscriber = registry.create_subscriber("test_channel").await.unwrap();

        // Publish data
        let data = RuntimeData::text("Hello, IPC!", "test_session");
        publisher.publish(data).await.unwrap();

        // Receive data
        let received = subscriber.receive().await.unwrap();
        assert!(received.is_some());

        let received_data = received.unwrap();
        assert_eq!(
            String::from_utf8_lossy(&received_data.payload),
            "Hello, IPC!"
        );

        registry.destroy_channel(channel).await.unwrap();
    }
}