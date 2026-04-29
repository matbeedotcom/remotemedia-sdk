//! M4.6 pass 1 — avatar pipeline end-to-end (CI-runnable).
//!
//! Channel-wires the three avatar-specific Rust nodes
//! ([`EmotionExtractorNode`], [`SyntheticLipSyncNode`],
//! [`Live2DRenderNode`] with [`MockBackend`]) into the §4.1 shape:
//!
//! ```text
//! text("[EMOTION:🤩]hi")
//!   → EmotionExtractorNode (multi-output: text + emotion json)
//!       ├─(text)──→ /dev/null  (the TTS slot — see pass 2)
//!       └─(emotion json)──→ Live2DRenderNode  ─┐
//! audio (synthetic 16 kHz)                      │
//!   → SyntheticLipSyncNode                      │
//!       └─(blendshape json)──→ Live2DRenderNode ┤
//!                                                ├→ Video frames
//!                                                ┘
//! ```
//!
//! Pass 1 substitutes `SyntheticLipSyncNode` for `Audio2FaceLipSyncNode`
//! (no model dep) and `MockBackend` for `WgpuBackend` (no GPU). The
//! pose stream flowing into the backend is the same shape the wgpu
//! renderer will see; tier-2 (M4.4 wgpu test) covers the pixel side.
//!
//! ## What this test pins
//!
//! - `EmotionExtractor` emits multi-output `(Text, Json)` per spec
//!   §3.1; only the Json edge reaches the renderer.
//! - `SyntheticLipSync` emits `BlendshapeFrame` Json per audio chunk,
//!   pts_ms aligned with input duration.
//! - The renderer's state machine receives both stream kinds via its
//!   single input port (`process_streaming` dispatches by `kind`).
//! - The renderer's ticker emits `RuntimeData::Video` at the
//!   configured framerate, regardless of input cadence.
//! - The recorded pose stream reflects both inputs (expression name
//!   from emotion, mouth params from blendshapes).
//!
//! ## What it does NOT cover
//!
//! - **Real TTS** (kokoro). Pass 2 (`avatar_full_pipeline_e2e.rs`,
//!   `#[ignore]`) wires the canonical §4.1 chain with kokoro_tts,
//!   audio resampling, and the WgpuBackend.
//! - **WebRTC video track wire-up**. The renderer's `RuntimeData::Video`
//!   output flows; routing it through a `webrtc-rs` track is M4
//!   transport work that lands once a `VideoSender` exists (parallel
//!   to M1's `AudioSender`).

