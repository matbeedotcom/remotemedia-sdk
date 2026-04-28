//! Tier-2 integration test for `Audio2FaceLipSyncNode` — the
//! coordinator that wires Audio2FaceInference + PGD/BVLS solver +
//! ArkitSmoother into a streaming `LipSyncNode`.
//!
//! Skips cleanly via `skip_if_no_real_avatar_models!` when
//! `AUDIO2FACE_TEST_BUNDLE` isn't set. Run locally after
//! `scripts/install-audio2face.sh` with:
//!
//! ```bash
//! export AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face
//! cargo test -p remotemedia-core --features avatar-audio2face \
//!   --test audio2face_lipsync_node_test
//! ```

#![cfg(feature = "avatar-audio2face")]

#[path = "avatar_test_support.rs"]
mod support;

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::lip_sync::audio2face::Audio2FaceIdentity;
use remotemedia_core::nodes::lip_sync::audio2face::inference::NUM_CENTER_FRAMES;
use remotemedia_core::nodes::lip_sync::{
    Audio2FaceLipSyncConfig, Audio2FaceLipSyncNode, Audio2FaceSolverChoice, BlendshapeFrame,
    LipSyncNode,
};
use remotemedia_core::nodes::AsyncStreamingNode;
use remotemedia_core::transport::session_control::{wrap_aux_port, BARGE_IN_PORT};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use support::sine_sweep_16k_mono;

fn bundle_root() -> PathBuf {
    PathBuf::from(std::env::var("AUDIO2FACE_TEST_BUNDLE").expect("env var checked by macro"))
}

fn audio_chunk(samples: Vec<f32>, sample_rate: u32) -> RuntimeData {
    RuntimeData::Audio {
        samples: AudioSamples::Vec(samples),
        sample_rate,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

async fn drive(node: &Audio2FaceLipSyncNode, data: RuntimeData) -> Vec<RuntimeData> {
    let collected = Arc::new(Mutex::new(Vec::new()));
    let cc = collected.clone();
    node.process_streaming(data, None, move |out| {
        cc.lock().unwrap().push(out);
        Ok(())
    })
    .await
    .expect("process_streaming");
    let v = collected.lock().unwrap().clone();
    v
}

fn default_config() -> Audio2FaceLipSyncConfig {
    Audio2FaceLipSyncConfig {
        bundle_path: bundle_root(),
        identity: Audio2FaceIdentity::Claire,
        solver: Audio2FaceSolverChoice::Pgd,
        use_gpu: false,
        smoothing_alpha: 0.0,
    }
}

#[tokio::test]
async fn one_second_window_emits_thirty_frames() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");
    assert_eq!(node.required_sample_rate(), 16_000);

    let audio = sine_sweep_16k_mono(1.0, 220.0, 880.0);
    let outs = drive(&node, audio_chunk(audio, 16_000)).await;

    assert_eq!(
        outs.len(),
        NUM_CENTER_FRAMES,
        "1 sec audio → 1 inference window → 30 BlendshapeFrames"
    );
    for out in &outs {
        match out {
            RuntimeData::Json(v) => {
                assert_eq!(v.get("kind").and_then(|k| k.as_str()), Some("blendshapes"));
            }
            _ => panic!("expected Json"),
        }
    }
}

#[tokio::test]
async fn pts_ms_is_monotonic_and_aligned_to_audio_clock() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");

    // 2 seconds of audio = 2 inference windows = 60 frames.
    let audio = sine_sweep_16k_mono(2.0, 220.0, 880.0);
    let outs = drive(&node, audio_chunk(audio, 16_000)).await;
    assert_eq!(outs.len(), 2 * NUM_CENTER_FRAMES);

    let mut prev_pts: Option<u64> = None;
    for (i, out) in outs.iter().enumerate() {
        let json = match out {
            RuntimeData::Json(v) => v,
            _ => panic!("expected Json"),
        };
        let frame = BlendshapeFrame::from_json(json).expect("frame");
        if let Some(p) = prev_pts {
            assert!(
                frame.pts_ms >= p,
                "pts_ms must be monotonic: frame {i} pts={} < prev={}",
                frame.pts_ms,
                p
            );
        }
        prev_pts = Some(frame.pts_ms);
    }
    // Last frame is at start of the second window plus 29 * 33.33 ≈ 967 ms.
    let last = BlendshapeFrame::from_json(outs.last().unwrap().as_json()).unwrap();
    assert!(
        last.pts_ms >= 1_900,
        "second-window last pts should be near 2s, got {}",
        last.pts_ms
    );
    assert!(
        last.pts_ms < 2_000,
        "frames represent the center of the window, never beyond it"
    );
}

