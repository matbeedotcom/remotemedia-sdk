//! Qwen speech-to-speech WebRTC demo server with **Live2D avatar lip-sync**.
//!
//! Extends [`qwen_s2s_webrtc_server`] by inserting the avatar pipeline
//! (`EmotionExtractorNode` → `Audio2FaceLipSyncNode` → `Live2DRenderNode`)
//! so the browser receives BOTH the synthesized voice on a WebRTC audio
//! track AND a 30 fps lip-synced avatar video on a WebRTC video track.
//!
//! Topology:
//!
//! ```text
//!   mic (48k) → resample_in (48→16k) → chunker (512) → vad ──┬──→ accumulator
//!                                                            │
//!                                                            │      ▼
//!                                                            │   stt_in (Whisper)
//!                                                            │      │
//!                                                            │      ▼
//!                                                            │   llm (Qwen3.6-27B GGUF, streams text)
//!                                                            │      │
//!                                                            └──→ coordinator (turn phase, sentencer)
//!                                                                   │
//!                                                                   ▼
//!                                                          emotion_extractor
//!                                                          ┌─(text)──→ kokoro_tts (24 kHz)
//!                                                          │              │
//!                                                          │              ├──→ audio (resample 24→48k, AUDIO SINK)
//!                                                          │              │
//!                                                          │              └──→ resample_a2f (24→16k)
//!                                                          │                          │
//!                                                          │                          ▼
//!                                                          │              audio2face_lipsync (Audio2Face ONNX)
//!                                                          │                          │
//!                                                          │                          ▼
//!                                                          └─(emotion)──→ live2d_render (Aria, VIDEO SINK)
//! ```
//!
//! The pipeline routes Kokoro's audio to two sinks: the WebRTC audio
//! output (after resampling to 48 kHz) AND the Audio2Face lip-sync chain
//! (after resampling to 16 kHz). Each blendshape produced by Audio2Face
//! synchronously renders one Video frame in `live2d_render`, stamped
//! with the audio-time pts so the audio track and video track stay
//! in lip-sync end-to-end.
//!
//! Control-bus endpoints exposed to the browser (same as the non-avatar
//! variant, plus the new video stream):
//!
//! - subscribe `vad.out`            — per-chunk speech state
//! - subscribe `stt_in.out`         — user transcript
//! - subscribe `audio.out`          — LLM token stream + TTS audio envelopes
//! - subscribe `coordinator.out`    — authoritative turn_state events
//! - publish `audio.in.barge_in`    — interrupt generation
//! - publish `audio.in.reset`       — wipe chat history
//!
//! # Required model assets
//!
//! - `LIVE2D_CUBISM_CORE_DIR`   (build-time) — unpacked Cubism SDK for Native.
//!                                             **Must be an absolute path** —
//!                                             cubism-core-sys's build script
//!                                             runs in a different cwd than
//!                                             cargo, so relative paths fail.
//! - `LIVE2D_AVATAR_MODEL_PATH` (runtime)    — path to the `.model3.json`
//!                                             (NOT the `.moc3` binary).
//! - `AUDIO2FACE_BUNDLE_PATH`   (runtime)    — directory with the persona-engine
//!                                             Audio2Face bundle (network.onnx,
//!                                             bs_skin_<Identity>.npz, etc.)
//!
//! # Usage
//!
//! ```bash
//! # All paths must be absolute. From the workspace root:
//! LIVE2D_CUBISM_CORE_DIR="$(pwd)/sdk/CubismSdkForNative-5-r.5" \
//! LIVE2D_AVATAR_MODEL_PATH="$(pwd)/models/live2d/aria/aria.model3.json" \
//! AUDIO2FACE_BUNDLE_PATH="$(pwd)/models/audio2face" \
//! QWEN_MODEL_PATH="$(pwd)/models/UD-Q4_K_XL.gguf" \
//! cargo run -p remotemedia-webrtc \
//!     --example qwen_s2s_avatar_webrtc_server \
//!     --features ws-signaling,avatar -- --port 8082
//! ```

