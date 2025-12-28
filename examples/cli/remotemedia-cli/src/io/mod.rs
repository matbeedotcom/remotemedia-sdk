//! I/O abstraction for named pipes, files, and stdin/stdout
//!
//! This module provides unified I/O handling for the CLI, supporting:
//! - Regular files
//! - Named pipes (FIFOs) on Unix systems
//! - Standard input/output (`-` shorthand)
//!
//! # Examples
//!
//! ```no_run
//! use remotemedia_cli::io::{InputSource, OutputSink, detect_input_source, detect_output_sink};
//!
//! // Detect input type
//! let source = detect_input_source("-").unwrap(); // Returns InputSource::Stdin
//! let source = detect_input_source("/tmp/fifo").unwrap(); // Returns InputSource::Pipe if FIFO
//!
//! // Detect output type
//! let sink = detect_output_sink("-").unwrap(); // Returns OutputSink::Stdout
//! ```

mod detection;
mod reader;
mod writer;

pub use detection::{detect_input_source, detect_output_sink, is_fifo};
pub use reader::InputReader;
pub use writer::OutputWriter;

use std::path::PathBuf;
use thiserror::Error;

/// Represents an input source for pipeline data
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    /// Regular file on disk
    File(PathBuf),
    /// Named pipe (FIFO) on Unix
    Pipe(PathBuf),
    /// Standard input (stdin)
    Stdin,
}

impl InputSource {
    /// Returns the path if this is a file or pipe source
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            InputSource::File(p) | InputSource::Pipe(p) => Some(p),
            InputSource::Stdin => None,
        }
    }

    /// Returns true if this source is stdin
    pub fn is_stdin(&self) -> bool {
        matches!(self, InputSource::Stdin)
    }

    /// Returns true if this source is a named pipe
    pub fn is_pipe(&self) -> bool {
        matches!(self, InputSource::Pipe(_))
    }
}

/// Represents an output sink for pipeline data
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputSink {
    /// Regular file on disk
    File(PathBuf),
    /// Named pipe (FIFO) on Unix
    Pipe(PathBuf),
    /// Standard output (stdout)
    Stdout,
}

impl OutputSink {
    /// Returns the path if this is a file or pipe sink
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            OutputSink::File(p) | OutputSink::Pipe(p) => Some(p),
            OutputSink::Stdout => None,
        }
    }

    /// Returns true if this sink is stdout
    pub fn is_stdout(&self) -> bool {
        matches!(self, OutputSink::Stdout)
    }

    /// Returns true if this sink is a named pipe
    pub fn is_pipe(&self) -> bool {
        matches!(self, OutputSink::Pipe(_))
    }
}

/// Errors that can occur during I/O operations
#[derive(Debug, Error)]
pub enum IoError {
    /// The specified path does not exist
    #[error("path not found: {path}")]
    PathNotFound { path: PathBuf },

    /// The path exists but is not accessible (permission denied)
    #[error("permission denied: {path}")]
    PermissionDenied { path: PathBuf },

    /// The path is not a valid file or pipe
    #[error("not a file or pipe: {path}")]
    InvalidPathType { path: PathBuf },

    /// Broken pipe - the reader/writer on the other end closed
    #[error("broken pipe: {message}")]
    BrokenPipe { message: String },

    /// End of stream reached
    #[error("end of stream")]
    EndOfStream,

    /// Generic I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The operation is not supported on this platform
    #[error("operation not supported on this platform: {operation}")]
    UnsupportedPlatform { operation: String },
}

impl IoError {
    /// Create a PathNotFound error
    pub fn path_not_found(path: impl Into<PathBuf>) -> Self {
        IoError::PathNotFound { path: path.into() }
    }

    /// Create a PermissionDenied error
    pub fn permission_denied(path: impl Into<PathBuf>) -> Self {
        IoError::PermissionDenied { path: path.into() }
    }

    /// Create a BrokenPipe error
    pub fn broken_pipe(message: impl Into<String>) -> Self {
        IoError::BrokenPipe {
            message: message.into(),
        }
    }
}

/// Result type for I/O operations
pub type IoResult<T> = Result<T, IoError>;
