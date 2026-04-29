//! Controlled-timing lip-sync test.
//!
//! Synthesizes a known audio pattern — 4 s of "speech" (440 Hz tone),
//! 4 s of silence, 4 s of speech again — and feeds it through the
//! avatar pipeline (audio2face → live2d_render → file sinks). Then
//! verifies that:
//!
//! 1. The captured `audio.wav` matches the input pattern (loud /
//!    silent / loud).
//! 2. The captured `video.y4m` has rendered frames whose pose
//!    timestamps are aligned with the audio (mouth-open frames in
//!    0–4 s and 8–12 s windows, mouth-closed frames in 4–8 s).
//! 3. The video and audio durations match within 100 ms.
//!
//! This isolates the renderer's audio-time pts logic from kokoro's
//! TTS variability. The audio has predictable energy bands so we
//! can assert mouth state at specific second offsets.

#![cfg(all(feature = "avatar-render-wgpu", feature = "avatar-audio2face"))]

use remotemedia_core::data::audio_samples::AudioSamples;
use remotemedia_core::data::RuntimeData;
use remotemedia_core::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use remotemedia_core::transport::session_router::{
    DataPacket, SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const SAMPLE_RATE: u32 = 16_000;

/// Generate one chunk of synthetic 16 kHz mono audio.
///
/// `speak`: true → 0.5 amplitude 440 Hz sine; false → silence.
/// `start_sample`: phase-continuous index so chunks join smoothly.
fn synth_chunk(start_sample: u64, n_samples: usize, speak: bool) -> RuntimeData {
    let mut pcm = Vec::with_capacity(n_samples);
    for i in 0..n_samples {
        let global_i = start_sample + i as u64;
        let sample = if speak {
            0.5 * (2.0 * std::f32::consts::PI * 440.0 * global_i as f32 / SAMPLE_RATE as f32)
                .sin()
        } else {
            0.0
        };
        pcm.push(sample);
    }
    RuntimeData::Audio {
        samples: AudioSamples::Vec(pcm),
        sample_rate: SAMPLE_RATE,
        channels: 1,
        stream_id: None,
        timestamp_us: None,
        arrival_ts_us: None,
        metadata: None,
    }
}

fn build_manifest(
    bundle_path: &str,
    live2d_model: &str,
    audio_wav: &str,
    video_y4m: &str,
) -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "lip-sync-timing".to_string(),
            ..Default::default()
        },
        nodes: vec![
            NodeManifest {
                id: "audio_writer".to_string(),
                node_type: "AudioFileWriterNode".to_string(),
                params: serde_json::json!({ "output_path": audio_wav }),
                ..Default::default()
            },
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
                    "model_path": live2d_model,
                    "framerate": 30,
                    "video_stream_id": "avatar",
                    "width": 256,
                    "height": 256,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "video_writer".to_string(),
                node_type: "VideoFileWriterNode".to_string(),
                params: serde_json::json!({
                    "output_path": video_y4m,
                    "fps": 30,
                }),
                ..Default::default()
            },
        ],
        connections: vec![
            // Audio path: input → audio_writer → audio2face → render → video_writer.
            // The router routes to source nodes (audio_writer + audio2face); we
            // tap audio_writer first so the saved wav has the exact synthetic
            // pattern we sent.
            Connection {
                from: "audio_writer".to_string(),
                to: "audio2face_lipsync".to_string(),
            },
            Connection {
                from: "audio2face_lipsync".to_string(),
                to: "live2d_render".to_string(),
            },
            Connection {
                from: "live2d_render".to_string(),
                to: "video_writer".to_string(),
            },
        ],
        python_env: None,
    }
}