#![cfg(feature = "ws-signaling")]

use remotemedia_core::manifest::{
    Connection, Manifest, ManifestMetadata, ManifestPythonEnv, NodeManifest,
};
use remotemedia_core::transport::{ExecutorConfig, PipelineExecutor};
use remotemedia_webrtc::config::WebRtcTransportConfig;
use remotemedia_webrtc::signaling::WebSocketSignalingServer;
use std::sync::Arc;

// Force-link remotemedia-python-nodes so its `inventory::submit!` macro
// pulls Python node factories (WhisperSTTNode, KokoroTTSNode, …) into
// the default streaming registry.
#[allow(unused_imports)]
use remotemedia_python_nodes as _python_nodes_link;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn parse_env_f32(key: &str) -> Option<f32> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn parse_env_bool(key: &str) -> Option<bool> {
    std::env::var(key).ok().and_then(|v| match v.to_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    })
}

fn parse_gpu_offload(val: &str) -> serde_json::Value {
    match val.to_lowercase().as_str() {
        "none" | "0" | "cpu" => serde_json::json!("none"),
        "all" | "gpu" => serde_json::json!("all"),
        n => {
            if let Ok(layers) = n.parse::<u16>() {
                serde_json::json!({ "layers": layers })
            } else {
                serde_json::json!("all")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Manifest builder
// ---------------------------------------------------------------------------

fn build_manifest() -> Result<Manifest, Box<dyn std::error::Error>> {
    // ---- Avatar asset paths (required) ----

    let live2d_model = std::env::var("LIVE2D_AVATAR_MODEL_PATH").map_err(|_| {
        "LIVE2D_AVATAR_MODEL_PATH not set; point at e.g. \
         models/live2d/aria/aria.model3.json (NOT the .moc3 binary)"
    })?;
    let audio2face_bundle = std::env::var("AUDIO2FACE_BUNDLE_PATH").map_err(|_| {
        "AUDIO2FACE_BUNDLE_PATH not set; point at the persona-engine \
         Audio2Face bundle directory (network.onnx + bs_skin_*.npz + …)"
    })?;
    if !std::path::Path::new(&live2d_model).exists() {
        return Err(format!("LIVE2D_AVATAR_MODEL_PATH not found on disk: {}", live2d_model).into());
    }
    if !std::path::Path::new(&audio2face_bundle).exists() {
        return Err(
            format!("AUDIO2FACE_BUNDLE_PATH not found on disk: {}", audio2face_bundle).into(),
        );
    }

    // ---- LLM (llama.cpp / GGUF) ----

    let default_system_prompt = "You are a helpful, friendly voice assistant. \
You speak conversationally and keep responses concise. \
When the user asks for code, commands, or structured data, provide it \
in markdown code blocks. Otherwise, respond in natural prose. \
Express emotion using emoji at the start of sentences (e.g. 🤩, 😊, 😢, 🤔).";

    let model_path = std::env::var("QWEN_MODEL_PATH")
        .unwrap_or_else(|_| "unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL".to_string());

    let gpu_offload = std::env::var("QWEN_GPU_OFFLOAD")
        .unwrap_or_else(|_| "all".to_string());

    let flash_attention = parse_env_bool("QWEN_FLASH_ATTENTION").unwrap_or(true);

    let llm_params = serde_json::json!({
        "model_path": model_path,
        "backend": {
            "numa": false,
            "gpu_offload": parse_gpu_offload(&gpu_offload),
            "flash_attention": flash_attention,
            "threads": parse_env_u32("QWEN_THREADS"),
            "threads_batch": null,
        },
        "context_size": parse_env_u32("QWEN_CONTEXT_SIZE").unwrap_or(8192),
        "batch_size": 2048,
        "max_tokens": parse_env_u32("QWEN_MAX_TOKENS").unwrap_or(2048),
        "temperature": parse_env_f32("QWEN_TEMPERATURE").unwrap_or(0.6),
        "top_p": parse_env_f32("QWEN_TOP_P").unwrap_or(0.8),
        "top_k": parse_env_u32("QWEN_TOP_K").unwrap_or(20),
        "min_p": parse_env_f32("QWEN_MIN_P").unwrap_or(0.0),
        "repeat_penalty": 1.1,
        "system_prompt": std::env::var("QWEN_SYSTEM_PROMPT")
            .unwrap_or_else(|_| default_system_prompt.to_string()),
        "seed": 0,
    });

    // ---- Whisper STT ----

    let whisper_params = serde_json::json!({
        "model_id": "openai/whisper-base.en",
        "language": "en",
        "device": "cpu",
    });
    let whisper_deps = vec![
        "transformers>=4.40.0".to_string(),
        "torch>=2.1".to_string(),
        "accelerate>=0.33".to_string(),
    ];

    // ---- Kokoro TTS (24 kHz output; resampled both ways downstream) ----

    let kokoro_params = serde_json::json!({
        "lang_code": std::env::var("KOKORO_LANG").unwrap_or_else(|_| "a".to_string()),
        "voice": std::env::var("KOKORO_VOICE").unwrap_or_else(|_| "af_heart".to_string()),
        "speed": 1.0,
        "sample_rate": 24000,
        "stream_chunks": true,
        "skip_tokens": ["<|text_end|>", "```"],
    });
    let kokoro_deps = vec![
        "kokoro>=0.9.4".to_string(),
        "soundfile".to_string(),
        "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl".to_string(),
    ];

    // ---- Avatar video size ----

    let avatar_width =
        parse_env_u32("AVATAR_WIDTH").unwrap_or(512);
    let avatar_height =
        parse_env_u32("AVATAR_HEIGHT").unwrap_or(512);

    // ---- Nodes ----

    let nodes: Vec<NodeManifest> = vec![
        // --- Audio input pipeline (mic → VAD → STT → LLM → coordinator) ---
        NodeManifest {
            id: "resample_in".to_string(),
            node_type: "FastResampleNode".to_string(),
            params: serde_json::json!({
                "source_rate": 48000,
                "target_rate": 16000,
                "quality": "Medium",
                "channels": 1,
            }),
            ..Default::default()
        },
        NodeManifest {
            id: "chunker".to_string(),
            node_type: "AudioChunkerNode".to_string(),
            params: serde_json::json!({ "chunkSize": 512 }),
            ..Default::default()
        },
        NodeManifest {
            id: "vad".to_string(),
            node_type: "SileroVADNode".to_string(),
            params: serde_json::json!({
                "threshold": 0.6,
                "neg_threshold": 0.35,
                "sample_rate": 16000,
                "min_speech_duration_ms": 250,
                "min_silence_duration_ms": 500,
                "speech_pad_ms": 150,
            }),
            ..Default::default()
        },
        NodeManifest {
            id: "accumulator".to_string(),
            node_type: "AudioBufferAccumulatorNode".to_string(),
            params: serde_json::json!({
                "min_utterance_duration_ms": 300,
                "max_utterance_duration_ms": 30000,
            }),
            ..Default::default()
        },
        NodeManifest {
            id: "stt_in".to_string(),
            node_type: "WhisperSTTNode".to_string(),
            params: whisper_params,
            python_deps: Some(whisper_deps),
            ..Default::default()
        },
        NodeManifest {
            id: "llm".to_string(),
            node_type: "LlamaCppGenerationNode".to_string(),
            params: llm_params,
            ..Default::default()
        },
        NodeManifest {
            id: "coordinator".to_string(),
            node_type: "ConversationCoordinatorNode".to_string(),
            params: serde_json::json!({
                "split_pattern": r"[.!?,;:\n]+",
                "min_sentence_length": 2,
                "yield_partial_on_end": true,
                "llm_silence_timeout_ms": 120000,
                "user_speech_debounce_ms": 150,
                // Barge targets: stop both LLM generation, TTS, and reset
                // the avatar's lip-sync state so the previous turn's
                // mouth animation doesn't bleed into the next.
                "barge_in_targets": ["llm", "kokoro_tts", "audio2face_lipsync", "live2d_render"],
                "first_chunk_min_chars": 25,
            }),
            ..Default::default()
        },

        // --- Avatar pipeline ---
        // Splits the LLM text into a clean text stream + emoji-derived
        // emotion JSON envelopes. Text → kokoro_tts; emotion → live2d_render.
        NodeManifest {
            id: "emotion_extractor".to_string(),
            node_type: "EmotionExtractorNode".to_string(),
            params: serde_json::json!({}),
            ..Default::default()
        },
        NodeManifest {
            id: "kokoro_tts".to_string(),
            node_type: "KokoroTTSNode".to_string(),
            params: kokoro_params,
            python_deps: Some(kokoro_deps),
            ..Default::default()
        },
        // 24 kHz Kokoro audio → 48 kHz for the WebRTC audio track.
        // Named "audio" by convention so WebRTC adapts it as the audio
        // sink (matches the non-avatar variant).
        NodeManifest {
            id: "audio".to_string(),
            node_type: "FastResampleNode".to_string(),
            params: serde_json::json!({
                "source_rate": 24000,
                "target_rate": 48000,
                "quality": "Medium",
                "channels": 1,
            }),
            ..Default::default()
        },
        // 24 kHz Kokoro audio → 16 kHz for Audio2Face inference.
        // Tap off the same kokoro_tts output as the audio sink above so
        // the avatar's mouth animates against exactly the audio the
        // listener hears.
        NodeManifest {
            id: "resample_a2f".to_string(),
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
                "identity": std::env::var("AUDIO2FACE_IDENTITY")
                    .unwrap_or_else(|_| "Claire".to_string()),
                "solver": std::env::var("AUDIO2FACE_SOLVER")
                    .unwrap_or_else(|_| "pgd".to_string()),
                "use_gpu": parse_env_bool("AUDIO2FACE_USE_GPU").unwrap_or(false),
                "smoothing_alpha": parse_env_f32("AUDIO2FACE_SMOOTHING")
                    .unwrap_or(0.0),
            }),
            ..Default::default()
        },
        // Renderer + video sink. Emits RuntimeData::Video with stream_id
        // = "avatar"; the WebRTC server attaches a VideoTrack for it.
        NodeManifest {
            id: "live2d_render".to_string(),
            node_type: "Live2DRenderNode".to_string(),
            params: serde_json::json!({
                "model_path": live2d_model,
                "framerate": 30,
                "video_stream_id": "avatar",
                "width": avatar_width,
                "height": avatar_height,
            }),
            ..Default::default()
        },
    ];

    // ---- Connections ----

    let connections: Vec<Connection> = vec![
        // Input → VAD
        Connection { from: "resample_in".into(), to: "chunker".into() },
        Connection { from: "chunker".into(), to: "vad".into() },
        Connection { from: "vad".into(), to: "accumulator".into() },
        // Coordinator watches VAD events for barge detection
        Connection { from: "vad".into(), to: "coordinator".into() },
        // STT → LLM → coordinator
        Connection { from: "accumulator".into(), to: "stt_in".into() },
        Connection { from: "stt_in".into(), to: "llm".into() },
        Connection { from: "llm".into(), to: "coordinator".into() },
        // Coordinator → emotion extraction
        Connection { from: "coordinator".into(), to: "emotion_extractor".into() },
        // Emotion extractor splits into two streams:
        //   - text → kokoro_tts (the audio we synthesize)
        //   - emotion json → live2d_render (sets expression/motion)
        Connection { from: "emotion_extractor".into(), to: "kokoro_tts".into() },
        Connection { from: "emotion_extractor".into(), to: "live2d_render".into() },
        // Kokoro audio fans out two ways:
        //   - 24 → 48 kHz for the WebRTC audio track (sink id = "audio")
        //   - 24 → 16 kHz for Audio2Face inference
        Connection { from: "kokoro_tts".into(), to: "audio".into() },
        Connection { from: "kokoro_tts".into(), to: "resample_a2f".into() },
        // Audio2Face → renderer
        Connection { from: "resample_a2f".into(), to: "audio2face_lipsync".into() },
        Connection { from: "audio2face_lipsync".into(), to: "live2d_render".into() },
    ];

    Ok(Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "qwen-llama-s2s-avatar-webrtc".to_string(),
            description: Some(
                "Speech-to-speech with Live2D avatar lip-sync: \
                 Whisper → Qwen3.6-27B-GGUF → Kokoro TTS → Audio2Face → \
                 Live2D (Aria) over WebRTC"
                    .to_string(),
            ),
            ..Default::default()
        },
        nodes,
        connections,
        python_env: Some(ManifestPythonEnv {
            python_version: Some("3.12".to_string()),
            scope: None,
            extra_deps: Vec::new(),
        }),
    })
}

