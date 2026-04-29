//! M4.6 pass 2 — canonical §4.1 full avatar pipeline.
//!
//! Wires the complete spec §4.1 manifest end-to-end:
//!
//! ```text
//! text("[EMOTION:🤩]hi")
//!   → EmotionExtractorNode (Rust)
//!       ├─(text)──→ KokoroTTSNode (Python multiprocess @ 24 kHz)
//!       │              └─(audio)──→ FastResampleNode (24 kHz → 16 kHz)
//!       │                              └─(audio)──→ Audio2FaceLipSyncNode
//!       │                                              └─(blendshapes)─┐
//!       └─(emotion json)────────────────────────────────────────────────┤
//!                                                                       ├→ Live2DRenderNode (Wgpu/Aria)
//!                                                                       ┘    └→ RuntimeData::Video
//! ```
//!
//! Manifest construction follows the pattern in
//! [`qwen_s2s_webrtc_server.rs`](../../../crates/transports/webrtc/examples/qwen_s2s_webrtc_server.rs):
//! `KokoroTTSNode` is declared with inline `python_deps` so the
//! managed-venv multiprocess executor installs the right packages
//! at session-start. `FastResampleNode` bridges the 24 kHz TTS
//! output to Audio2Face's 16 kHz input.
//!
//! ## Status
//!
//! **`#[ignore]` by default.** This test requires a fully-set-up
//! dev environment:
//!
//! 1. `LIVE2D_CUBISM_CORE_DIR` (build-time, gated by
//!    `cubism-core-sys` build script).
//! 2. `LIVE2D_TEST_MODEL_PATH` (runtime — Aria model).
//! 3. `AUDIO2FACE_TEST_BUNDLE` (runtime — persona-engine bundle).
//! 4. **Python interpreter** discoverable by the multiprocess
//!    executor (managed mode handles the venv + `kokoro>=0.9.4`
//!    install).
//! 5. **`KokoroTTSNode` Python multiprocess infrastructure**: the
//!    managed-venv path resolves `kokoro>=0.9.4` + dependencies on
//!    first run, then boots a Python subprocess. First-run cost is
//!    significant (multi-minute). Subsequent runs reuse the venv.
//!
//! Audio2FaceLipSyncNode + Live2DRenderNode factories now ship in
//! `core_provider.rs` (M4.6 follow-up); the e2e wires through the
//! same factory path that [`avatar_factory_pipeline_test.rs`]
//! validates without TTS.
//!
//! Run via:
//!
//! ```bash
//! export LIVE2D_CUBISM_CORE_DIR=$PWD/sdk/CubismSdkForNative-5-r.5
//! export LIVE2D_TEST_MODEL_PATH=$PWD/models/live2d/aria/aria.model3.json
//! export AUDIO2FACE_TEST_BUNDLE=$PWD/models/audio2face
//! cargo test -p remotemedia-core --features avatar-render-wgpu \
//!     --test avatar_full_pipeline_e2e -- --ignored --nocapture
//! ```
//!
//! ## Why `#[ignore]` instead of running on every CI invocation
//!
//! Each prerequisite is heavyweight + per-developer:
//!
//! - The Cubism SDK is a license-gated download.
//! - Aria (~46 MB) is a manual install.
//! - The Audio2Face bundle (~632 MB) is a manual install.
//! - `kokoro>=0.9.4` pulls PyTorch + a ~300 MB voice pack.
//!
//! The simpler [`avatar_pipeline_e2e.rs`] (M4.6 pass 1) covers the
//! avatar-specific integration our code owns; this file is the
//! integration verification you run once locally to confirm
//! everything actually composes when the upstream pieces are
//! present.

#![cfg(feature = "avatar-render-wgpu")]

use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use std::path::PathBuf;
use std::sync::Arc;

