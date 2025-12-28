//! Audio I/O using cpal

pub mod devices;
pub mod mic;
pub mod speaker;
pub mod wav;

pub use devices::*;
pub use mic::*;
pub use speaker::*;
pub use wav::{is_wav, parse_wav, WavHeader};
