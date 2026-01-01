//! Stream Health Analyzer
//!
//! Shared types and utilities for stream health analysis.
//! This crate provides:
//! - `HealthEvent` - Unified event types for stream health monitoring
//! - `EventSink` - Trait for event delivery (terminal, SSE, webhook)
//! - `EventEmitter` - JSONL output and event collection
//!
//! # Usage
//!
//! ```rust
//! use remotemedia_health_analyzer::{HealthEvent, EventEmitter, EventSink};
//!
//! // Create an event emitter for JSONL output
//! let mut emitter = EventEmitter::stdout();
//!
//! // Emit health events
//! emitter.emit(HealthEvent::health(0.95, vec![])).unwrap();
//! emitter.emit(HealthEvent::silence(3500.0, -60.0, None)).unwrap();
//! ```

mod events;
mod sink;
mod conversion;

pub use events::{HealthEvent, EventEmitter};
pub use sink::{EventSink, TerminalSink, ChannelSink};
pub use conversion::convert_json_to_health_events;