/// Build the canonical §4.1 avatar manifest. Mirrors the qwen
/// example's `KokoroTTSNode` + `FastResampleNode` shape so the
/// multiprocess executor can boot the TTS Python subprocess via
/// the managed venv path.
fn build_full_avatar_manifest(
    audio2face_bundle_path: &str,
    live2d_model_path: &str,
) -> Manifest {
    let kokoro_deps = vec![
        "kokoro>=0.9.4".to_string(),
        "soundfile".to_string(),
        "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl".to_string(),
    ];

    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "avatar-full-e2e".to_string(),
            ..Default::default()
        },
        nodes: vec![
            NodeManifest {
                id: "emotion_extractor".to_string(),
                node_type: "EmotionExtractorNode".to_string(),
                params: serde_json::json!({}),
                ..Default::default()
            },
            NodeManifest {
                id: "kokoro_tts".to_string(),
                node_type: "KokoroTTSNode".to_string(),
                params: serde_json::json!({
                    "lang_code": "a",
                    "voice": "af_heart",
                    "speed": 1.0,
                    "sample_rate": 24000,
                    "stream_chunks": true,
                    "skip_tokens": ["<|text_end|>", "```"],
                }),
                python_deps: Some(kokoro_deps),
                ..Default::default()
            },
            NodeManifest {
                id: "audio_resample".to_string(),
                node_type: "FastResampleNode".to_string(),
                params: serde_json::json!({
                    "source_rate": 24000,
                    "target_rate": 16000,
                    "quality": "Medium",
                    "channels": 1,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "audio2face_lipsync".to_string(),
                node_type: "Audio2FaceLipSyncNode".to_string(),
                params: serde_json::json!({
                    "bundle_path": audio2face_bundle_path,
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
                    "model_path": live2d_model_path,
                    "framerate": 30,
                    "video_stream_id": "avatar",
                }),
                ..Default::default()
            },
        ],
        connections: vec![
            // Text branch: emotion's text channel → TTS → resample → audio2face → render
            Connection {
                from: "emotion_extractor".to_string(),
                to: "kokoro_tts".to_string(),
            },
            Connection {
                from: "kokoro_tts".to_string(),
                to: "audio_resample".to_string(),
            },
            Connection {
                from: "audio_resample".to_string(),
                to: "audio2face_lipsync".to_string(),
            },
            Connection {
                from: "audio2face_lipsync".to_string(),
                to: "live2d_render".to_string(),
            },
            // Json branch: emotion events fan to render alongside blendshapes
            Connection {
                from: "emotion_extractor".to_string(),
                to: "live2d_render".to_string(),
            },
        ],
        python_env: None,
    }
}

fn check_env() -> Option<&'static str> {
    if std::env::var("LIVE2D_TEST_MODEL_PATH").is_err() {
        return Some("LIVE2D_TEST_MODEL_PATH");
    }
    if std::env::var("AUDIO2FACE_TEST_BUNDLE").is_err() {
        return Some("AUDIO2FACE_TEST_BUNDLE");
    }
    None
}

