//! Audio I/O using cpal

pub mod devices;
pub mod mic;
pub mod speaker;

pub use devices::*;
pub use mic::*;
pub use speaker::*;
