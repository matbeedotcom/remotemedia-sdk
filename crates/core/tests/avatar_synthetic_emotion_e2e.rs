//! Synthetic-emotion avatar pipeline e2e (avatar plan M2.7).
//!
//! Wires the M0 + M1 + M2 pieces together end-to-end with deterministic
//! synthetic inputs (no LLM, no real Audio2Face ONNX, no Live2D model):
//!
//! ```text
//!   text_source([EMOTION:🤩] hi [EMOTION:😊] bye)
//!     → EmotionExtractorNode
//!         ├─(text)→ (synthesized speech path: a sine fixture stands in
//!         │          for kokoro_tts; lip-sync sees real audio shape)
//!         │       → SyntheticLipSyncNode → blendshape_sink
//!         └─(json) → emotion_sink
//! ```
//!
//! Asserts the spec invariants the renderer (M4) will rely on:
//! - Two emotion Json events arrive with `emoji ∈ {🤩, 😊}` in source order.
//! - A continuous blendshape stream arrives with monotonic `pts_ms`.
//! - The text reaching the (synthetic) TTS path has tags stripped.
//!
//! When the real Audio2FaceLipSyncNode lands, swap the SyntheticLipSync
//! constructor for the real one — same trait, same envelope, same
//! manifest edge — and the assertions stand.

#![cfg(all(feature = "avatar-emotion", feature = "avatar-lipsync"))]

#[path = "avatar_test_support.rs"]
mod support;

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::text_channel::tag_text_str;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::nodes::emotion_extractor::EmotionExtractorNode;
use remotemedia_core::nodes::lip_sync::{
    BlendshapeFrame, SyntheticLipSyncConfig, SyntheticLipSyncNode,
};
use remotemedia_core::nodes::AsyncStreamingNode;
use std::sync::{Arc, Mutex};

use support::sine_sweep_16k_mono;

/// Drive the EmotionExtractor with one Text frame; collect Text + Json
/// outputs split by their data-path edges.
async fn drive_emotion_extractor(
    node: &EmotionExtractorNode,
    text: RuntimeData,
) -> (Vec<RuntimeData>, Vec<serde_json::Value>) {
    let texts = Arc::new(Mutex::new(Vec::<RuntimeData>::new()));
    let jsons = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let texts_c = Arc::clone(&texts);
    let jsons_c = Arc::clone(&jsons);
    node.process_streaming(text, None, move |out| {
        match &out {
            RuntimeData::Text(_) => texts_c.lock().unwrap().push(out.clone()),
            RuntimeData::Json(v) => jsons_c.lock().unwrap().push(v.clone()),
            _ => {}
        }
        Ok(())
    })
    .await
    .expect("emotion extractor process_streaming");
    let t = texts.lock().unwrap().clone();
    let j = jsons.lock().unwrap().clone();
    (t, j)
}

/// Drive the synthetic LipSync with a sequence of audio chunks; return
/// every emitted Json frame.
async fn drive_lipsync(
    node: &SyntheticLipSyncNode,
    chunks: Vec<RuntimeData>,
) -> Vec<serde_json::Value> {
    let collected = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    for chunk in chunks {
        let cc = Arc::clone(&collected);
        node.process_streaming(chunk, None, move |out| {
            if let RuntimeData::Json(v) = out {
                cc.lock().unwrap().push(v);
            }
            Ok(())
        })
        .await
        .expect("lipsync process_streaming");
    }
    let v = collected.lock().unwrap().clone();
    v
}