/// **Canonical §4.1 full e2e — `#[ignore]` by default.**
///
/// Builds the §4.1 manifest using the qwen-example pattern (Kokoro
/// + FastResample + Audio2Face + Live2D), spins up `SessionRouter`,
/// drives one text turn, and asserts Video frames flow with the
/// configured `stream_id`.
///
/// All four factories (`EmotionExtractor`, `KokoroTTS`,
/// `FastResample`, `Audio2FaceLipSync`, `Live2DRender`) are
/// registered in `core_provider.rs`; the only thing gating this
/// test is the runtime Python kokoro venv + the heavy model
/// downloads.
#[tokio::test]
#[ignore = "requires kokoro venv + Aria + Audio2Face + Cubism SDK"]
async fn full_avatar_pipeline_emits_video_track_with_emotion_and_lipsync() {
    use remotemedia_core::data::RuntimeData;
    use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
    use remotemedia_core::transport::session_router::{
        DataPacket, SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
    };
    use std::time::Duration;
    use tokio::sync::mpsc;

    if let Some(missing) = check_env() {
        eprintln!(
            "[skip] {missing} not set — see avatar_full_pipeline_e2e.rs \
             docstring for required env vars"
        );
        return;
    }

    let bundle = std::env::var("AUDIO2FACE_TEST_BUNDLE").unwrap();
    let model = std::env::var("LIVE2D_TEST_MODEL_PATH").unwrap();
    let manifest = Arc::new(build_full_avatar_manifest(&bundle, &model));

    // Sanity: the manifest declares all 5 nodes + 5 connections.
    assert_eq!(manifest.nodes.len(), 5);
    assert_eq!(manifest.connections.len(), 5);

    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) =
        mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    // SessionRouter::new instantiates every node via its registered
    // factory. With M4.6 follow-up's factory registrations in place
    // this should succeed when the env vars + model files are
    // present; only the kokoro_tts subprocess boot is left as a
    // runtime cost.
    let (mut router, _shutdown_tx) = SessionRouter::new(
        "avatar-e2e".to_string(),
        manifest.clone(),
        registry,
        output_tx,
    )
    .expect("SessionRouter::new");

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Drive one emotion-tagged text turn.
    let _ = input_tx
        .send(DataPacket {
            data: RuntimeData::Text("[EMOTION:\u{1F929}]Hello there!".to_string()),
            from_node: "client".to_string(),
            to_node: None,
            session_id: "avatar-e2e".to_string(),
            sequence: 0,
            sub_sequence: 0,
        })
        .await;

    // Collect outputs for 2 seconds. At 30 fps this should yield
    // ≥50 Video frames per spec §M4.6.
    let mut video_frames: Vec<RuntimeData> = Vec::new();
    let collect_until = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < collect_until {
        if let Ok(Some(out)) =
            tokio::time::timeout(Duration::from_millis(100), output_rx.recv()).await
        {
            if matches!(out, RuntimeData::Video { .. }) {
                video_frames.push(out);
            }
        }
    }

    assert!(
        video_frames.len() >= 50,
        "expected ≥50 Video frames in 2s; got {}",
        video_frames.len()
    );
    for f in &video_frames {
        if let RuntimeData::Video { stream_id, .. } = f {
            assert_eq!(stream_id.as_deref(), Some("avatar"));
        }
    }
    // Mid-stream pixel sanity: a frame ~1s in should have non-trivial coverage.
    if let Some(RuntimeData::Video { pixel_data, .. }) = video_frames.get(30) {
        let nonzero = pixel_data.iter().filter(|&&b| b != 0).count();
        assert!(nonzero > 1000, "mid-stream frame should have non-trivial pixels");
    }

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

/// Smoke (always runs): pin the canonical manifest's structural
/// shape so a typo here doesn't hide until someone enables the
/// gated test. Catches changes to node ids, types, or connections.
#[test]
fn manifest_structure_pins_section_4_1_shape() {
    let m = build_full_avatar_manifest(
        "/path/to/audio2face",
        "/path/to/aria.model3.json",
    );
    assert_eq!(m.version, "v1");
    assert_eq!(m.metadata.name, "avatar-full-e2e");
    assert_eq!(m.nodes.len(), 5);
    assert_eq!(m.connections.len(), 5);

    let node_types: Vec<&str> = m.nodes.iter().map(|n| n.node_type.as_str()).collect();
    assert_eq!(
        node_types,
        vec![
            "EmotionExtractorNode",
            "KokoroTTSNode",
            "FastResampleNode",
            "Audio2FaceLipSyncNode",
            "Live2DRenderNode",
        ]
    );

    // Kokoro carries python_deps so the multiprocess executor's
    // managed-venv path resolves them at session start (matches
    // qwen_s2s_webrtc_server.rs pattern).
    let kokoro = m.nodes.iter().find(|n| n.id == "kokoro_tts").unwrap();
    let deps = kokoro.python_deps.as_ref().expect("kokoro_tts python_deps");
    assert!(deps.iter().any(|d| d.starts_with("kokoro")));
    assert!(deps.iter().any(|d| d.starts_with("soundfile")));

    // Connections trace the §4.1 graph: text branch fans through
    // tts/resample/audio2face/render; json branch direct to render.
    let froms: Vec<&str> = m.connections.iter().map(|c| c.from.as_str()).collect();
    let tos: Vec<&str> = m.connections.iter().map(|c| c.to.as_str()).collect();
    assert!(froms.contains(&"kokoro_tts") && tos.contains(&"audio_resample"));
    assert!(froms.contains(&"audio2face_lipsync") && tos.contains(&"live2d_render"));
    assert!(froms.contains(&"emotion_extractor") && tos.contains(&"live2d_render"));
}

#[allow(dead_code)]
fn _suppress_unused_path(_: PathBuf) {}
