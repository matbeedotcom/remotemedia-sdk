//! Python integration module
//!
//! This module provides:
//! 1. FFI functions for Python to call Rust runtime
//! 2. Data marshaling between Python and Rust types  
//! 3. Python node execution via CPython (PyO3)
//! 4. Zero-copy numpy array integration

// FFI module requires python-async feature (pyo3-async-runtimes)
#[cfg(feature = "python-async")]
pub mod ffi;

pub mod marshal;
pub mod node_executor;
pub mod runtime_data_py;

// numpy_marshal requires native-numpy feature
#[cfg(feature = "native-numpy")]
pub mod numpy_marshal;

pub mod cpython_executor;
// pub mod cpython_node;  // Archived in v0.2.1 - adapter no longer needed

// Re-export FFI module for Python extension (only when python-async is enabled)
#[cfg(feature = "python-async")]
pub use ffi::*;

pub use cpython_executor::CPythonNodeExecutor;
pub use node_executor::PythonNodeInstance;
pub use runtime_data_py::{py_to_runtime_data, runtime_data_to_py, runtime_data_to_py_with_session, PyRuntimeData};
// pub use cpython_node::{CPythonNodeFactory, inputs_to_pydict, pydict_to_outputs};  // Archived in v0.2.1

// Multiprocess support modules (requires multiprocess feature)
#[cfg(feature = "multiprocess")]
pub mod multiprocess {
    pub mod data_transfer;
    pub mod health_monitor;
    pub mod ipc_channel;
    pub mod multiprocess_executor;
    pub mod process_manager;

    pub use multiprocess_executor::MultiprocessExecutor;
    pub use process_manager::ProcessManager;
}
