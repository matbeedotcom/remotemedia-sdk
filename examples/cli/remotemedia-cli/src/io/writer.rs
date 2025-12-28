//! Unified async writer for files, pipes, and stdout

use std::pin::Pin;

use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

use super::{IoError, IoResult, OutputSink};

/// Unified async writer that handles files, named pipes, and stdout
///
/// This struct wraps different output sinks behind a common async write interface,
/// allowing the same code to write to regular files, FIFOs, or standard output.
pub struct OutputWriter {
    inner: BufWriter<Pin<Box<dyn AsyncWrite + Send + Unpin>>>,
    sink: OutputSink,
}

impl OutputWriter {
    /// Open an output sink for writing
    ///
    /// # Arguments
    /// * `sink` - The output sink to open
    ///
    /// # Returns
    /// An `OutputWriter` ready to write to the sink
    ///
    /// # Errors
    /// * `IoError::Io` if the file/pipe cannot be opened
    /// * `IoError::PermissionDenied` if the path isn't writable
    ///
    /// # Notes
    /// For named pipes, this will block until a reader connects (standard Unix behavior).
    /// For files, this will create the file if it doesn't exist, or truncate if it does.
    pub async fn open(sink: OutputSink) -> IoResult<Self> {
        let writer: Pin<Box<dyn AsyncWrite + Send + Unpin>> = match &sink {
            OutputSink::File(path) => {
                // Create parent directories if needed
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() && !parent.exists() {
                        tokio::fs::create_dir_all(parent).await.map_err(|e| {
                            if e.kind() == std::io::ErrorKind::PermissionDenied {
                                IoError::permission_denied(parent)
                            } else {
                                IoError::Io(e)
                            }
                        })?;
                    }
                }

                let file = tokio::fs::File::create(path).await.map_err(|e| {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        IoError::permission_denied(path)
                    } else {
                        IoError::Io(e)
                    }
                })?;
                Box::pin(file)
            }
            OutputSink::Pipe(path) => {
                // Named pipes are opened for writing using OpenOptions
                // The open() call will block until a reader connects
                let file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(path)
                    .await
                    .map_err(|e| {
                        if e.kind() == std::io::ErrorKind::NotFound {
                            IoError::path_not_found(path)
                        } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                            IoError::permission_denied(path)
                        } else {
                            IoError::Io(e)
                        }
                    })?;
                Box::pin(file)
            }
            OutputSink::Stdout => Box::pin(tokio::io::stdout()),
        };

        Ok(Self {
            inner: BufWriter::new(writer),
            sink,
        })
    }

    /// Get a reference to the output sink
    pub fn sink(&self) -> &OutputSink {
        &self.sink
    }

    /// Write data to the sink
    ///
    /// # Arguments
    /// * `buf` - Data to write
    ///
    /// # Returns
    /// Number of bytes written
    ///
    /// # Errors
    /// * `IoError::BrokenPipe` if the reader closed the pipe
    pub async fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.inner.write(buf).await.map_err(Self::map_write_error)
    }

    /// Write all data to the sink
    ///
    /// # Arguments
    /// * `buf` - Data to write
    ///
    /// # Errors
    /// * `IoError::BrokenPipe` if the reader closed the pipe
    pub async fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        self.inner
            .write_all(buf)
            .await
            .map_err(Self::map_write_error)
    }

    /// Flush buffered data to the sink
    ///
    /// # Errors
    /// * `IoError::BrokenPipe` if the reader closed the pipe
    pub async fn flush(&mut self) -> IoResult<()> {
        self.inner.flush().await.map_err(Self::map_write_error)
    }

    /// Shutdown the writer, flushing any remaining data
    ///
    /// # Errors
    /// * `IoError::BrokenPipe` if the reader closed the pipe
    pub async fn shutdown(&mut self) -> IoResult<()> {
        self.inner.shutdown().await.map_err(Self::map_write_error)
    }

    /// Map I/O errors to IoError, handling broken pipe specially
    fn map_write_error(e: std::io::Error) -> IoError {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            IoError::broken_pipe("output pipe reader closed")
        } else {
            IoError::Io(e)
        }
    }
}

impl std::fmt::Debug for OutputWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputWriter")
            .field("sink", &self.sink)
            .finish()
    }
}

/// Check if an error is a broken pipe error
///
/// This is useful for determining if a write failure was due to the
/// reader closing the pipe (which may be expected in some scenarios).
pub fn is_broken_pipe(error: &IoError) -> bool {
    matches!(error, IoError::BrokenPipe { .. })
}
