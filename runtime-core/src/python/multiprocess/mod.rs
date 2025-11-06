//! Multiprocess execution for Python nodes via iceoryx2 IPC

pub mod data_transfer;
pub mod health_monitor;
pub mod ipc_channel;
pub mod multiprocess_executor;
pub mod process_manager;

pub use multiprocess_executor::MultiprocessExecutor;