// ---------------------------------------------------------------------------
// Python env defaults
// ---------------------------------------------------------------------------

fn default_python_env() {
    let ensure = |k: &str, v: &str| {
        if std::env::var_os(k).is_none() {
            std::env::set_var(k, v);
        }
    };
    ensure("PYTHON_ENV_MODE", "managed");
    ensure("PYTHON_VERSION", "3.12");

    if std::env::var_os("REMOTEMEDIA_PYTHON_SRC").is_none() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidate = manifest_dir
            .ancestors()
            .nth(3)
            .map(|root| root.join("clients").join("python"));
        if let Some(path) = candidate {
            if path.exists() {
                std::env::set_var("REMOTEMEDIA_PYTHON_SRC", path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    default_python_env();

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let mut args = std::env::args().skip(1);
    let mut port: u16 = 8082;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                port = args
                    .next()
                    .ok_or("--port requires a value")?
                    .parse()
                    .map_err(|e| format!("bad --port: {e}"))?;
            }
            "--host" => {
                let _ = args.next().ok_or("--host requires a value")?;
            }
            "-h" | "--help" => {
                eprintln!(
                    "qwen_s2s_avatar_webrtc_server [--host ADDR] [--port PORT]\n\n\
                     S2S with Live2D avatar lip-sync:\n  \
                     Whisper STT → Qwen3.6-27B-GGUF (llama.cpp) → Kokoro TTS\n  \
                     → Audio2Face (ONNX) → Live2D (Aria) over WebRTC\n\n\
                     required env (use ABSOLUTE paths — relative paths fail in build scripts):\n  \
                     LIVE2D_AVATAR_MODEL_PATH  path to .model3.json (NOT .moc3)\n  \
                     AUDIO2FACE_BUNDLE_PATH    persona-engine Audio2Face bundle dir\n  \
                     LIVE2D_CUBISM_CORE_DIR    Cubism SDK for Native (build-time, absolute)\n\n\
                     env (optional):\n  \
                     QWEN_MODEL_PATH         GGUF model path (default: unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL)\n  \
                     QWEN_SYSTEM_PROMPT      System message\n  \
                     QWEN_CONTEXT_SIZE       Context window in tokens (default: 8192)\n  \
                     QWEN_MAX_TOKENS         Max tokens per generation (default: 2048)\n  \
                     QWEN_GPU_OFFLOAD        GPU offload: none, all, or layer count (default: all)\n  \
                     QWEN_FLASH_ATTENTION    Enable Flash Attention 2 (default: true)\n  \
                     QWEN_THREADS            Computation threads (default: auto)\n  \
                     QWEN_TEMPERATURE        Sampling temperature (default: 0.6)\n  \
                     QWEN_TOP_P              Nucleus sampling cutoff (default: 0.8)\n  \
                     QWEN_TOP_K              Top-k sampling cutoff (default: 20)\n  \
                     QWEN_MIN_P              Min-p sampling cutoff (default: 0.0)\n  \
                     KOKORO_LANG             Kokoro language code (default: a)\n  \
                     KOKORO_VOICE            Kokoro voice preset (default: af_heart)\n  \
                     AUDIO2FACE_IDENTITY     Claire | James | Mark (default: Claire)\n  \
                     AUDIO2FACE_SOLVER       pgd | bvls (default: pgd)\n  \
                     AUDIO2FACE_USE_GPU      true | false (default: false)\n  \
                     AUDIO2FACE_SMOOTHING    Per-frame ARKit EMA alpha (default: 0.0)\n  \
                     AVATAR_WIDTH            Render width (default: 512)\n  \
                     AVATAR_HEIGHT           Render height (default: 512)\n  \
                     PYTHON_ENV_MODE=managed use uv-managed per-node venvs\n  \
                     PYTHON_VERSION=3.12     pin managed-venv Python version\n  \
                     REMOTEMEDIA_PYTHON_SRC  path to clients/python for editable install"
                );
                return Ok(());
            }
            other => {
                eprintln!("unrecognized arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let manifest = Arc::new(build_manifest()?);

    let mut exec_cfg = ExecutorConfig::default();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    exec_cfg.session_id_prefix = format!("s{}", ts);

    // Per-node scheduler budgets. The avatar nodes are heavy-weight on
    // first call: Audio2Face cold ONNX inference is ~3.6 s on Apple
    // Silicon CPU; Live2D wgpu device init + Aria texture upload is
    // ~1-2 s; LlamaCppGenerationNode loads the full 27B GGUF on first
    // inference (~30-60 s). Subsequent calls are much faster.
    exec_cfg.scheduler_config = exec_cfg
        .scheduler_config
        .with_node_timeout("llm", 300_000)
        .with_node_timeout("kokoro_tts", 120_000)
        .with_node_timeout("audio2face_lipsync", 60_000)
        .with_node_timeout("live2d_render", 30_000);

    let executor = Arc::new(PipelineExecutor::with_config(exec_cfg)?);

    let config = Arc::new(WebRtcTransportConfig::default());
    let server = WebSocketSignalingServer::new(port, config, executor, manifest);
    let handle = server.start().await?;

    println!("READY ws://127.0.0.1:{port}/ws");
    println!("Pipeline:       Whisper STT → Qwen3.6-27B-GGUF (llama.cpp) → Kokoro TTS → Audio2Face → Live2D (Aria)");
    println!(
        "Model path:     {}",
        std::env::var("QWEN_MODEL_PATH")
            .unwrap_or_else(|_| "unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL".to_string())
    );
    println!(
        "Avatar model:   {}",
        std::env::var("LIVE2D_AVATAR_MODEL_PATH").unwrap_or_else(|_| "(unset)".into())
    );
    println!(
        "A2F bundle:     {}",
        std::env::var("AUDIO2FACE_BUNDLE_PATH").unwrap_or_else(|_| "(unset)".into())
    );
    println!(
        "A2F identity:   {}",
        std::env::var("AUDIO2FACE_IDENTITY").unwrap_or_else(|_| "Claire".into())
    );
    println!(
        "Avatar size:    {}x{}",
        std::env::var("AVATAR_WIDTH").unwrap_or_else(|_| "512".into()),
        std::env::var("AVATAR_HEIGHT").unwrap_or_else(|_| "512".into())
    );
    println!(
        "GPU offload:    {}",
        std::env::var("QWEN_GPU_OFFLOAD").unwrap_or_else(|_| "all".to_string())
    );
    println!(
        "KOKORO_VOICE:   {}",
        std::env::var("KOKORO_VOICE").unwrap_or_else(|_| "af_heart".to_string())
    );
    println!(
        "PYTHON_ENV_MODE={}",
        std::env::var("PYTHON_ENV_MODE").unwrap_or_else(|_| "(unset)".to_string())
    );
    println!(
        "REMOTEMEDIA_PYTHON_SRC={}",
        std::env::var("REMOTEMEDIA_PYTHON_SRC").unwrap_or_else(|_| "(unset)".to_string())
    );
    println!("Press Ctrl-C to stop.");

    tokio::signal::ctrl_c().await?;
    drop(handle);
    Ok(())
}
