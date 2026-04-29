//! M4.4 tier-2 integration test: load Aria via the wgpu backend
//! and render a frame.
//!
//! Three env-vars gate:
//! - `LIVE2D_CUBISM_CORE_DIR` (required at compile time — the
//!   `cubism-core-sys` build script reads it).
//! - `LIVE2D_TEST_MODEL_PATH` (runtime — points at
//!   `aria.model3.json`).
//!
//! Skips cleanly when either env var is unset OR when no GPU
//! adapter is available on the host (CI sandboxes without
//! lavapipe / mock-icd, etc.). Saves the rendered PNG to
//! `target/avatar-render-tests/aria.png` so a developer can open
//! it with `open target/avatar-render-tests/aria.png` after the
//! test completes — first visible Aria render lands here.

#![cfg(feature = "avatar-render-wgpu")]

use remotemedia_core::nodes::live2d_render::{
    Live2DBackend, Live2DRenderState, Pose, StateConfig, WgpuBackend,
};
use std::path::PathBuf;

fn aria_model_path() -> Option<PathBuf> {
    let p = std::env::var("LIVE2D_TEST_MODEL_PATH").ok()?;
    let p = PathBuf::from(p);
    if p.exists() { Some(p) } else { None }
}

macro_rules! skip_if_no_aria {
    () => {
        match aria_model_path() {
            Some(p) => p,
            None => {
                eprintln!(
                    "[skip] LIVE2D_TEST_MODEL_PATH not set; install Aria \
                     via scripts/install-live2d-aria.sh"
                );
                return;
            }
        }
    };
}

fn try_init_backend(width: u32, height: u32) -> Option<WgpuBackend> {
    match WgpuBackend::new(width, height) {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("[skip] no wgpu adapter on this host: {e}");
            None
        }
    }
}

#[test]
fn renders_aria_to_nontrivial_pixels() {
    let model_path = skip_if_no_aria!();
    let Some(mut backend) = try_init_backend(1024, 1024) else {
        return;
    };
    backend
        .load_model(&model_path)
        .expect("load Aria into wgpu backend");

    let pose = Pose::default(); // neutral pose, no overrides
    let frame = backend.render_frame(&pose).expect("render");

    assert_eq!(frame.width, 1024);
    assert_eq!(frame.height, 1024);
    assert_eq!(frame.pixels.len(), 1024 * 1024 * 3);

    let nonzero = frame.nonzero_byte_count();
    assert!(
        nonzero > 1000,
        "expected non-trivial pixel coverage from Aria render; \
         got only {nonzero} non-zero bytes"
    );
    eprintln!(
        "Aria neutral render: {nonzero} non-zero bytes \
         ({:.2}% of frame)",
        nonzero as f64 * 100.0 / frame.pixels.len() as f64,
    );

    // Save the rendered frame to disk so a developer can eyeball
    // it. PNG encoding via the `image` crate (already pulled in
    // via avatar-render-wgpu).
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .parent()
        .map(|p| p.join("avatar-render-tests"))
        .unwrap_or_else(|| PathBuf::from("target/avatar-render-tests"));
    std::fs::create_dir_all(&out_dir).ok();
    let out_path = out_dir.join("aria_neutral.png");
    let buf = image::RgbImage::from_raw(frame.width, frame.height, frame.pixels.clone())
        .expect("RgbImage from raw");
    if let Err(e) = buf.save(&out_path) {
        eprintln!("[warn] failed to save render preview {:?}: {e}", out_path);
    } else {
        eprintln!("✅ saved render preview: {}", out_path.display());
    }
}

/// As above but runs in a tokio multi-thread runtime to mirror the
/// streaming pipeline's execution context. If pixels go static here
/// while the sync version produces distinct frames, the bug is
/// somewhere in the async/wgpu interaction (e.g. `device.poll(Wait)`
/// behavior under a busy tokio runtime, or task-scheduling
/// interleaving).
#[test]
fn render_loop_under_tokio_emits_distinct_pixels_per_varying_pose() {
    fn fnv1a_64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    let model_path = skip_if_no_aria!();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("tokio rt");

    let result = rt.block_on(async move {
        let Some(mut backend) = try_init_backend(256, 256) else {
            return None;
        };
        backend.load_model(&model_path).expect("load Aria");
        let values: [f32; 8] = [0.0, 0.2, 0.5, 0.8, 1.0, 0.5, 0.0, 0.7];
        let mut hashes = Vec::new();
        for &v in &values {
            // Yield between each render so other tokio tasks can run,
            // mimicking the ticker_loop's `interval.tick().await`.
            tokio::task::yield_now().await;
            let mut pose = Pose::default();
            pose.params.insert("ParamJawOpen".to_string(), v);
            pose.params.insert("ParamMouthOpenY".to_string(), v);
            let frame = backend.render_frame(&pose).expect("render");
            let h = fnv1a_64(&frame.pixels);
            eprintln!("[tokio loop] ParamJawOpen={:.2} hash={:#018x}", v, h);
            hashes.push(h);
        }
        Some(hashes)
    });
    let Some(hashes) = result else { return };
    let unique: std::collections::HashSet<_> = hashes.iter().collect();
    assert!(
        unique.len() >= 4,
        "expected at least 4 distinct hashes under tokio runtime, got {}",
        unique.len()
    );
}

