//! Session management and routing
//!
//! Manages streaming sessions and routes data between peers and pipelines.

pub mod session;

pub use session::{Session, SessionId, SessionManager, SessionState};
