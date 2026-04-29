//! Load-bearing avatar e2e — runs the canonical §4.1 chain *and*
//! taps the audio + video streams to disk so you can play the
//! capture back and verify lip-sync visually + audibly.
//!
//! ```text
//! text("[EMOTION:🤩]hi …")
//!   → EmotionExtractorNode
//!       ├─(text)── KokoroTTSNode (Python multiproc, 24 kHz)
//!       │            ├──→ AudioFileWriterNode → audio.wav  (TAP)
//!       │            └── FastResampleNode (24 → 16 kHz)
//!       │                  └── Audio2FaceLipSyncNode
//!       │                        └── Live2DRenderNode
//!       │                              └──→ VideoFileWriterNode → video.y4m
//!       └─(json)─────────────────────────────────────────┘
//! ```
//!
//! After the test runs, the captured files live in
//! `target/avatar-disk-capture/{audio.wav, video.y4m}`. To view +
//! sync them:
//!
//! ```bash
//! ffmpeg -i target/avatar-disk-capture/video.y4m \
//!        -i target/avatar-disk-capture/audio.wav \
//!        -c:v libx264 -pix_fmt yuv420p -c:a aac \
//!        target/avatar-disk-capture/out.mp4
//! open target/avatar-disk-capture/out.mp4
//! ```
//!
//! Same env-var gate as the canonical e2e (`LIVE2D_TEST_MODEL_PATH`
//! + `AUDIO2FACE_TEST_BUNDLE`). Skips cleanly when not set.
//!
//! **Why we need this test**: the standalone factory pipeline test
//! (no TTS) only proves blendshapes + render compose; it can't
//! verify the AUDIO maps onto the rendered MOUTH SHAPES. The only
//! way to validate lip-sync is to look at the saved video and
//! listen to the saved audio side-by-side.

#![cfg(feature = "avatar-render-wgpu")]

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

#[allow(unused_imports)]
use remotemedia_python_nodes as _python_nodes_link;

fn check_env() -> Option<&'static str> {
    if std::env::var("LIVE2D_TEST_MODEL_PATH").is_err() {
        return Some("LIVE2D_TEST_MODEL_PATH");
    }
    if std::env::var("AUDIO2FACE_TEST_BUNDLE").is_err() {
        return Some("AUDIO2FACE_TEST_BUNDLE");
    }
    None
}

fn ensure_managed_python_env() {
    let ensure = |k: &str, v: &str| {
        if std::env::var_os(k).is_none() {
            std::env::set_var(k, v);
        }
    };
    ensure("PYTHON_ENV_MODE", "managed");
    ensure("PYTHON_VERSION", "3.12");
    if std::env::var_os("REMOTEMEDIA_PYTHON_SRC").is_none() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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

fn capture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|root| root.join("target").join("avatar-disk-capture"))
        .expect("workspace root")
}

