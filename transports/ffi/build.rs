//! Build script for remotemedia-ffi
//!
//! Conditionally runs napi-build when the napi feature is enabled.

fn main() {
    #[cfg(feature = "napi")]
    {
        napi_build::setup();
    }
}
