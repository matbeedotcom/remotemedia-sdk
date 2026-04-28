//! Tier-2 integration test for the Audio2Face inference pipeline.
//!
//! Loads the actual persona-engine bundle (`network.onnx` +
//! `bs_skin_<Identity>.npz` + `model_data_<Identity>.npz` +
//! `bs_skin_config_<Identity>.json`) and exercises the full
//! `BlendshapeConfig::from_path` → `BlendshapeData::load` →
//! `Audio2FaceInference::load` → `infer` chain.
//!
//! Skips cleanly via [`skip_if_no_real_avatar_models!`] when
//! `AUDIO2FACE_TEST_BUNDLE` isn't set — CI hosts that don't have
//! the 738 MiB bundle on disk pass without action. Run locally
//! after `scripts/install-audio2face.sh` with:
//!
//! ```bash
//! export AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face
//! cargo test -p remotemedia-core --features avatar-audio2face \
//!   --test audio2face_inference_test
//! ```

#![cfg(feature = "avatar-audio2face")]

#[path = "avatar_test_support.rs"]
mod support;

use remotemedia_core::nodes::lip_sync::audio2face::{
    Audio2FaceIdentity, Audio2FaceInference, BlendshapeConfig, BlendshapeData, BundlePaths,
};
use std::path::PathBuf;

use support::sine_sweep_16k_mono;

fn bundle_root() -> PathBuf {
    PathBuf::from(std::env::var("AUDIO2FACE_TEST_BUNDLE").expect("env var checked by macro"))
}

#[test]
fn parses_real_claire_config() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let paths = BundlePaths::new(bundle_root(), Audio2FaceIdentity::Claire);
    let config = BlendshapeConfig::from_path(paths.bs_skin_config()).expect("config");
    assert_eq!(config.num_poses, 52);
    // Per the bundled bs_skin_config_Claire.json: 39 of 52 poses are
    // active. The off ones are eye-look L/R (×4 each), jaw L/R, mouth
    // L/R, and tongueOut — i.e. blendshapes the model can't reliably
    // predict from audio alone.
    assert_eq!(
        config.active_count(),
        39,
        "expected Claire to have 39 active poses; got {}",
        config.active_count()
    );
    assert!(config.template_bb_size > 40.0 && config.template_bb_size < 50.0);
    assert_eq!(config.multipliers.len(), 52);
    assert_eq!(config.offsets.len(), 52);
}

#[test]
fn loads_real_claire_blendshape_data() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let paths = BundlePaths::new(bundle_root(), Audio2FaceIdentity::Claire);
    let config = BlendshapeConfig::from_path(paths.bs_skin_config()).expect("config");
    let data =
        BlendshapeData::load(paths.bs_skin_npz(), paths.model_data_npz(), &config).expect("data");

    // Sanity-check shapes against the known bundle layout (24002
    // vertices full, frontal mask is a subset).
    assert!(data.frontal_mask.len() > 0);
    assert!(data.masked_position_count >= data.frontal_mask.len() * 3);
    assert_eq!(data.masked_position_count, data.frontal_mask.len() * 3);
    assert_eq!(data.active_count, config.active_count());
    assert_eq!(
        data.delta_matrix.len(),
        data.masked_position_count * data.active_count
    );
    assert_eq!(data.neutral_skin_flat.len(), 24_002 * 3);
    assert_eq!(data.eye_close_pose_delta_flat.len(), 24_002 * 3);
    assert_eq!(data.lip_open_pose_delta_flat.len(), 24_002 * 3);

    // Saccade matrix presence varies by bundle version; just exercise the path.
    if let Some(rows) = data.saccade_rot_rows {
        assert_eq!(rows * 2, data.saccade_rot_flat.as_ref().unwrap().len());
    }
}

#[test]
fn audio2face_infer_produces_expected_output_shape() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let paths = BundlePaths::new(bundle_root(), Audio2FaceIdentity::Claire);
    let mut inference =
        Audio2FaceInference::load(paths.network_onnx(), false).expect("load Audio2Face");

    // 1 second of synthetic sine-sweep audio at 16 kHz.
    let audio = sine_sweep_16k_mono(1.0, 220.0, 880.0);
    let out = inference
        .infer(&audio, Audio2FaceIdentity::Claire.one_hot_index())
        .expect("infer");

    use remotemedia_core::nodes::lip_sync::audio2face::inference::{
        EYES_SIZE, NUM_CENTER_FRAMES, SKIN_SIZE,
    };
    assert_eq!(out.frame_count, NUM_CENTER_FRAMES);
    assert_eq!(out.skin_flat.len(), NUM_CENTER_FRAMES * SKIN_SIZE);
    assert_eq!(out.eye_flat.len(), NUM_CENTER_FRAMES * EYES_SIZE);

    // Sanity: outputs aren't all zeros for non-silent input. (A
    // model that accidentally became deterministic-zero would still
    // pass the shape check but not this content check.)
    let nonzero_skin = out.skin_flat.iter().filter(|&&v| v.abs() > 1e-6).count();
    assert!(
        nonzero_skin > 100,
        "expected non-trivial skin activity; got {nonzero_skin} non-zero floats out of {}",
        out.skin_flat.len()
    );
}

#[test]
fn gru_state_carries_temporal_context() {
    skip_if_no_real_avatar_models!("AUDIO2FACE_TEST_BUNDLE");
    let paths = BundlePaths::new(bundle_root(), Audio2FaceIdentity::Claire);
    let mut inference =
        Audio2FaceInference::load(paths.network_onnx(), false).expect("load Audio2Face");

    // Two consecutive infers of the same audio should NOT produce
    // identical output (GRU state advances between calls). Two
    // consecutive infers separated by reset_state SHOULD start
    // producing the same first-frame output.
    let audio = sine_sweep_16k_mono(1.0, 220.0, 880.0);
    let idx = Audio2FaceIdentity::Claire.one_hot_index();

    let a = inference.infer(&audio, idx).expect("infer #1");
    let b = inference.infer(&audio, idx).expect("infer #2 (state advanced)");
    let differ = a
        .skin_flat
        .iter()
        .zip(&b.skin_flat)
        .any(|(x, y)| (x - y).abs() > 1e-4);
    assert!(differ, "consecutive infers should differ — GRU state should advance");

    inference.reset_state();
    let c = inference.infer(&audio, idx).expect("infer #3 (post-reset)");
    // After reset, infer should produce output close to the first call's.
    let close: f64 = a
        .skin_flat
        .iter()
        .zip(&c.skin_flat)
        .map(|(x, y)| ((x - y) as f64).abs())
        .sum::<f64>()
        / a.skin_flat.len() as f64;
    assert!(
        close < 1e-3,
        "post-reset infer should match first-call infer; mean abs diff = {close}"
    );
}
