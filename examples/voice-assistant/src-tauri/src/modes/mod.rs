//! Execution mode management

pub mod fallback;
pub mod hybrid;
pub mod local;
pub mod remote;

/// Execution mode
#[derive(Debug, Clone)]
pub enum ExecutionMode {
    /// All processing happens locally
    Local,
    /// Local VAD/preprocessing + remote inference with local fallback
    Hybrid { remote_url: String },
    /// All processing happens on remote server
    Remote { server_url: String },
}

impl ExecutionMode {
    /// Get the mode name as a string
    pub fn name(&self) -> &str {
        match self {
            ExecutionMode::Local => "local",
            ExecutionMode::Hybrid { .. } => "hybrid",
            ExecutionMode::Remote { .. } => "remote",
        }
    }

    /// Check if this mode requires network connectivity
    pub fn requires_network(&self) -> bool {
        matches!(self, ExecutionMode::Hybrid { .. } | ExecutionMode::Remote { .. })
    }
}
