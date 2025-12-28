//! Path type detection for files, named pipes, and stdin/stdout

use std::path::Path;

use super::{InputSource, IoError, IoResult, OutputSink};

/// Detect the type of input source from a path string
///
/// # Arguments
/// * `path` - Path string, or `-` for stdin
///
/// # Returns
/// * `InputSource::Stdin` if path is `-`
/// * `InputSource::Pipe` if path is a named pipe (FIFO)
/// * `InputSource::File` if path is a regular file
///
/// # Errors
/// * `IoError::PathNotFound` if the path doesn't exist
/// * `IoError::PermissionDenied` if the path isn't readable
pub fn detect_input_source(path: &str) -> IoResult<InputSource> {
    // Handle stdin shorthand
    if path == "-" {
        return Ok(InputSource::Stdin);
    }

    let path_buf = std::path::PathBuf::from(path);

    // Check if path exists
    if !path_buf.exists() {
        return Err(IoError::path_not_found(&path_buf));
    }

    // Check if it's a FIFO (named pipe)
    if is_fifo(&path_buf) {
        return Ok(InputSource::Pipe(path_buf));
    }

    // Check if it's a regular file
    if path_buf.is_file() {
        return Ok(InputSource::File(path_buf));
    }

    // Not a file or pipe
    Err(IoError::InvalidPathType { path: path_buf })
}

/// Detect the type of output sink from a path string
///
/// # Arguments
/// * `path` - Path string, or `-` for stdout
///
/// # Returns
/// * `OutputSink::Stdout` if path is `-`
/// * `OutputSink::Pipe` if path is a named pipe (FIFO)
/// * `OutputSink::File` for any other path (will be created if needed)
///
/// # Notes
/// Unlike input detection, output paths don't need to exist (they can be created).
/// However, if a path exists and is a FIFO, it will be detected as a pipe.
pub fn detect_output_sink(path: &str) -> IoResult<OutputSink> {
    // Handle stdout shorthand
    if path == "-" {
        return Ok(OutputSink::Stdout);
    }

    let path_buf = std::path::PathBuf::from(path);

    // If path exists, check if it's a FIFO
    if path_buf.exists() {
        if is_fifo(&path_buf) {
            return Ok(OutputSink::Pipe(path_buf));
        }
        // Existing file - will be overwritten
        return Ok(OutputSink::File(path_buf));
    }

    // Non-existent path - will be created as a regular file
    Ok(OutputSink::File(path_buf))
}

/// Check if a path is a named pipe (FIFO)
///
/// # Arguments
/// * `path` - Path to check
///
/// # Returns
/// `true` if the path is a FIFO, `false` otherwise (including if path doesn't exist)
///
/// # Platform Support
/// - Unix: Uses `FileTypeExt::is_fifo()`
/// - Windows: Always returns `false` (Windows named pipes use a different mechanism)
#[cfg(unix)]
pub fn is_fifo(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;

    match std::fs::metadata(path) {
        Ok(metadata) => metadata.file_type().is_fifo(),
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub fn is_fifo(_path: &Path) -> bool {
    // Windows named pipes use a different mechanism (\\.\pipe\name)
    // This feature is Unix-only for now
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_stdin() {
        let source = detect_input_source("-").unwrap();
        assert_eq!(source, InputSource::Stdin);
        assert!(source.is_stdin());
    }

    #[test]
    fn test_detect_stdout() {
        let sink = detect_output_sink("-").unwrap();
        assert_eq!(sink, OutputSink::Stdout);
        assert!(sink.is_stdout());
    }

    #[test]
    fn test_detect_nonexistent_input() {
        let result = detect_input_source("/nonexistent/path/to/file");
        assert!(matches!(result, Err(IoError::PathNotFound { .. })));
    }

    #[test]
    fn test_detect_nonexistent_output() {
        // Non-existent output paths are OK (will be created)
        let sink = detect_output_sink("/tmp/new_output_file_test").unwrap();
        assert!(matches!(sink, OutputSink::File(_)));
    }

    #[cfg(unix)]
    #[test]
    fn test_detect_regular_file() {
        // Use Cargo.toml as a known existing file
        let source =
            detect_input_source(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
        assert!(matches!(source, InputSource::File(_)));
        assert!(!source.is_pipe());
    }
}
