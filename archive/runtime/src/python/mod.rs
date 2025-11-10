//! Python integration module
//!
//! This module provides:
//! 1. Data marshaling between Python and Rust types
//! 2. Python node execution via CPython (PyO3)
//! 3. Zero-copy numpy array integration
//! 4. Multiprocess Python execution with iceoryx2 IPC
//!
//! ## v0.4.0 Changes
//! - FFI functions extracted to transports/remotemedia-ffi crate
//! - Legacy FFI code archived in archive/legacy-python-ffi/
//! - marshal.rs and numpy_marshal.rs moved to FFI transport

pub mod node_executor;
pub mod runtime_data_py;

pub mod cpython_executor;
// pub mod cpython_node;  // Archived in v0.2.1 - adapter no longer needed

// DEPRECATED in v0.3.0: Use MultiprocessExecutor instead
// CPythonNodeExecutor is kept for backward compatibility but will be removed in v0.4.0
#[deprecated(
    since = "0.3.0",
    note = "Use MultiprocessExecutor instead. CPythonNodeExecutor will be removed in v0.4.0"
)]
pub use cpython_executor::CPythonNodeExecutor;

pub use node_executor::PythonNodeInstance;
pub use runtime_data_py::{
    py_to_runtime_data, runtime_data_to_py, runtime_data_to_py_with_session, PyRuntimeData,
};
// pub use cpython_node::{CPythonNodeFactory, inputs_to_pydict, pydict_to_outputs};  // Archived in v0.2.1

/// Multiprocess execution support modules (requires multiprocess feature)
///
/// This module provides true process-level isolation for Python nodes via iceoryx2 IPC.
/// Use `MultiprocessExecutor` instead of the deprecated `CPythonNodeExecutor`.
#[cfg(feature = "multiprocess")]
pub mod multiprocess {
    pub mod data_transfer;
    pub mod health_monitor;
    pub mod ipc_channel;
    pub mod multiprocess_executor;
    pub mod process_manager;

    // Re-export commonly used types
    pub use data_transfer::{DataType, RuntimeData};
    pub use ipc_channel::{ChannelHandle, ChannelRegistry};
    pub use multiprocess_executor::{
        InitProgress, InitStatus, MultiprocessConfig, MultiprocessExecutor, SessionState,
        SessionStatus,
    };
    pub use process_manager::ProcessManager;
}