#[tokio::test]
async fn buffer_accumulates_across_chunks() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");

    // 0.5 sec → no inference yet.
    let half = sine_sweep_16k_mono(0.5, 220.0, 880.0);
    let outs1 = drive(&node, audio_chunk(half.clone(), 16_000)).await;
    assert_eq!(outs1.len(), 0, "half-second alone does not trigger inference");

    // Another 0.5 sec → buffer crosses 16000 → one full window.
    let outs2 = drive(&node, audio_chunk(half, 16_000)).await;
    assert_eq!(
        outs2.len(),
        NUM_CENTER_FRAMES,
        "two halves should fire one inference"
    );
}

#[tokio::test]
async fn non_audio_passes_through() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");
    let outs = drive(&node, RuntimeData::Text("hello".into())).await;
    assert_eq!(outs.len(), 1);
    match &outs[0] {
        RuntimeData::Text(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Text passthrough"),
    }
}

#[tokio::test]
async fn wrong_sample_rate_errors() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");
    let collected = Arc::new(Mutex::new(Vec::new()));
    let cc = collected.clone();
    let err = node
        .process_streaming(audio_chunk(vec![0.0; 16_000], 22_050), None, move |o| {
            cc.lock().unwrap().push(o);
            Ok(())
        })
        .await
        .expect_err("must reject non-16kHz audio");
    assert!(format!("{err}").contains("16 kHz"));
}

#[tokio::test]
async fn barge_in_clears_state_and_drains_buffer() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");

    // Partial buffer (won't trigger).
    let _ = drive(
        &node,
        audio_chunk(sine_sweep_16k_mono(0.5, 220.0, 880.0), 16_000),
    )
    .await;

    // Send barge envelope. Should drop buffered audio and emit nothing.
    // The kind:barge_in JSON is the in-band path the node accepts on
    // its data channel; the manifest-level path goes through
    // process_control_message (covered by the M2.6 router test).
    let barge = RuntimeData::Json(serde_json::json!({"kind": "barge_in"}));
    let outs = drive(&node, barge).await;
    assert_eq!(outs.len(), 0, "barge emits no frames");

    // Now half a second again: still shouldn't fire (buffer was cleared).
    let outs = drive(
        &node,
        audio_chunk(sine_sweep_16k_mono(0.5, 220.0, 880.0), 16_000),
    )
    .await;
    assert_eq!(outs.len(), 0, "barge should have drained the audio buffer");

    // Another half second: now we have 1s and should fire.
    let outs = drive(
        &node,
        audio_chunk(sine_sweep_16k_mono(0.5, 220.0, 880.0), 16_000),
    )
    .await;
    assert_eq!(outs.len(), NUM_CENTER_FRAMES);

    // pts_ms should restart from 0 (cum_window_ms reset by barge).
    let first = BlendshapeFrame::from_json(outs[0].as_json()).unwrap();
    assert!(first.pts_ms < 50, "post-barge pts_ms restarts from 0");
}

#[tokio::test]
async fn loud_audio_produces_nontrivial_blendshape_activity() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");

    let audio = sine_sweep_16k_mono(1.0, 220.0, 880.0);
    let outs = drive(&node, audio_chunk(audio, 16_000)).await;

    // Mid-window frame — most "settled" GRU state.
    let mid = BlendshapeFrame::from_json(outs[15].as_json()).unwrap();
    let nonzero = mid.arkit_52.iter().filter(|&&v| v.abs() > 1e-3).count();
    assert!(
        nonzero > 0,
        "mid-window frame should have at least some non-zero blendshape activations"
    );
}