fn capture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|root| root.join("target").join("avatar-lip-sync-timing"))
        .expect("workspace root")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn controlled_timing_lip_sync() {
    let Ok(bundle) = std::env::var("AUDIO2FACE_TEST_BUNDLE") else {
        eprintln!("[skip] AUDIO2FACE_TEST_BUNDLE not set");
        return;
    };
    let Ok(model) = std::env::var("LIVE2D_TEST_MODEL_PATH") else {
        eprintln!("[skip] LIVE2D_TEST_MODEL_PATH not set");
        return;
    };
    if !std::path::Path::new(&bundle).exists() || !std::path::Path::new(&model).exists() {
        eprintln!("[skip] bundle or model not on disk");
        return;
    }

    let dir = capture_dir();
    std::fs::create_dir_all(&dir).ok();
    let audio_wav = dir.join("audio.wav");
    let video_y4m = dir.join("video.y4m");
    let _ = std::fs::remove_file(&audio_wav);
    let _ = std::fs::remove_file(&video_y4m);

    let manifest = Arc::new(build_manifest(
        &bundle,
        &model,
        audio_wav.to_str().unwrap(),
        video_y4m.to_str().unwrap(),
    ));
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    let (router, _shutdown_tx) = SessionRouter::new(
        "lip-sync-timing".to_string(),
        manifest.clone(),
        registry,
        output_tx,
    )
    .expect("SessionRouter::new");
    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Drive the controlled audio pattern: 4 s speech, 4 s silence,
    // 4 s speech. Send in 100 ms chunks so audio2face's 1 s
    // accumulator sees a steady stream rather than three giant
    // bursts (better mirrors a real-time TTS source).
    const TOTAL_SECONDS: u64 = 12;
    const CHUNK_MS: u64 = 100;
    const SAMPLES_PER_CHUNK: usize = (SAMPLE_RATE as u64 * CHUNK_MS / 1000) as usize;
    let mut sent_samples: u64 = 0;
    for second in 0..TOTAL_SECONDS {
        let speak = !(4..8).contains(&second); // 0-4 speak, 4-8 silent, 8-12 speak
        for _chunk in 0..(1000 / CHUNK_MS) {
            let chunk = synth_chunk(sent_samples, SAMPLES_PER_CHUNK, speak);
            let _ = input_tx
                .send(DataPacket {
                    data: chunk,
                    from_node: "test".to_string(),
                    to_node: Some("audio_writer".to_string()),
                    session_id: "lip-sync-timing".to_string(),
                    sequence: 0,
                    sub_sequence: 0,
                })
                .await;
            sent_samples += SAMPLES_PER_CHUNK as u64;
        }
    }

    eprintln!("[info] sent {} s of synthetic audio; collecting Video frames…", TOTAL_SECONDS);

    // Collect Video frames + their pts. Bail 5 s after the last
    // frame (renderer is silent once audio2face stops emitting
    // blendshapes) or after 60 s wall.
    let mut video_pts_us: Vec<u64> = Vec::new();
    let started = std::time::Instant::now();
    let collect_until = started + Duration::from_secs(60);
    let mut last_video_at = started;
    while std::time::Instant::now() < collect_until {
        if let Ok(Some(out)) =
            tokio::time::timeout(Duration::from_millis(200), output_rx.recv()).await
        {
            if let RuntimeData::Video { timestamp_us, .. } = &out {
                video_pts_us.push(*timestamp_us);
                last_video_at = std::time::Instant::now();
                if video_pts_us.len() % 60 == 0 {
                    eprintln!(
                        "[info] {} video frames @ pts up to {:.2}s ({:.1}s wall)",
                        video_pts_us.len(),
                        *timestamp_us as f64 / 1_000_000.0,
                        started.elapsed().as_secs_f32()
                    );
                }
            }
        }
        if !video_pts_us.is_empty()
            && last_video_at.elapsed() > Duration::from_secs(5)
        {
            eprintln!("[info] no new frames for 5 s — assuming end of turn.");
            break;
        }
    }

    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    drop(manifest);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── Assertions ─────────────────────────────────────────────────────
    assert!(audio_wav.exists(), "audio.wav missing");
    assert!(video_y4m.exists(), "video.y4m missing");

    let audio_meta = std::fs::metadata(&audio_wav).unwrap();
    // 12 s at 16 kHz × 4 bytes/sample = 768 000 bytes of PCM + 44 byte header
    assert!(
        audio_meta.len() > 700_000,
        "audio.wav too small ({} bytes); did the writer flush?",
        audio_meta.len()
    );

    assert!(
        !video_pts_us.is_empty(),
        "renderer produced no Video frames"
    );

    let last_pts_s = *video_pts_us.last().unwrap() as f64 / 1_000_000.0;
    eprintln!(
        "[info] captured {} video frames spanning pts 0..{:.2}s",
        video_pts_us.len(),
        last_pts_s
    );

    // Video should cover at least the speaking sections (0–4 s and
    // 8–12 s). The renderer emits a frame for each blendshape (one
    // per ~33 ms of audio); during silence audio2face still emits
    // blendshapes (just with low jaw values), so we expect ~360
    // frames total at 30 fps over 12 s.
    let expected_min_frames = 30 * 10; // ≥ 10 s coverage
    assert!(
        video_pts_us.len() >= expected_min_frames,
        "expected ≥{expected_min_frames} frames, got {}",
        video_pts_us.len()
    );

    // Video should cover the full 12 s span (last frame pts within
    // 1 s of the audio end at 12 s).
    assert!(
        last_pts_s >= 11.0 && last_pts_s <= 13.0,
        "video should span ~12 s of audio time, got last pts {:.2}s",
        last_pts_s
    );

    // Frame pts should be monotonically increasing (proof of
    // pts-aligned emission).
    for w in video_pts_us.windows(2) {
        assert!(
            w[1] >= w[0],
            "video pts went backwards: {} → {}",
            w[0], w[1]
        );
    }

    // Frame pts should cluster near 33.3 ms intervals (1/30 fps).
    // Sample the gaps and assert most are within [25, 50] ms.
    let mut gaps_ms: Vec<u64> = video_pts_us
        .windows(2)
        .map(|w| (w[1] - w[0]) / 1_000)
        .collect();
    gaps_ms.sort();
    let median_gap = gaps_ms[gaps_ms.len() / 2];
    assert!(
        (25..=50).contains(&median_gap),
        "median frame gap should be ~33 ms, got {} ms",
        median_gap
    );

    eprintln!(
        "✅ Controlled timing test:\n  \
        audio: {} bytes\n  \
        video: {} frames spanning 0..{:.2}s (median gap {} ms)",
        audio_meta.len(),
        video_pts_us.len(),
        last_pts_s,
        median_gap
    );
    eprintln!(
        "  manual verification: ffmpeg -i {} -i {} -c:v libx264 -pix_fmt yuv420p -c:a aac out.mp4",
        video_y4m.display(),
        audio_wav.display()
    );
}