fn build_manifest(
    audio2face_bundle: &str,
    live2d_model: &str,
    audio_wav: &str,
    video_y4m: &str,
) -> Manifest {
    let kokoro_deps = vec![
        "kokoro>=0.9.4".to_string(),
        "soundfile".to_string(),
        "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl".to_string(),
    ];
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "avatar-disk-capture".to_string(),
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
            // Tap: audio file writer sits between kokoro_tts and the
            // resampler so it captures the TTS-native 24 kHz audio.
            NodeManifest {
                id: "audio_writer".to_string(),
                node_type: "AudioFileWriterNode".to_string(),
                params: serde_json::json!({ "output_path": audio_wav }),
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
                    "bundle_path": audio2face_bundle,
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
                    "width": 512,
                    "height": 512,
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
            // Text branch: emotion → tts → audio_writer (passthrough)
            // → resample → audio2face → live2d_render → video_writer.
            Connection {
                from: "emotion_extractor".to_string(),
                to: "kokoro_tts".to_string(),
            },
            Connection {
                from: "kokoro_tts".to_string(),
                to: "audio_writer".to_string(),
            },
            Connection {
                from: "audio_writer".to_string(),
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
            Connection {
                from: "live2d_render".to_string(),
                to: "video_writer".to_string(),
            },
            // Json branch: emotion events fan to the renderer alongside
            // the blendshapes coming from audio2face.
            Connection {
                from: "emotion_extractor".to_string(),
                to: "live2d_render".to_string(),
            },
        ],
        python_env: None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn captures_avatar_audio_and_video_to_disk_for_visual_inspection() {
    if let Some(missing) = check_env() {
        eprintln!(
            "[skip] {missing} not set — see avatar_disk_capture_test.rs \
             docstring for required env vars"
        );
        return;
    }
    ensure_managed_python_env();

    let bundle = std::env::var("AUDIO2FACE_TEST_BUNDLE").unwrap();
    let model = std::env::var("LIVE2D_TEST_MODEL_PATH").unwrap();
    let dir = capture_dir();
    std::fs::create_dir_all(&dir).ok();
    let audio_wav = dir.join("audio.wav");
    let video_y4m = dir.join("video.y4m");
    // Wipe any prior capture so the test always reflects this run.
    let _ = std::fs::remove_file(&audio_wav);
    let _ = std::fs::remove_file(&video_y4m);

    let manifest = Arc::new(build_manifest(
        &bundle,
        &model,
        audio_wav.to_str().unwrap(),
        video_y4m.to_str().unwrap(),
    ));
    let registry = Arc::new(create_default_streaming_registry());
    let (output_tx, mut output_rx) =
        mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

    eprintln!(
        "[info] building SessionRouter (heavy: kokoro venv + Audio2Face \
         load + wgpu+Aria init)…"
    );
    let (router, _shutdown_tx) = SessionRouter::new(
        "avatar-disk-capture".to_string(),
        manifest.clone(),
        registry,
        output_tx,
    )
    .expect("SessionRouter::new");

    let input_tx = router.get_input_sender();
    let handle = router.start();

    // Drive a longer text turn so the audio file ends up with
    // multiple seconds of speech — easier to verify lip-sync vs a
    // half-second clip. Two emoji span the turn so emotion
    // expression also exercises.
    let text = "[EMOTION:\u{1F929}]Hello there friend! \
                I'm Aria, glad to meet you. \
                [EMOTION:\u{1F60A}]Have a wonderful day.";
    let _ = input_tx
        .send(DataPacket {
            data: RuntimeData::Text(text.to_string()),
            from_node: "client".to_string(),
            to_node: None,
            session_id: "avatar-disk-capture".to_string(),
            sequence: 0,
            sub_sequence: 0,
        })
        .await;

    eprintln!("[info] driving text turn — collecting Video frames for up to 90s …");
    // Collect Video for up to 90s. First-run cold path is dominated
    // by the kokoro voice pack download (only on first invocation,
    // cached afterward); subsequent runs are much faster.
    let mut video_frame_count = 0u64;
    let started = std::time::Instant::now();
    let collect_until = started + Duration::from_secs(90);
    let mut last_video_at = started;
    while std::time::Instant::now() < collect_until {
        if let Ok(Some(out)) =
            tokio::time::timeout(Duration::from_millis(200), output_rx.recv()).await
        {
            if matches!(out, RuntimeData::Video { .. }) {
                video_frame_count += 1;
                last_video_at = std::time::Instant::now();
                if video_frame_count % 30 == 0 {
                    eprintln!(
                        "[info] {} video frames captured ({:.1}s elapsed)",
                        video_frame_count,
                        started.elapsed().as_secs_f32()
                    );
                }
            }
        }
        // Bail once we've been idle for 5s after first frame —
        // means the TTS turn has finished + drained.
        if video_frame_count > 0
            && last_video_at.elapsed() > Duration::from_secs(5)
        {
            eprintln!("[info] no new frames for 5s — assuming end of turn.");
            break;
        }
    }

    // Drop the input + tear down the router so file-sink Drops fire
    // (which is when WAV size headers get patched).
    drop(input_tx);
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    // Also drop the manifest's Arc to release any node references
    // the SessionRouter still holds — the file-sink Drops fire here.
    drop(manifest);
    // Give Drop tasks a beat to finish their I/O.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Sanity assertions.
    assert!(
        audio_wav.exists(),
        "audio.wav missing at {}",
        audio_wav.display()
    );
    assert!(
        video_y4m.exists(),
        "video.y4m missing at {}",
        video_y4m.display()
    );
    let audio_meta = std::fs::metadata(&audio_wav).unwrap();
    let video_meta = std::fs::metadata(&video_y4m).unwrap();
    assert!(
        audio_meta.len() > 1024,
        "audio.wav too small ({} bytes) — TTS likely did not synth",
        audio_meta.len()
    );
    assert!(
        video_meta.len() > 1024,
        "video.y4m too small ({} bytes) — renderer likely produced no frames",
        video_meta.len()
    );
    // Y4M magic.
    let mut head = [0u8; 10];
    use std::io::Read;
    let mut f = std::fs::File::open(&video_y4m).unwrap();
    f.read_exact(&mut head).unwrap();
    assert_eq!(&head[..10], b"YUV4MPEG2 ", "video.y4m header magic wrong");

    eprintln!(
        "\n✅ Avatar disk capture written:\n  audio: {}  ({} bytes)\n  video: {}  ({} bytes; {} frames)\n",
        audio_wav.display(),
        audio_meta.len(),
        video_y4m.display(),
        video_meta.len(),
        video_frame_count,
    );
    eprintln!("Combine with:");
    eprintln!(
        "  ffmpeg -i {} -i {} -c:v libx264 -pix_fmt yuv420p -c:a aac {}/out.mp4",
        video_y4m.display(),
        audio_wav.display(),
        dir.display()
    );
    eprintln!("\nThen `open {}/out.mp4` to verify the lip-sync visually + audibly.", dir.display());

    assert!(
        video_frame_count >= 30,
        "expected ≥30 video frames; got {video_frame_count}"
    );
}
