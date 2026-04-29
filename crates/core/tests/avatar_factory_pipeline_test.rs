//! M4.6 follow-up — factory-wired SessionRouter pipeline (no TTS).
//!
//! Proves the `Audio2FaceLipSyncNodeFactory` + `Live2DRenderNodeFactory`
//! that just landed in `core_provider.rs` actually instantiate
//! correctly through `SessionRouter::new`, AND that synthetic audio
//! pushed into the router emerges as `RuntimeData::Video` frames
//! out the other side.
//!
//! This is the **closest-to-§4.1 e2e we can run on a developer
//! machine without the Python kokoro venv**:
//!
//! ```text
//!   synthetic 16 kHz audio (test fixture)
//!     → Audio2FaceLipSyncNode (real ONNX bundle)
//!         └─→ Live2DRenderNode (WgpuBackend rendering Aria)
//!                └→ RuntimeData::Video
//! ```
//!
//! Once kokoro_tts is in the picture, [`avatar_full_pipeline_e2e.rs`]
//! adds the TTS leg in front; this file pins everything from
//! Audio2Face onward.
//!
//! Gated on:
//! - `LIVE2D_CUBISM_CORE_DIR` (build-time) — unpacked Cubism SDK for Native dir
//! - `LIVE2D_TEST_MODEL_PATH` (runtime) — path to **`.model3.json`** (NOT `.moc3`).
//!   Pointing this at the `.moc3` binary makes the JSON parser barf at byte 0
//!   and the wgpu backend fails to load — which silently surfaces as
//!   "0 video frames" because `SessionRouter::new` returned an Err that the
//!   test ignores.
//! - `AUDIO2FACE_TEST_BUNDLE` (runtime) — directory containing
//!   `network.onnx` + `bs_skin_<Identity>.npz` + `model_data_<Identity>.npz`
//!   + `bs_skin_config_<Identity>.json` + `model_config_<Identity>.json`.

#![cfg(feature = "avatar-render-wgpu")]

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use remotemedia_core::transport::session_router::{
    DataPacket, SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Skip-helper — bails the test cleanly when the env vars or models
/// aren't on disk.
fn check_prereqs() -> Option<(String, String)> {
    let bundle = std::env::var("AUDIO2FACE_TEST_BUNDLE").ok()?;
    let model = std::env::var("LIVE2D_TEST_MODEL_PATH").ok()?;
    if !std::path::Path::new(&bundle).exists() {
        return None;
    }
    if !std::path::Path::new(&model).exists() {
        return None;
    }
    Some((bundle, model))
}

/// 1 second of half-amplitude 440 Hz sine at 16 kHz mono — the
/// sample rate Audio2FaceLipSyncNode requires.
fn one_second_audio() -> RuntimeData {
    let n = 16_000;
    let pcm: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin())
        .collect();
    RuntimeData::Audio {
        samples: AudioSamples::Vec(pcm),
        sample_rate: 16_000,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

fn build_factory_pipeline_manifest(bundle_path: &str, model_path: &str) -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "avatar-factory-pipeline".to_string(),
            ..Default::default()
        },
        nodes: vec![
            NodeManifest {
                id: "audio2face_lipsync".to_string(),
                node_type: "Audio2FaceLipSyncNode".to_string(),
                params: serde_json::json!({
                    "bundle_path": bundle_path,
                    "identity": "Claire",
                    "solver": "pgd",
                    "use_gpu": false,
                    "smoothing_alpha": 0.0,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "live2d_render".to_string(),
                node_type: "Live2DRenderNode".to_string(),
                params: serde_json::json!({
                    "model_path": model_path,
                    "framerate": 30,
                    "video_stream_id": "avatar",
                    // Smaller frame size keeps the test fast — the
                    // wgpu backend works at any size; M4.4 tier-2
                    // already covers 1024².
                    "width": 256,
                    "height": 256,
                }),
                ..Default::default()
            },
        ],
        connections: vec![Connection {
            from: "audio2face_lipsync".to_string(),
            to: "live2d_render".to_string(),
        }],
        python_env: None,
    }
}

/// Diagnostic — Audio2Face standalone through SessionRouter (as
/// a sink). Confirms whether blendshape Json envelopes flow when
/// the node is in router context.
#[tokio::test]
async fn audio2face_alone_through_router_emits_blendshapes() {
    let bundle = match std::env::var("AUDIO2FACE_TEST_BUNDLE") {
        Ok(b) if std::path::Path::new(&b).exists() => b,
        _ => {
            eprintln!("[skip] AUDIO2FACE_TEST_BUNDLE not set");
            return;
        }
    };
    let manifest = Arc::new(Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "a2f-only".to_string(),
            ..Default::default()
        },
        nodes: vec![NodeManifest {
            id: "audio2face_lipsync".to_string(),
            node_type: "Audio2FaceLipSyncNode".to_string(),
            params: serde_json::json!({
                "bundle_path": bundle,
                "identity": "Claire",
                "solver": "pgd",
                "use_gpu": false,
                "smoothing_alpha": 0.0,
            }),
            ..Default::default()
        }],
        connections: vec![],
        python_env: None,
    });
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);
    let (router, _shutdown) = SessionRouter::new(
        "a2f-only".to_string(),
        manifest,
        registry,
        output_tx,
    )
    .expect("SessionRouter::new");
    let input_tx = router.get_input_sender();
    let handle = router.start();

    let _ = input_tx
        .send(DataPacket {
            data: one_second_audio(),
            from_node: "test".to_string(),
            to_node: None,
            session_id: "a2f-only".to_string(),
            sequence: 0,
            sub_sequence: 0,
        })
        .await;

    let mut blends = 0;
    let collect_until = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < collect_until {
        if let Ok(Some(out)) =
            tokio::time::timeout(Duration::from_millis(200), output_rx.recv()).await
        {
            if let RuntimeData::Json(v) = &out {
                if v.get("kind").and_then(|k| k.as_str()) == Some("blendshapes") {
                    blends += 1;
                }
            }
        }
    }
    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(
        blends >= 25,
        "audio2face standalone should emit ~30 blendshapes per 1s window; got {blends}"
    );
}