/// Make a 100 ms 16 kHz mono audio chunk from a slice of samples.
fn audio_chunk(samples: Vec<f32>) -> RuntimeData {
    RuntimeData::Audio {
        samples: AudioSamples::Vec(samples),
        sample_rate: 16_000,
        channels: 1,
        stream_id: Some("avatar-stream".to_string()),
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

#[tokio::test]
async fn synthetic_emotion_pipeline_emits_emotions_text_and_blendshapes() {
    // ── Stage 1: drive EmotionExtractor with the synthetic text stream.
    let extractor = EmotionExtractorNode::with_default_pattern();
    let input_text = tag_text_str("[EMOTION:🤩] hi [EMOTION:😊] bye", "tts");
    let (text_outputs, emotion_jsons) = drive_emotion_extractor(
        &extractor,
        RuntimeData::Text(input_text.clone()),
    )
    .await;

    // ── Spec invariant 1: two emotion events in source order.
    assert_eq!(
        emotion_jsons.len(),
        2,
        "expected two emotion events, got {}",
        emotion_jsons.len()
    );
    assert_eq!(emotion_jsons[0]["emoji"], "🤩");
    assert_eq!(emotion_jsons[1]["emoji"], "😊");
    assert!(
        emotion_jsons[0]["source_offset_chars"].as_u64().unwrap()
            < emotion_jsons[1]["source_offset_chars"].as_u64().unwrap(),
        "emotion events must be in source order"
    );

    // ── Spec invariant 2: text reaching TTS has tags stripped.
    let single_text = match text_outputs.as_slice() {
        [RuntimeData::Text(s)] => s.clone(),
        other => panic!(
            "expected exactly one Text output from EmotionExtractor, got {:?}",
            other
        ),
    };
    let (_, body) =
        remotemedia_core::data::text_channel::split_text_str(&single_text);
    assert!(
        !body.contains("[EMOTION:"),
        "tags must be stripped before reaching TTS, got: {body:?}"
    );

    // ── Stage 2: stand-in TTS path — generate 1 second of sine sweep
    // audio in three chunks (representing chunked TTS output). Real
    // kokoro_tts would emit f32 mono 16 kHz — we match that shape.
    let chunks: Vec<RuntimeData> = (0..3)
        .map(|i| {
            // 333 ms each, sweep 220 → 880 Hz, scaled by chunk index
            // so RMS varies across chunks (and therefore blendshapes
            // differ per chunk, not just static).
            let sweep = sine_sweep_16k_mono(0.333, 220.0 + (i as f32) * 100.0, 880.0);
            audio_chunk(sweep)
        })
        .collect();

    let lipsync = SyntheticLipSyncNode::new(SyntheticLipSyncConfig::default());
    let blend_jsons = drive_lipsync(&lipsync, chunks).await;

    // ── Spec invariant 3: continuous blendshape stream with monotonic pts_ms.
    assert!(
        blend_jsons.len() >= 3,
        "expected at least one blendshape Json per audio chunk, got {}",
        blend_jsons.len()
    );

    let frames: Vec<BlendshapeFrame> = blend_jsons
        .iter()
        .map(|v| BlendshapeFrame::from_json(v).expect("valid blendshape envelope"))
        .collect();

    let mut last_pts: i128 = -1;
    for f in &frames {
        assert_eq!(f.arkit_52.len(), 52, "all frames must be 52-vectors");
        assert!(
            (f.pts_ms as i128) > last_pts,
            "pts_ms must be strictly monotonic, got {} after {}",
            f.pts_ms,
            last_pts
        );
        last_pts = f.pts_ms as i128;
    }

    // ── Spec invariant 4: at least one frame has a non-zero mouth
    // (audio was non-silent; the synthetic LipSync's RMS-driven path
    // exercises the same data plane the renderer reads). This locks
    // that the e2e isn't silently degenerating to all-zeros.
    let any_jaw_movement = frames.iter().any(|f| f.arkit_52[17] > 0.05);
    assert!(
        any_jaw_movement,
        "expected at least one frame with non-trivial jawOpen"
    );
}

#[tokio::test]
async fn empty_text_input_emits_no_emotions_no_blendshapes() {
    // Sanity: silent path → no emotion events. (Renderer M4's idle/blink
    // scheduler covers the no-audio case; this just verifies upstream
    // doesn't manufacture spurious events.)
    let extractor = EmotionExtractorNode::with_default_pattern();
    let (texts, jsons) = drive_emotion_extractor(
        &extractor,
        RuntimeData::Text(tag_text_str("just hello", "tts")),
    )
    .await;
    assert_eq!(texts.len(), 1, "extractor still emits the Text frame");
    assert_eq!(jsons.len(), 0, "no tags = no emotion events");

    // Empty audio path → no jaw movement.
    let lipsync = SyntheticLipSyncNode::with_default();
    let frames = drive_lipsync(&lipsync, vec![audio_chunk(vec![0.0; 1600])]).await;
    let frame = BlendshapeFrame::from_json(&frames[0]).unwrap();
    assert!(frame.arkit_52.iter().all(|&v| v == 0.0));
}
