//! M4.3 integration test: drive `Live2DRenderState` through a
//! `MockBackend` and assert the recorded pose stream matches the
//! input arbitration spec (§6.1) end-to-end.
//!
//! The unit tests inside `state.rs` cover the state machine in
//! isolation. This file exercises the *backend trait seam* — the
//! pose flow that the wgpu+CubismCore backend (M4.4) will replace
//! `MockBackend` against. If the seam shape changes, this test
//! catches it.

#![cfg(feature = "avatar-render-test-support")]

use remotemedia_core::nodes::lip_sync::BlendshapeFrame;
use remotemedia_core::nodes::live2d_render::{
    Live2DBackend, Live2DRenderState, MockBackend, StateConfig,
};
use std::time::Duration;

const ARKIT_52: usize = 52;

/// Build a default state machine + a default 1280x720 mock backend
/// for tests. Returns the pair so each test can drive both.
fn fixture() -> (Live2DRenderState, MockBackend) {
    (
        Live2DRenderState::new(StateConfig::default_config()),
        MockBackend::default_hd(),
    )
}

/// Drive one render tick: compute the pose and hand it to the
/// backend. Mirrors what the M4.5 `Live2DRenderNode` will do on
/// every frame.
fn render_one(state: &mut Live2DRenderState, backend: &mut MockBackend) {
    let pose = state.compute_pose();
    backend.render_frame(&pose).expect("backend.render_frame");
}

#[test]
fn render_seam_records_one_pose_per_call() {
    let (mut state, mut backend) = fixture();
    state.push_blendshape(BlendshapeFrame::new([0.5; ARKIT_52], 100, None));
    state.update_audio_clock(100);

    render_one(&mut state, &mut backend);
    state.tick_wall(Duration::from_millis(33));
    render_one(&mut state, &mut backend);
    state.tick_wall(Duration::from_millis(33));
    render_one(&mut state, &mut backend);

    let frames = backend.recorded();
    assert_eq!(frames.len(), 3);
    // Indices are monotonically ascending.
    assert_eq!(frames[0].index, 0);
    assert_eq!(frames[1].index, 1);
    assert_eq!(frames[2].index, 2);
}

#[test]
fn render_seam_carries_lerped_mouth_to_backend() {
    let (mut state, mut backend) = fixture();
    state.push_blendshape(BlendshapeFrame::new([0.0; ARKIT_52], 0, None));
    state.push_blendshape(BlendshapeFrame::new([1.0; ARKIT_52], 200, None));
    state.update_audio_clock(100); // exact midpoint
    render_one(&mut state, &mut backend);

    let frames = backend.recorded();
    assert_eq!(frames.len(), 1);
    let p = &frames[0].pose;
    assert!(
        (p.mouth_value("ParamJawOpen") - 0.5).abs() < 1e-3,
        "expected ~0.5, got {}",
        p.mouth_value("ParamJawOpen")
    );
    assert!(
        (p.mouth_value("ParamMouthOpenY") - 0.5).abs() < 1e-3,
        "ParamMouthOpenY should also map to jawOpen"
    );
}

#[test]
fn render_seam_carries_emotion_metadata_to_backend() {
    let (mut state, mut backend) = fixture();
    state.push_emotion("\u{1F929}"); // 🤩 → excited_star + Excited
    render_one(&mut state, &mut backend);

    let frames = backend.recorded();
    assert_eq!(frames[0].pose.expression_id, "excited_star");
    assert_eq!(frames[0].pose.motion_group, "Excited");
}

#[test]
fn render_seam_reflects_emotion_expiration() {
    let mut state = Live2DRenderState::new(StateConfig {
        expression_hold_seconds: 0.5,
        ..StateConfig::default_config()
    });
    let mut backend = MockBackend::default_hd();

    state.push_emotion("\u{1F929}"); // 🤩
    render_one(&mut state, &mut backend);
    state.tick_wall(Duration::from_millis(600));
    render_one(&mut state, &mut backend);

    let frames = backend.recorded();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].pose.expression_id, "excited_star");
    assert_eq!(frames[1].pose.expression_id, "neutral");
}

#[test]
fn render_seam_reflects_barge_clearing_ring_but_keeping_emotion() {
    let (mut state, mut backend) = fixture();
    state.push_blendshape(BlendshapeFrame::new([1.0; ARKIT_52], 100, None));
    state.update_audio_clock(100);
    state.push_emotion("\u{1F929}");

    render_one(&mut state, &mut backend);
    state.handle_barge();
    // Sample again post-barge — mouth is neutral, emotion still active.
    render_one(&mut state, &mut backend);

    let frames = backend.recorded();
    assert_eq!(frames.len(), 2);
    // Pre-barge: jaw open from blendshape.
    assert!(frames[0].pose.mouth_value("ParamJawOpen") > 0.5);
    assert_eq!(frames[0].pose.expression_id, "excited_star");
    // Post-barge: mouth neutral, emotion still excited_star.
    assert_eq!(frames[1].pose.mouth_value("ParamJawOpen"), 0.0);
    assert_eq!(frames[1].pose.expression_id, "excited_star");
}

#[test]
fn render_seam_blink_progresses_through_pose_stream() {
    let mut state = Live2DRenderState::new(StateConfig {
        blink_interval_min_ms: 100,
        blink_interval_max_ms: 100,
        blink_duration_ms: 200,
        ..StateConfig::default_config()
    });
    let mut backend = MockBackend::default_hd();

    // First render at wall=0: blink not yet started → eye_open = 1.0.
    render_one(&mut state, &mut backend);
    // Advance to wall=150: 50ms into the 200ms blink (closing half).
    state.tick_wall(Duration::from_millis(150));
    render_one(&mut state, &mut backend);
    // Advance to wall=300: blink fully complete; eye_open back to 1.0,
    // and the next blink is scheduled for wall=400.
    state.tick_wall(Duration::from_millis(150));
    render_one(&mut state, &mut backend);

    let frames = backend.recorded();
    assert_eq!(frames[0].pose.eye_open, 1.0, "no blink yet");
    assert!(
        frames[1].pose.eye_open < 1.0 && frames[1].pose.eye_open > 0.0,
        "mid-blink should be (0, 1), got {}",
        frames[1].pose.eye_open
    );
    assert_eq!(frames[2].pose.eye_open, 1.0, "blink complete");
}

#[test]
fn backend_dimensions_are_reported_consistently() {
    let mut backend = MockBackend::new(640, 480);
    assert_eq!(backend.frame_dimensions(), (640, 480));

    let pose = remotemedia_core::nodes::live2d_render::Pose::default();
    let frame = backend.render_frame(&pose).expect("render");
    assert_eq!(frame.width, 640);
    assert_eq!(frame.height, 480);
    assert_eq!(frame.pixels.len(), 640 * 480 * 3);
    // Mock returns a black frame.
    assert_eq!(frame.nonzero_byte_count(), 0);
}

#[test]
fn backend_recording_can_be_reset() {
    let mut backend = MockBackend::default_hd();
    let pose = remotemedia_core::nodes::live2d_render::Pose::default();
    backend.render_frame(&pose).unwrap();
    backend.render_frame(&pose).unwrap();
    assert_eq!(backend.render_count(), 2);
    backend.reset();
    assert_eq!(backend.render_count(), 0);
}
