//! Unified async reader for files, pipes, and stdin

use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncReadExt, BufReader};

use super::{InputSource, IoError, IoResult};

/// Unified async reader that handles files, named pipes, and stdin
///
/// This struct wraps different input sources behind a common async read interface,
/// allowing the same code to read from regular files, FIFOs, or standard input.
pub struct InputReader {
    inner: BufReader<Pin<Box<dyn AsyncRead + Send + Unpin>>>,
    source: InputSource,
}

impl InputReader {
    /// Open an input source for reading
    ///
    /// # Arguments
    /// * `source` - The input source to open
    ///
    /// # Returns
    /// An `InputReader` ready to read from the source
    ///
    /// # Errors
    /// * `IoError::Io` if the file/pipe cannot be opened
    ///
    /// # Notes
    /// For named pipes, this will block until a writer connects (standard Unix behavior).
    pub async fn open(source: InputSource) -> IoResult<Self> {
        let reader: Pin<Box<dyn AsyncRead + Send + Unpin>> = match &source {
            InputSource::File(path) => {
                let file = tokio::fs::File::open(path).await.map_err(|e| {
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
            InputSource::Pipe(path) => {
                // Named pipes are opened the same way as files in async context
                // The open() call will block until a writer connects
                let file = tokio::fs::File::open(path).await.map_err(|e| {
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
            InputSource::Stdin => Box::pin(tokio::io::stdin()),
        };

        Ok(Self {
            inner: BufReader::new(reader),
            source,
        })
    }

    /// Get a reference to the input source
    pub fn source(&self) -> &InputSource {
        &self.source
    }

    /// Read data into a buffer
    ///
    /// # Arguments
    /// * `buf` - Buffer to read into
    ///
    /// # Returns
    /// Number of bytes read, or 0 if end of stream
    pub async fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.inner.read(buf).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                IoError::broken_pipe("input pipe closed")
            } else {
                IoError::Io(e)
            }
        })
    }

    /// Read all remaining data into a vector
    ///
    /// # Returns
    /// All remaining data from the source
    ///
    /// # Warning
    /// For streaming sources (pipes, stdin), this will block until EOF.
    /// For large inputs, consider using `read()` in a loop instead.
    pub async fn read_to_end(&mut self) -> IoResult<Vec<u8>> {
        let mut buf = Vec::new();
        self.inner.read_to_end(&mut buf).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                IoError::broken_pipe("input pipe closed")
            } else {
                IoError::Io(e)
            }
        })?;
        Ok(buf)
    }

    /// Read an exact number of bytes
    ///
    /// # Arguments
    /// * `buf` - Buffer to fill completely
    ///
    /// # Errors
    /// Returns error if EOF is reached before buffer is filled
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> IoResult<()> {
        self.inner
            .read_exact(buf)
            .await
            .map(|_| ())
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    IoError::EndOfStream
                } else if e.kind() == std::io::ErrorKind::BrokenPipe {
                    IoError::broken_pipe("input pipe closed")
                } else {
                    IoError::Io(e)
                }
            })
    }
}

impl std::fmt::Debug for InputReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputReader")
            .field("source", &self.source)
            .finish()
    }
}