#[tokio::test]
async fn factories_instantiate_through_session_router() {
    let Some((bundle, model)) = check_prereqs() else {
        eprintln!(
            "[skip] AUDIO2FACE_TEST_BUNDLE + LIVE2D_TEST_MODEL_PATH \
             not set; install via scripts/install-{{audio2face,live2d-aria}}.sh"
        );
        return;
    };

    let manifest = Arc::new(build_factory_pipeline_manifest(&bundle, &model));
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut _output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    // SessionRouter::new instantiates each node via its registered
    // factory. This is what was failing in M4.6 before the factory
    // registrations landed; now it should succeed.
    let router_result = SessionRouter::new(
        "avatar-factory-test".to_string(),
        manifest,
        registry,
        output_tx,
    );

    let (router, shutdown_tx) = router_result.expect("SessionRouter::new should succeed");

    // Tear down without driving any inputs — the assertion is just
    // that construction succeeded (heavy: ~3.6s for ONNX load + a
    // few hundred ms for wgpu device init + Aria texture upload).
    let _ = router; // keep alive briefly
    drop(shutdown_tx);
    tokio::time::sleep(Duration::from_millis(50)).await;
    eprintln!("✅ Audio2FaceLipSyncNode + Live2DRenderNode factories construct cleanly");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_factory_pipeline_emits_video_for_audio_input() {
    let Some((bundle, model)) = check_prereqs() else {
        eprintln!(
            "[skip] AUDIO2FACE_TEST_BUNDLE + LIVE2D_TEST_MODEL_PATH \
             not set"
        );
        return;
    };

    let manifest = Arc::new(build_factory_pipeline_manifest(&bundle, &model));
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) =
        mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (router, _shutdown_tx) = SessionRouter::new(
        "avatar-factory-e2e".to_string(),
        manifest,
        registry,
        output_tx,
    )
    .expect("SessionRouter::new");

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Drive 1 second of synthetic audio. Audio2Face buffers 1s
    // windows + emits 30 BlendshapeFrame Json envelopes; each one
    // flushes the renderer's frame queue. Cold ONNX inference is
    // ~3.6s on Apple Silicon CPU; collection window must cover it.
    let _ = input_tx
        .send(DataPacket {
            data: one_second_audio(),
            from_node: "test".to_string(),
            to_node: None,
            session_id: "avatar-factory-e2e".to_string(),
            sequence: 0,
            sub_sequence: 0,
        })
        .await;

    // Collect Video for ~10 seconds. Cold ONNX inference is ~3.6s
    // on Apple Silicon CPU + we need 30 blendshape arrivals to flush
    // the renderer's queued ticker frames.
    //
    // **Why this test requires `multi_thread` runtime**: the
    // Audio2Face ONNX inference is sync (no .await inside ort's
    // run loop). On a single-threaded `#[tokio::test]` runtime,
    // the ~3.6s cold inference blocks every other task — including
    // the renderer's free-running ticker — so the ticker can't
    // fire until inference finishes. Multi-threaded runtime gives
    // the ticker its own worker so it stays at 30 fps regardless
    // of audio2face's CPU work.
    let mut video_frames: Vec<RuntimeData> = Vec::new();
    let collect_until = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < collect_until {
        if let Ok(Some(out)) =
            tokio::time::timeout(Duration::from_millis(100), output_rx.recv()).await
        {
            if matches!(out, RuntimeData::Video { .. }) {
                video_frames.push(out);
            }
        }
    }

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

    assert!(
        video_frames.len() >= 20,
        "expected ≥20 Video frames over ~10s collection; got {}",
        video_frames.len()
    );
    eprintln!("✅ collected {} Video frames", video_frames.len());
    eprintln!("✅ collected {} Video frames", video_frames.len());

    for f in &video_frames {
        if let RuntimeData::Video {
            stream_id, format, width, height, pixel_data, ..
        } = f {
            assert_eq!(stream_id.as_deref(), Some("avatar"));
            assert_eq!(*format, remotemedia_core::data::video::PixelFormat::Rgb24);
            assert_eq!((*width, *height), (256, 256));
            assert_eq!(pixel_data.len(), 256 * 256 * 3);
        }
    }

    // Mid-stream pixel sanity — one frame should have non-trivial
    // coverage. Aria's neutral pose covers ~58% of frame pixels at
    // 1024² (per M4.4 measurements); at 256² with the lipsync-driven
    // pose it should still have well above 1000 non-zero bytes.
    let mid = &video_frames[video_frames.len() / 2];
    if let RuntimeData::Video { pixel_data, .. } = mid {
        let nonzero = pixel_data.iter().filter(|&&b| b != 0).count();
        assert!(nonzero > 1000, "mid-stream frame coverage too low: {nonzero}");
        eprintln!(
            "  mid-stream: {nonzero} non-zero bytes ({:.2}% of {} bytes)",
            nonzero as f64 * 100.0 / pixel_data.len() as f64,
            pixel_data.len()
        );
    }
}
