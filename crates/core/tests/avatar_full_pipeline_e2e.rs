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
//! ## Prerequisites
//!
//! 1. `LIVE2D_CUBISM_CORE_DIR` (build-time, gated by
//!    `cubism-core-sys` build script).
//! 2. `LIVE2D_TEST_MODEL_PATH` (runtime — Aria model).
//! 3. `AUDIO2FACE_TEST_BUNDLE` (runtime — persona-engine bundle).
//!
//! Python kokoro deps install **automatically** via the
//! `PYTHON_ENV_MODE=managed` path (matches the qwen example). The
//! test sets `PYTHON_ENV_MODE=managed` + `PYTHON_VERSION=3.12` +
//! `REMOTEMEDIA_PYTHON_SRC` if not already set, so first-run takes
//! several minutes (PyTorch + Kokoro voice pack downloads), then
//! subsequent runs reuse the cached venv.
//!
//! Skips cleanly when `LIVE2D_TEST_MODEL_PATH` /
//! `AUDIO2FACE_TEST_BUNDLE` aren't on disk.
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

// Force-link `remotemedia-python-nodes` so its `inventory::submit!`
// macros register the Python node factories (KokoroTTSNode, …) into
// the default streaming registry. Same trick as
// `qwen_s2s_webrtc_server.rs::_python_nodes_link`.
#[allow(unused_imports)]
use remotemedia_python_nodes as _python_nodes_link;

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

/// Set up the managed-venv Python defaults the multiprocess
/// executor consults. Mirrors `default_python_env()` in
/// [`qwen_s2s_webrtc_server.rs`].
fn ensure_managed_python_env() {
    let ensure = |k: &str, v: &str| {
        if std::env::var_os(k).is_none() {
            std::env::set_var(k, v);
        }
    };
    ensure("PYTHON_ENV_MODE", "managed");
    ensure("PYTHON_VERSION", "3.12");

    if std::env::var_os("REMOTEMEDIA_PYTHON_SRC").is_none() {
        // CARGO_MANIFEST_DIR is `crates/core`; clients/python lives
        // two levels up at `<workspace>/clients/python`.
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidate = manifest_dir
            .ancestors()
            .nth(2)
            .map(|root| root.join("clients").join("python"));
        if let Some(path) = candidate {
            if path.exists() {
                std::env::set_var("REMOTEMEDIA_PYTHON_SRC", path);
            }
        }
    }
}

/// **Canonical §4.1 full e2e.**
///
/// Builds the §4.1 manifest using the qwen-example pattern (Kokoro
/// + FastResample + Audio2Face + Live2D), spins up `SessionRouter`
/// under `PYTHON_ENV_MODE=managed`, drives one text turn, and
/// asserts Video frames flow with the configured `stream_id`.
///
/// First run downloads PyTorch + the Kokoro voice pack into the
/// managed venv (multi-minute). Subsequent runs reuse the cache.
/// Skips when `LIVE2D_TEST_MODEL_PATH` / `AUDIO2FACE_TEST_BUNDLE`
/// aren't set.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
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

    // Set the managed-venv defaults so KokoroTTSNode + the Python
    // multiprocess executor know where to look. Mirrors the qwen
    // example's `default_python_env()`.
    ensure_managed_python_env();

    let bundle = std::env::var("AUDIO2FACE_TEST_BUNDLE").unwrap();
    let model = std::env::var("LIVE2D_TEST_MODEL_PATH").unwrap();
    let manifest = Arc::new(build_full_avatar_manifest(&bundle, &model));

    // Sanity: the manifest declares all 5 nodes + 5 connections.
    assert_eq!(manifest.nodes.len(), 5);
    assert_eq!(manifest.connections.len(), 5);

    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) =
        mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    eprintln!(
        "[info] building SessionRouter — first run downloads kokoro \
         + PyTorch into the managed venv (multi-minute)…"
    );
    let (router, _shutdown_tx) = SessionRouter::new(
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

    // Collect outputs for 60 seconds — covers (a) first-run kokoro
    // venv setup + voice pack download, (b) Python subprocess
    // boot, (c) TTS synthesis of "Hello there!", (d) Audio2Face's
    // ~3.6s cold inference, (e) renderer's 30 fps emit. Subsequent
    // runs reuse the venv cache and finish in seconds.
    let mut video_frames: Vec<RuntimeData> = Vec::new();
    let mut first_video_at: Option<std::time::Instant> = None;
    let started = std::time::Instant::now();
    let collect_until = started + Duration::from_secs(60);
    while std::time::Instant::now() < collect_until {
        if let Ok(Some(out)) =
            tokio::time::timeout(Duration::from_millis(200), output_rx.recv()).await
        {
            if matches!(out, RuntimeData::Video { .. }) {
                if first_video_at.is_none() {
                    first_video_at = Some(std::time::Instant::now());
                    eprintln!(
                        "[info] first Video frame at {:.1}s",
                        started.elapsed().as_secs_f32()
                    );
                }
                video_frames.push(out);
                // Once we've got ≥30 frames, we've proven the chain works
                // end-to-end — bail to keep the test fast.
                if video_frames.len() >= 30 {
                    break;
                }
            }
        }
    }

    eprintln!(
        "[info] collected {} Video frames in {:.1}s",
        video_frames.len(),
        started.elapsed().as_secs_f32()
    );
    assert!(
        video_frames.len() >= 30,
        "expected ≥30 Video frames; got {} (first frame at {:?})",
        video_frames.len(),
        first_video_at.map(|t| t.duration_since(started))
    );
    for f in &video_frames {
        if let RuntimeData::Video { stream_id, .. } = f {
            assert_eq!(stream_id.as_deref(), Some("avatar"));
        }
    }
    // Mid-stream pixel sanity: a frame ~halfway in should have
    // non-trivial coverage from the rendered Aria.
    let mid_idx = video_frames.len() / 2;
    if let Some(RuntimeData::Video { pixel_data, .. }) = video_frames.get(mid_idx) {
        let nonzero = pixel_data.iter().filter(|&&b| b != 0).count();
        assert!(
            nonzero > 1000,
            "mid-stream frame should have non-trivial pixels; got {nonzero} non-zero bytes"
        );
        eprintln!("[info] mid-stream pixel coverage: {nonzero} non-zero bytes");
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