/// Regression test: render a tight loop of varying poses and confirm
/// each `RgbFrame.pixels` actually reflects the pose change. The
/// streaming pipeline was emitting renders where the SDK clearly
/// updated vertex positions per call but the readback returned
/// byte-identical pixels — pinpoints whether the bug lives in the
/// backend's render→readback path or further downstream.
#[test]
fn render_loop_emits_distinct_pixels_per_varying_pose() {
    fn fnv1a_64(bytes: &[u8]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    let model_path = skip_if_no_aria!();
    let Some(mut backend) = try_init_backend(256, 256) else {
        return;
    };
    backend.load_model(&model_path).expect("load Aria");

    let mut hashes = Vec::new();
    let values: [f32; 8] = [0.0, 0.2, 0.5, 0.8, 1.0, 0.5, 0.0, 0.7];
    for &v in &values {
        let mut pose = Pose::default();
        pose.params.insert("ParamJawOpen".to_string(), v);
        pose.params.insert("ParamMouthOpenY".to_string(), v);
        let frame = backend.render_frame(&pose).expect("render");
        let h = fnv1a_64(&frame.pixels);
        eprintln!("[loop] ParamJawOpen={:.2} hash={:#018x}", v, h);
        hashes.push(h);
    }
    let unique: std::collections::HashSet<_> = hashes.iter().collect();
    assert!(
        unique.len() >= 4,
        "expected at least 4 distinct pixel hashes across {} varied poses, got {}",
        values.len(),
        unique.len()
    );
}

#[test]
fn renders_aria_with_open_jaw() {
    // Drive ParamJawOpen to the high end of its range and confirm
    // the rendered frame differs from the neutral pose. Pinpoints
    // the param-set→csmUpdateModel→VB upload→render path
    // end-to-end.
    let model_path = skip_if_no_aria!();
    let Some(mut backend) = try_init_backend(512, 512) else {
        return;
    };
    backend.load_model(&model_path).expect("load Aria");

    let neutral = backend.render_frame(&Pose::default()).expect("neutral");

    let mut open_pose = Pose::default();
    open_pose.params.insert("ParamJawOpen".to_string(), 1.0);
    open_pose.params.insert("ParamMouthOpenY".to_string(), 1.0);
    let open = backend.render_frame(&open_pose).expect("jaw open");

    // The two frames must differ — a stuck pose pipeline returning
    // the same image regardless of params would slip past the
    // earlier "non-zero pixel" assertion.
    let differing = neutral
        .pixels
        .iter()
        .zip(&open.pixels)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        differing > 100,
        "open-jaw pose should differ from neutral; got only \
         {differing} differing bytes"
    );
    eprintln!(
        "open-jaw vs neutral: {differing} differing bytes \
         ({:.3}% of frame)",
        differing as f64 * 100.0 / neutral.pixels.len() as f64,
    );
}

#[test]
fn renders_through_state_machine_to_pixels() {
    // Full M4.3 + M4.4 wiring: state machine drives blendshapes,
    // backend renders the resulting pose. Asserts the seam works
    // end-to-end with real GPU pixels.
    let model_path = skip_if_no_aria!();
    let Some(mut backend) = try_init_backend(512, 512) else {
        return;
    };
    backend.load_model(&model_path).expect("load Aria");

    let mut state = Live2DRenderState::new(StateConfig::default_config());
    state.push_blendshape(remotemedia_core::nodes::lip_sync::BlendshapeFrame::new(
        {
            let mut a = [0.0f32; 52];
            a[17] = 0.8; // jawOpen
            a
        },
        100,
        None,
    ));
    state.update_audio_clock(100);

    let pose = state.compute_pose();
    let frame = backend.render_frame(&pose).expect("render");
    assert!(frame.nonzero_byte_count() > 1000);
}
