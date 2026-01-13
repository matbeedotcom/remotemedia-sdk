//! Shared Pipeline Execution for RemoteMedia SDK
//!
//! This crate provides reusable pipeline execution infrastructure used by both
//! the `remotemedia-ingest-srt` service and the `stream-health-demo` CLI.
//!
//! # Key Components
//!
//! - [`PipelineSession`] - Wraps `PipelineExecutor` for streaming analysis
//! - [`PipelineRegistry`] - Loads and manages pipeline YAML templates
//! - [`convert_output_to_health_event`] - Converts `RuntimeData` to `HealthEvent`
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_pipeline_runner::{PipelineSession, PipelineRegistry};
//! use remotemedia_runtime_core::data::RuntimeData;
//!
//! // Load pipeline template
//! let registry = PipelineRegistry::embedded();
//! let manifest = registry.get("demo_audio_quality_v1")?;
//!
//! // Create session and process data
//! let mut session = PipelineSession::new(manifest).await?;
//! session.send(RuntimeData::Audio { ... }).await?;
//!
//! while let Some(output) = session.try_recv()? {
//!     if let Some(event) = convert_output_to_health_event(&output) {
//!         println!("Event: {:?}", event);
//!     }
//! }
//! ```

mod conversion;
mod registry;
mod session;

pub use conversion::{convert_output_to_health_event, convert_output_to_health_events};
pub use registry::{PipelineRegistry, PipelineTemplate};
pub use session::{PipelineSession, PipelineSessionError};
