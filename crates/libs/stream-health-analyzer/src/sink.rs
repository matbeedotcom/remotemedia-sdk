//! Event sink trait and implementations
//!
//! This module defines the `EventSink` trait for event delivery
//! and provides implementations for different output targets.

use crate::HealthEvent;
use std::io::Write;
use tokio::sync::broadcast;

/// Trait for event delivery targets
///
/// Implementations include:
/// - `TerminalSink` - JSONL output to terminal/file
/// - `ChannelSink` - Broadcast channel for SSE/WebSocket
pub trait EventSink: Send + Sync {
    /// Emit an event to this sink
    fn emit(&self, event: HealthEvent) -> Result<(), EventSinkError>;

    /// Close the sink and perform any cleanup
    fn close(&self) -> Result<(), EventSinkError> {
        Ok(())
    }
}

/// Error type for event sink operations
#[derive(Debug, thiserror::Error)]
pub enum EventSinkError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Channel send error: {0}")]
    Channel(String),
}

/// Terminal/file JSONL sink
///
/// Writes events as newline-delimited JSON to a writer.
pub struct TerminalSink<W: Write + Send + Sync> {
    writer: std::sync::Mutex<W>,
}

impl<W: Write + Send + Sync> TerminalSink<W> {
    /// Create a new terminal sink writing to the specified output
    pub fn new(writer: W) -> Self {
        Self {
            writer: std::sync::Mutex::new(writer),
        }
    }
}

impl TerminalSink<std::io::Stdout> {
    /// Create a terminal sink writing to stdout
    pub fn stdout() -> Self {
        Self::new(std::io::stdout())
    }
}

impl<W: Write + Send + Sync> EventSink for TerminalSink<W> {
    fn emit(&self, event: HealthEvent) -> Result<(), EventSinkError> {
        let line = serde_json::to_string(&event)
            .map_err(|e| EventSinkError::Serialization(e.to_string()))?;
        let mut writer = self.writer.lock().unwrap();
        writeln!(writer, "{}", line)?;
        writer.flush()?;
        Ok(())
    }
}

/// Broadcast channel sink for SSE/WebSocket delivery
///
/// Sends events to a tokio broadcast channel that can have multiple subscribers.
pub struct ChannelSink {
    sender: broadcast::Sender<HealthEvent>,
}

impl ChannelSink {
    /// Create a new channel sink with the specified capacity
    pub fn new(capacity: usize) -> (Self, broadcast::Receiver<HealthEvent>) {
        let (sender, receiver) = broadcast::channel(capacity);
        (Self { sender }, receiver)
    }

    /// Create a new channel sink with default capacity (256)
    pub fn with_default_capacity() -> (Self, broadcast::Receiver<HealthEvent>) {
        Self::new(256)
    }

    /// Subscribe to receive events from this sink
    pub fn subscribe(&self) -> broadcast::Receiver<HealthEvent> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl EventSink for ChannelSink {
    fn emit(&self, event: HealthEvent) -> Result<(), EventSinkError> {
        self.sender
            .send(event)
            .map_err(|e| EventSinkError::Channel(e.to_string()))?;
        Ok(())
    }
}

/// Multi-sink that broadcasts events to multiple sinks
pub struct MultiSink {
    sinks: Vec<Box<dyn EventSink>>,
}

impl MultiSink {
    /// Create a new multi-sink
    pub fn new() -> Self {
        Self { sinks: Vec::new() }
    }

    /// Add a sink to the multi-sink
    pub fn add_sink(&mut self, sink: Box<dyn EventSink>) {
        self.sinks.push(sink);
    }
}

impl Default for MultiSink {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSink for MultiSink {
    fn emit(&self, event: HealthEvent) -> Result<(), EventSinkError> {
        for sink in &self.sinks {
            sink.emit(event.clone())?;
        }
        Ok(())
    }

    fn close(&self) -> Result<(), EventSinkError> {
        for sink in &self.sinks {
            sink.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_terminal_sink() {
        struct SharedBuffer(Arc<Mutex<Cursor<Vec<u8>>>>);

        impl Write for SharedBuffer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.0.lock().unwrap().flush()
            }
        }

        // Add Send + Sync implementation
        unsafe impl Send for SharedBuffer {}
        unsafe impl Sync for SharedBuffer {}

        let buffer = Arc::new(Mutex::new(Cursor::new(Vec::new())));
        let sink = TerminalSink::new(SharedBuffer(buffer.clone()));

        sink.emit(HealthEvent::health(0.95, vec![])).unwrap();
        sink.emit(HealthEvent::silence(3000.0, -60.0, None)).unwrap();

        let inner = buffer.lock().unwrap();
        let output = String::from_utf8(inner.get_ref().clone()).unwrap();
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"type\":\"health\""));
        assert!(lines[1].contains("\"type\":\"silence\""));
    }

    #[tokio::test]
    async fn test_channel_sink() {
        let (sink, mut receiver) = ChannelSink::with_default_capacity();

        sink.emit(HealthEvent::health(0.95, vec![])).unwrap();
        sink.emit(HealthEvent::silence(3000.0, -60.0, None)).unwrap();

        let event1 = receiver.recv().await.unwrap();
        let event2 = receiver.recv().await.unwrap();

        assert!(event1.is_health());
        assert!(event2.is_silence());
    }

    #[tokio::test]
    async fn test_channel_sink_multiple_subscribers() {
        let (sink, mut receiver1) = ChannelSink::with_default_capacity();
        let mut receiver2 = sink.subscribe();

        assert_eq!(sink.receiver_count(), 2);

        sink.emit(HealthEvent::health(0.95, vec![])).unwrap();

        let event1 = receiver1.recv().await.unwrap();
        let event2 = receiver2.recv().await.unwrap();

        assert!(event1.is_health());
        assert!(event2.is_health());
    }
}