/// M2.6: barge envelope dispatched via `process_control_message` (the
/// path the session router takes when a coordinator's
/// `barge_in_targets` includes this node) clears state identically to
/// the in-band JSON path tested above. Asserts the
/// `wrap_aux_port(BARGE_IN_PORT, …)` envelope produced by
/// `SessionControl::publish` makes its way through correctly.
#[tokio::test]
async fn process_control_message_barge_in_clears_state() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");

    // Half-second to seed the buffer + advance the clock.
    let _ = drive(
        &node,
        audio_chunk(sine_sweep_16k_mono(0.5, 220.0, 880.0), 16_000),
    )
    .await;

    // Wrapped aux-port envelope, exactly as `SessionControl::publish`
    // for `<node>.in.barge_in` would build.
    let envelope = wrap_aux_port(BARGE_IN_PORT, RuntimeData::Json(serde_json::json!({})));
    let handled = node
        .process_control_message(envelope, Some("session-1".into()))
        .await
        .expect("control message ok");
    assert!(handled, "node must report it handled the barge envelope");

    // Buffer was drained → another half-second alone shouldn't fire.
    let outs = drive(
        &node,
        audio_chunk(sine_sweep_16k_mono(0.5, 220.0, 880.0), 16_000),
    )
    .await;
    assert_eq!(outs.len(), 0, "barge should have drained the buffer");

    // Another half-second crosses the 1-sec window → 30 frames, with
    // pts_ms restarting at zero (cum_window_ms reset by barge).
    let outs = drive(
        &node,
        audio_chunk(sine_sweep_16k_mono(0.5, 220.0, 880.0), 16_000),
    )
    .await;
    assert_eq!(outs.len(), NUM_CENTER_FRAMES);
    let first = BlendshapeFrame::from_json(outs[0].as_json()).unwrap();
    assert!(first.pts_ms < 50, "post-barge pts_ms restarts from 0");
}

/// Non-barge control messages (e.g. unrelated Json) return
/// `Ok(false)` so the runtime's universal fallback semantics still
/// apply (i.e. messages that aren't the barge port pass through any
/// other handler if one is added later).
#[tokio::test]
async fn process_control_message_ignores_non_barge() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let node = Audio2FaceLipSyncNode::load(default_config()).expect("load node");

    let unrelated = RuntimeData::Json(serde_json::json!({"kind": "context", "text": "hi"}));
    let handled = node
        .process_control_message(unrelated, None)
        .await
        .expect("ok");
    assert!(!handled);
}

#[tokio::test]
async fn bvls_solver_produces_valid_output_too() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    // BVLS is materially slower than PGD; one window is plenty.
    let cfg = Audio2FaceLipSyncConfig {
        solver: Audio2FaceSolverChoice::Bvls,
        ..default_config()
    };
    let node = Audio2FaceLipSyncNode::load(cfg).expect("load with BVLS");
    let audio = sine_sweep_16k_mono(1.0, 220.0, 880.0);
    let outs = drive(&node, audio_chunk(audio, 16_000)).await;
    assert_eq!(outs.len(), NUM_CENTER_FRAMES);
    let frame = BlendshapeFrame::from_json(outs[15].as_json()).unwrap();
    let nonzero = frame.arkit_52.iter().filter(|&&v| v.abs() > 1e-3).count();
    assert!(nonzero > 0, "BVLS solver should also produce activity");
}

/// Helper trait so tests can do `out.as_json()` without verbose matching.
trait AsJsonRef {
    fn as_json(&self) -> &serde_json::Value;
}
impl AsJsonRef for RuntimeData {
    fn as_json(&self) -> &serde_json::Value {
        match self {
            RuntimeData::Json(v) => v,
            _ => panic!("expected Json variant"),
        }
    }
}
