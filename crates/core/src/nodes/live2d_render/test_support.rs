//! Test-only [`Live2DBackend`] implementation.
//!
//! `MockBackend` records every `render_frame` call (the pose +
//! frame index) so input-arbitration assertions can run without a
//! GPU. The `WgpuBackend` (M4.4) implements the same trait and gets
//! fed identical poses; what's recorded here in tests is exactly
//! what the real renderer would have to render.
//!
//! Visibility:
//!
//! - Inside `crates/core` test code (`tests/`) the `cfg(test)` flag
//!   doesn't propagate from lib to integration test crate, so we
//!   gate exposure on `feature = "avatar-render-test-support"` too.
//! - Other crates that want to drive the state machine in their own
//!   tests can flip the feature on via `dev-dependencies`.

use super::backend_trait::BackendError;
use super::{Live2DBackend, Pose, RgbFrame};
use std::sync::{Arc, Mutex};

/// Recorded `render_frame` invocation.
#[derive(Debug, Clone)]
pub struct RecordedFrame {
    /// 0-indexed call number.
    pub index: usize,
    /// Snapshot of the pose passed to `render_frame`.
    pub pose: Pose,
}

/// Test-only backend that records every render call. The recorded
/// log is `Arc<Mutex<…>>` so a test can clone the handle, hand the
/// backend off to a renderer, and read the log out the side.
#[derive(Debug, Clone)]
pub struct MockBackend {
    inner: Arc<Mutex<MockState>>,
    width: u32,
    height: u32,
}

#[derive(Debug, Default)]
struct MockState {
    frames: Vec<RecordedFrame>,
}

impl MockBackend {
    /// Build a mock at the given dimensions. Defaults are
    /// 1280×720 (matches the wgpu backend's planned default).
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockState::default())),
            width,
            height,
        }
    }

    /// Convenience: 1280×720 default.
    pub fn default_hd() -> Self {
        Self::new(1280, 720)
    }

    /// Snapshot the recorded frames so far. Each call returns a
    /// fresh `Vec` — the backend itself keeps its log.
    pub fn recorded(&self) -> Vec<RecordedFrame> {
        self.inner.lock().unwrap().frames.clone()
    }

    /// How many `render_frame` calls have been recorded.
    pub fn render_count(&self) -> usize {
        self.inner.lock().unwrap().frames.len()
    }

    /// Clear the recording. Useful when re-using a single backend
    /// across multiple test phases.
    pub fn reset(&self) {
        self.inner.lock().unwrap().frames.clear();
    }
}

impl Live2DBackend for MockBackend {
    fn render_frame(&mut self, pose: &Pose) -> Result<RgbFrame, BackendError> {
        let mut s = self.inner.lock().unwrap();
        let index = s.frames.len();
        s.frames.push(RecordedFrame { index, pose: pose.clone() });
        // Return a black frame at the configured size — tests that
        // care about pixel content should use the wgpu backend
        // instead. The mock's job is to capture the pose stream.
        Ok(RgbFrame::black(self.width, self.height))
    }

    fn frame_dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
