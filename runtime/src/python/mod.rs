//! Python integration module
//!
//! This module provides:
//! 1. FFI functions for Python to call Rust runtime
//! 2. Data marshaling between Python and Rust types
//! 3. RustPython VM integration (Phase 1.5)
//! 4. Python node execution (Phase 1.6)

pub mod ffi;
pub mod marshal;
pub mod vm;
pub mod node_executor;
pub mod numpy_marshal;

// Re-export FFI module for Python extension
pub use ffi::*;
pub use vm::{PythonVm, VmConfig, VmPool};
pub use node_executor::PythonNodeInstance;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_creation() {
        let vm = PythonVm::new();
        assert!(vm.is_ok());
    }
}