#![cfg(feature = "avatar-render-test-support")]

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::video::PixelFormat;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::emotion_extractor::EmotionExtractorNode;
use remotemedia_core::nodes::lip_sync::SyntheticLipSyncNode;
use remotemedia_core::nodes::live2d_render::{
    Live2DRenderConfig, Live2DRenderNode, MockBackend,
};
use remotemedia_core::nodes::AsyncStreamingNode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Synth a short 16 kHz audio chunk so the SyntheticLipSyncNode has
/// something to derive blendshapes from. RMS-driven mouth: a louder
/// chunk → wider jaw open in the resulting BlendshapeFrame.
fn audio_chunk_loud(ms: u64) -> RuntimeData {
    let samples = (16_000 * ms as usize) / 1000;
    // Half-amplitude sine — RMS ≈ 0.354.
    let pcm: Vec<f32> = (0..samples)
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

/// Drive any AsyncStreamingNode with one input + collect every
/// output via a callback. Convenience for the ad-hoc orchestration
/// this e2e test does (no SessionRouter — see test's docstring).
async fn drive<N: AsyncStreamingNode>(node: &N, input: RuntimeData) -> Vec<RuntimeData> {
    let collected = Arc::new(Mutex::new(Vec::new()));
    let cc = collected.clone();
    node.process_streaming(input, None, move |o| {
        cc.lock().unwrap().push(o);
        Ok(())
    })
    .await
    .expect("process_streaming");
    let v = collected.lock().unwrap().clone();
    v
}

#[tokio::test]
async fn full_avatar_chain_emits_video_with_emotion_and_lipsync() {
    // ── Construct the chain ───────────────────────────────────────
    let emotion = EmotionExtractorNode::with_default_pattern();
    let lipsync = SyntheticLipSyncNode::with_default();
    let backend = MockBackend::new(640, 480);
    let backend_handle = backend.clone();
    let renderer = Live2DRenderNode::new_with_backend(
        Box::new(backend),
        Live2DRenderConfig {
            framerate: 30,
            video_stream_id: "avatar".to_string(),
            ..Live2DRenderConfig::default()
        },
    );

    // ── Drive an emotion-tagged turn ──────────────────────────────
    // EmotionExtractor emits multi-output: 1 Text (tags stripped)
    // + N Json events (one per matched tag). The Text edge would
    // route to TTS in the §4.1 manifest; we drop it here. The Json
    // edge routes to the renderer.
    let emotion_outputs = drive(
        &emotion,
        RuntimeData::Text("[EMOTION:\u{1F929}]hi there".into()),
    )
    .await;

    let json_count: usize = emotion_outputs
        .iter()
        .filter(|o| matches!(o, RuntimeData::Json(_)))
        .count();
    assert!(
        json_count >= 1,
        "EmotionExtractor should emit at least one emotion Json event; \
         got {json_count}"
    );

    for out in emotion_outputs {
        if matches!(out, RuntimeData::Json(_)) {
            let _ = drive(&renderer, out).await;
        }
    }

    // ── Drive synthetic audio through the lip-sync chain ──────────
    // 5 × 100 ms chunks at 16 kHz. SyntheticLipSync emits one
    // BlendshapeFrame per chunk + an audio_clock-style pts_ms.
    for _ in 0..5 {
        let lipsync_out = drive(&lipsync, audio_chunk_loud(100)).await;
        // Lip-sync emits exactly one BlendshapeFrame Json per audio
        // chunk; route it to the renderer alongside an audio clock
        // tick so the state machine samples it.
        for out in lipsync_out {
            // SyntheticLipSync emits Json blendshapes; harness the
            // pts to also fire an audio_clock event so the renderer
            // samples the keyframe.
            if let RuntimeData::Json(ref v) = out {
                if let Some(pts) = v.get("pts_ms").and_then(|p| p.as_u64()) {
                    let _ = drive(
                        &renderer,
                        RuntimeData::Json(serde_json::json!({
                            "kind": "audio_clock",
                            "pts_ms": pts,
                        })),
                    )
                    .await;
                }
            }
            let _ = drive(&renderer, out).await;
        }
    }

    // ── Let the renderer's ticker emit some frames ────────────────
    tokio::time::sleep(Duration::from_millis(700)).await;
    let final_outputs = drive(&renderer, RuntimeData::Text(String::new())).await;

    // ── Assert: Video frames flow ─────────────────────────────────
    let video_frames: Vec<_> = final_outputs
        .iter()
        .filter(|o| matches!(o, RuntimeData::Video { .. }))
        .collect();
    assert!(
        video_frames.len() >= 8,
        "expected ≥8 Video frames in ~0.7s of ticker time; got {}",
        video_frames.len()
    );
    for frame in &video_frames {
        let RuntimeData::Video {
            stream_id, format, width, height, pixel_data, ..
        } = frame else { unreachable!() };
        assert_eq!(stream_id.as_deref(), Some("avatar"));
        assert_eq!(*format, PixelFormat::Rgb24);
        assert_eq!((*width, *height), (640, 480));
        assert_eq!(pixel_data.len(), 640 * 480 * 3);
    }

    // ── Assert: backend saw both kinds of pose modulation ────────
    let recorded = backend_handle.recorded();
    assert!(
        recorded.iter().any(|r| r.pose.expression_id == "excited_star"),
        "renderer's recorded pose stream should reflect the emotion \
         input (expression_id = \"excited_star\"); observed: {:?}",
        recorded
            .iter()
            .map(|r| r.pose.expression_id.clone())
            .collect::<std::collections::HashSet<_>>()
    );
    let max_jaw = recorded
        .iter()
        .map(|r| r.pose.mouth_value("ParamJawOpen"))
        .fold(0.0f32, f32::max);
    assert!(
        max_jaw > 0.3,
        "renderer's recorded pose stream should reflect the lip-sync \
         input (max ParamJawOpen > 0.3); got {max_jaw}"
    );
    eprintln!(
        "✅ avatar e2e: {} Video frames, max ParamJawOpen={:.3}, \
         expressions: {:?}",
        video_frames.len(),
        max_jaw,
        recorded
            .iter()
            .map(|r| r.pose.expression_id.clone())
            .collect::<std::collections::HashSet<_>>()
    );
}

/// Emotion-only path: text without lip-sync produces an emotion-driven
/// pose stream + neutral mouth.
#[tokio::test]
async fn emotion_only_path_yields_neutral_mouth() {
    let emotion = EmotionExtractorNode::with_default_pattern();
    let backend = MockBackend::new(64, 64);
    let backend_handle = backend.clone();
    let renderer = Live2DRenderNode::new_with_backend(
        Box::new(backend),
        Live2DRenderConfig {
            framerate: 30,
            video_stream_id: "avatar".into(),
            ..Default::default()
        },
    );

    let emotion_outs = drive(&emotion, RuntimeData::Text("[EMOTION:\u{1F622}]oh".into())).await;
    for out in emotion_outs {
        if matches!(out, RuntimeData::Json(_)) {
            let _ = drive(&renderer, out).await;
        }
    }

    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = drive(&renderer, RuntimeData::Text(String::new())).await;

    let recorded = backend_handle.recorded();
    assert!(!recorded.is_empty());
    // 😢 → "sad" + "Sad" per default emotion mapping.
    assert!(
        recorded.iter().any(|r| r.pose.expression_id == "sad"),
        "expected sad expression"
    );
    // No lip-sync inputs → mouth stays at neutral throughout.
    let max_jaw = recorded
        .iter()
        .map(|r| r.pose.mouth_value("ParamJawOpen"))
        .fold(0.0f32, f32::max);
    assert!(
        max_jaw < 0.05,
        "without lip-sync, mouth should stay neutral; got max ParamJawOpen={max_jaw}"
    );
}

/// Lip-sync-only path: audio without emotion produces a pose stream
/// with mouth movement and neutral expression.
#[tokio::test]
async fn lipsync_only_path_yields_neutral_expression() {
    let lipsync = SyntheticLipSyncNode::with_default();
    let backend = MockBackend::new(64, 64);
    let backend_handle = backend.clone();
    let renderer = Live2DRenderNode::new_with_backend(
        Box::new(backend),
        Live2DRenderConfig {
            framerate: 30,
            video_stream_id: "avatar".into(),
            ..Default::default()
        },
    );

    for _ in 0..3 {
        let outs = drive(&lipsync, audio_chunk_loud(100)).await;
        for out in outs {
            if let RuntimeData::Json(ref v) = out {
                if let Some(pts) = v.get("pts_ms").and_then(|p| p.as_u64()) {
                    let _ = drive(
                        &renderer,
                        RuntimeData::Json(serde_json::json!({
                            "kind": "audio_clock", "pts_ms": pts,
                        })),
                    )
                    .await;
                }
            }
            let _ = drive(&renderer, out).await;
        }
    }

    tokio::time::sleep(Duration::from_millis(400)).await;
    let _ = drive(&renderer, RuntimeData::Text(String::new())).await;

    let recorded = backend_handle.recorded();
    assert!(!recorded.is_empty());
    let max_jaw = recorded
        .iter()
        .map(|r| r.pose.mouth_value("ParamJawOpen"))
        .fold(0.0f32, f32::max);
    assert!(
        max_jaw > 0.3,
        "expected mouth movement from lip-sync; got max ParamJawOpen={max_jaw}"
    );
    // Without emotion events, expression stays neutral.
    assert!(
        recorded.iter().all(|r| r.pose.expression_id == "neutral"),
        "without emotion events, every pose should be neutral; \
         observed: {:?}",
        recorded
            .iter()
            .map(|r| r.pose.expression_id.clone())
            .collect::<std::collections::HashSet<_>>()
    );
}

/// Multi-emotion text exercises EmotionExtractor's multi-output path
/// + the renderer's emotion expiration logic. The expression should
/// reflect the *last* emoji in the sequence (current emotion overrides
/// previous when a new event arrives within the hold window).
#[tokio::test]
async fn multi_emotion_sequence_picks_latest() {
    let emotion = EmotionExtractorNode::with_default_pattern();
    let backend = MockBackend::new(64, 64);
    let backend_handle = backend.clone();
    let renderer = Live2DRenderNode::new_with_backend(
        Box::new(backend),
        Live2DRenderConfig::default(),
    );

    // 🤩 then 😊 in the same turn — extractor emits both Json events.
    let outs = drive(
        &emotion,
        RuntimeData::Text("[EMOTION:\u{1F929}]hi[EMOTION:\u{1F60A}]bye".into()),
    )
    .await;
    let json_count: usize = outs
        .iter()
        .filter(|o| matches!(o, RuntimeData::Json(_)))
        .count();
    assert_eq!(json_count, 2, "should emit both emotion events");
    for out in outs {
        if matches!(out, RuntimeData::Json(_)) {
            let _ = drive(&renderer, out).await;
        }
    }

    tokio::time::sleep(Duration::from_millis(300)).await;
    let _ = drive(&renderer, RuntimeData::Text(String::new())).await;

    // The latest pose should reflect happy (the second emotion).
    let recorded = backend_handle.recorded();
    let last = recorded.last().expect("at least one pose");
    assert_eq!(last.pose.expression_id, "happy");
    assert_eq!(last.pose.motion_group, "Happy");
}
