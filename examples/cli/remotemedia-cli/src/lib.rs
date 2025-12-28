//! RemoteMedia CLI library
//!
//! This library exposes the core utilities used by the RemoteMedia CLI,
//! enabling specialized executables that embed specific pipelines.
//!
//! # Example
//!
//! ```no_run
//! use remotemedia_cli::{audio, io, pipeline};
//! use remotemedia_runtime_core::data::RuntimeData;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Parse embedded pipeline YAML
//! let manifest = pipeline::parse_manifest(PIPELINE_YAML)?;
//!
//! // Create runner and execute
//! let runner = pipeline::create_runner()?;
//! let output = pipeline::execute_unary(&runner, std::sync::Arc::new(manifest), input).await?;
//! # Ok(())
//! # }
//! # const PIPELINE_YAML: &str = "";
//! # let input = RuntimeData::Text(String::new());
//! ```

pub mod audio;
pub mod io;
pub mod pipeline;
pub mod output;

// Re-export commonly used types
pub use audio::{is_wav, parse_wav};
pub use io::{detect_input_source, detect_output_sink, InputReader, InputSource, OutputSink, OutputWriter};
pub use output::{OutputFormat, Outputter};
pub use pipeline::{create_runner, execute_unary, parse_manifest, StreamingSession};
