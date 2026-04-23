//! LFM2-Audio WebRTC demo server.
//!
//! Brings up the WebRTC WebSocket signaling server on a browser-reachable
//! port, armed with a turn-based LFM2-Audio pipeline. Browser clients
//! connect via WebRTC for audio, and use the *same* WebSocket for the
//! Session Control Bus (`control.subscribe` / `control.publish`).
//!
//! Topology:
//!
//! ```text
//!   mic (48k) ─► resample_in (48k→16k) ─► chunker (512) ─► vad ─► accumulator
//!                                                                     │
//!                                           ┌─────────────────────────┤
//!                                           ▼                         ▼
//!                                       stt_in (16k)       resample_up (16→24k)
//!                                                                     │
//!                                                                     ▼
//!                                                              audio (LFM2, 24k)
//!                                                                     │
//!                                                                     ▼
//!                                                            resample_out (24→48k, SINK)
//! ```
//!
//! `resample_out` is the sink — only sink outputs flow into the
//! WebRTC audio track. LFM2 emits 24 kHz, the negotiated Opus track
//! is 48 kHz (matches the client's Opus decoder); resampling up
//! before the track sender avoids recreating the encoder mid-stream
//! at a different rate, which would silently desync the RTP clock.
//! The live assistant transcript comes from the model's own text
//! tokens on the `audio.out` control-bus tap — no re-transcribe
//! needed.
//!                              │                     │
//!                              │                     ├──► stt_out
//!                              │                     │
//!                              ▼                     ▼
//!                    (control-bus text)         WebRTC audio sink
//! ```
//!
//! Control-bus endpoints exposed to the browser:
//!
//! - subscribe `vad.out`       — per-chunk speech state (`is_speech_start`, etc.)
//! - subscribe `accumulator.out` — full utterance released on silence
//! - subscribe `stt_in.out`    — user transcript (what you said)
//! - subscribe `stt_out.out`   — assistant transcript (what the model spoke back)
//! - subscribe `audio.out`     — live text tokens from LFM2 during generation
//! - publish `audio.in.context`     — inject knowledge text into the conversation
//! - publish `audio.in.system_prompt` — override the persona
//! - publish `audio.in.barge_in`     — interrupt in-flight generation
//! - publish `audio.in.reset`        — wipe chat history + context
//!
//! Usage:
//!
//! ```bash
//! # Torch backend (Linux/CUDA):
//! cargo run --example lfm2_audio_webrtc_server \
//!     -p remotemedia-webrtc --features ws-signaling -- --port 8081
//!
//! # MLX backend (Apple Silicon):
//! LFM2_AUDIO_BACKEND=mlx cargo run --example lfm2_audio_webrtc_server \
//!     -p remotemedia-webrtc --features ws-signaling -- --port 8081
//!
//! # PersonaPlex-7B (Moshi-family, MLX, Apple Silicon):
//! #   First-run cost is ~70 s for weight download + MLX kernel JIT.
//! #   The scheduler already gives the `audio` node 180 s for its
//! #   first call; override with AUDIO_TIMEOUT_MS if needed.
//! LFM2_AUDIO_BACKEND=personaplex cargo run --example lfm2_audio_webrtc_server \
//!     -p remotemedia-webrtc --features ws-signaling -- --port 8081
//! ```
//!
//! Then serve `examples/web/lfm2-audio-webrtc/` and point it at
//! `ws://127.0.0.1:8081/ws`.

#![cfg(feature = "ws-signaling")]

use remotemedia_core::manifest::{
    Connection, Manifest, ManifestMetadata, ManifestPythonEnv, NodeManifest,
};
use remotemedia_core::transport::{ExecutorConfig, PipelineExecutor};
use remotemedia_webrtc::config::WebRtcTransportConfig;
use remotemedia_webrtc::signaling::WebSocketSignalingServer;
use std::sync::Arc;

// Force-link remotemedia-python-nodes so its `inventory::submit!` macro
// pulls Python node factories (LFM2AudioNode / WhisperSTTNode / ...) into
// the default streaming registry.
#[allow(unused_imports)]
use remotemedia_python_nodes as _python_nodes_link;

#[derive(Copy, Clone, Debug)]
enum AudioBackend {
    LfmTorch,
    LfmMlx,
    PersonaPlexMlx,
}

fn select_backend() -> AudioBackend {
    match std::env::var("LFM2_AUDIO_BACKEND").as_deref() {
        Ok("mlx") | Ok("MLX") | Ok("apple") => AudioBackend::LfmMlx,
        Ok("personaplex") | Ok("PERSONAPLEX") | Ok("plex") => {
            AudioBackend::PersonaPlexMlx
        }
        _ => AudioBackend::LfmTorch,
    }
}

fn build_manifest() -> Manifest {
    let backend = select_backend();

    let whisper_params = serde_json::json!({
        "model_id": "openai/whisper-tiny.en",
        "language": "en",
    });
    let whisper_deps = vec![
        "transformers>=4.40.0".to_string(),
        "torch>=2.1".to_string(),
        "accelerate>=0.33".to_string(),
    ];

    let (audio_node_type, audio_params, audio_deps) = match backend {
        AudioBackend::LfmMlx => (
            "LFM2AudioMlxNode".to_string(),
            serde_json::json!({
                "hf_repo": "mlx-community/LFM2.5-Audio-1.5B-4bit",
                "max_new_tokens": 512,
                "sample_rate": 24000,
                "text_only": false,
            }),
            vec![
                "mlx-audio>=0.1".to_string(),
                "numpy>=1.24".to_string(),
            ],
        ),
        AudioBackend::PersonaPlexMlx => (
            "PersonaPlexAudioMlxNode".to_string(),
            serde_json::json!({
                "hf_repo": "nvidia/personaplex-7b-v1",
                "quantized": 8,
                "voice": "NATF2",
                "sample_rate": 24000,
                "system_prompt":
                    "You are a wise and friendly teacher. Answer questions or \
                     provide advice in a clear and engaging way.",
            }),
            vec![
                // Don't pin numpy here — personaplex-mlx's own pyproject
                // declares `numpy>=2.1,<2.3`, and adding a second pin
                // just hands the resolver two intersecting constraints
                // to reconcile against remotemedia-client's numpy range.
                "personaplex-mlx @ git+https://github.com/mu-hashmi/personaplex-mlx.git"
                    .to_string(),
            ],
        ),
        AudioBackend::LfmTorch => (
            "LFM2AudioNode".to_string(),
            serde_json::json!({
                "hf_repo": "LiquidAI/LFM2-Audio-1.5B",
                "max_new_tokens": 512,
                "sample_rate": 24000,
                "text_only": false,
            }),
            vec![
                "liquid-audio>=0.1".to_string(),
                "transformers>=4.54.0,<5.0".to_string(),
                "torch>=2.1".to_string(),
                "torchaudio>=2.1".to_string(),
                "accelerate>=0.33".to_string(),
            ],
        ),
    };

    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "lfm2-audio-webrtc".to_string(),
            description: Some(
                "Turn-based LFM2-Audio voice assistant over WebRTC with \
                 control-bus taps for transcripts + knowledge injection"
                    .to_string(),
            ),
            ..Default::default()
        },
        nodes: vec![
            // Incoming browser audio is 48kHz Opus. Silero VAD only
            // accepts 8k/16k, so downsample first.
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
            // SileroVAD wants 512-sample input chunks, always.
            // Without this, ONNX Runtime rejects every frame with
            // "Invalid dimension #2" and VAD emits nothing.
            NodeManifest {
                id: "chunker".to_string(),
                node_type: "AudioChunkerNode".to_string(),
                params: serde_json::json!({
                    "chunkSize": 512,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "vad".to_string(),
                node_type: "SileroVADNode".to_string(),
                params: serde_json::json!({
                    // Hysteresis: prob >= 0.6 ENTERS speech, must drop
                    // below 0.35 to start counting silence. Soft
                    // phonemes (fricatives, glides, weak vowels)
                    // routinely dip to 0.4-0.55 mid-word; without the
                    // lower release floor, VAD fires speech_end mid-
                    // utterance and Whisper gets a half-sentence.
                    // The silence window itself stays at 500 ms so
                    // end-of-turn latency is still snappy.
                    "threshold": 0.6,
                    "neg_threshold": 0.35,
                    "sample_rate": 16000,
                    "min_speech_duration_ms": 250,
                    "min_silence_duration_ms": 500,
                    "speech_pad_ms": 150,
                }),
                ..Default::default()
            },
            // Buffers audio during speech, releases one utterance on
            // silence. Downstream nodes see full turns, not chunks.
            NodeManifest {
                id: "accumulator".to_string(),
                node_type: "AudioBufferAccumulatorNode".to_string(),
                params: serde_json::json!({
                    "min_utterance_duration_ms": 300,
                    "max_utterance_duration_ms": 30000,
                }),
                ..Default::default()
            },
            // LFM2-Audio expects 24kHz mono. The accumulator emits the
            // VAD's 16kHz stream, so upsample before handing off.
            NodeManifest {
                id: "resample_up".to_string(),
                node_type: "FastResampleNode".to_string(),
                params: serde_json::json!({
                    "source_rate": 16000,
                    "target_rate": 24000,
                    "quality": "Medium",
                    "channels": 1,
                }),
                ..Default::default()
            },
            NodeManifest {
                id: "audio".to_string(),
                node_type: audio_node_type,
                params: audio_params,
                python_deps: Some(audio_deps),
                ..Default::default()
            },
            NodeManifest {
                id: "stt_in".to_string(),
                node_type: "WhisperSTTNode".to_string(),
                params: whisper_params,
                python_deps: Some(whisper_deps),
                ..Default::default()
            },
            // Upsample LFM2's 24 kHz output to match the negotiated
            // Opus track's 48 kHz. Without this, AudioTrack::send_audio
            // recreates the encoder at 24 kHz, which splits the RTP
            // stream across two clock rates and the browser's
            // Opus decoder plays silence (or garbled audio). Non-audio
            // frames (LFM2's text tokens) pass through unchanged.
            NodeManifest {
                id: "resample_out".to_string(),
                node_type: "FastResampleNode".to_string(),
                params: serde_json::json!({
                    "source_rate": 24000,
                    "target_rate": 48000,
                    "quality": "Medium",
                    "channels": 1,
                }),
                ..Default::default()
            },
        ],
        connections: vec![
            Connection {
                from: "resample_in".to_string(),
                to: "chunker".to_string(),
            },
            Connection {
                from: "chunker".to_string(),
                to: "vad".to_string(),
            },
            Connection {
                from: "vad".to_string(),
                to: "accumulator".to_string(),
            },
            Connection {
                from: "accumulator".to_string(),
                to: "resample_up".to_string(),
            },
            Connection {
                from: "resample_up".to_string(),
                to: "audio".to_string(),
            },
            Connection {
                from: "accumulator".to_string(),
                to: "stt_in".to_string(),
            },
            Connection {
                from: "audio".to_string(),
                to: "resample_out".to_string(),
            },
        ],
        // liquid-audio requires Python 3.12+. Pin the managed-venv
        // interpreter so `uv` provisions a 3.12 venv.
        python_env: Some(ManifestPythonEnv {
            python_version: Some("3.12".to_string()),
            scope: None,
            extra_deps: Vec::new(),
        }),
    }
}

/// Default the managed-venv env vars before anything else reads them.
///
/// liquid-audio (torch) and mlx-audio both need a managed-venv with the
/// right interpreter and the `remotemedia` client installed editable.
/// Without this, `multiprocess::process_manager` spawns the user's
/// system Python — which almost never has `remotemedia.core.multiprocessing.runner`
/// on its path — and every node crashes on startup. Honors any value
/// already set in the environment.
fn default_python_env() {
    let ensure = |k: &str, v: &str| {
        if std::env::var_os(k).is_none() {
            // Safe: only called once, before tokio runtime starts,
            // so no other threads race `setenv`.
            std::env::set_var(k, v);
        }
    };
    ensure("PYTHON_ENV_MODE", "managed");
    ensure("PYTHON_VERSION", "3.12");

    // Resolve <repo>/clients/python from this example's manifest dir
    // so editable installs work regardless of where the binary is run
    // from. CARGO_MANIFEST_DIR points at crates/transports/webrtc/.
    if std::env::var_os("REMOTEMEDIA_PYTHON_SRC").is_none() {
        let manifest_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidate = manifest_dir
            .ancestors()
            .nth(3) // crates/transports/webrtc -> crates/transports -> crates -> repo root
            .map(|root| root.join("clients").join("python"));
        if let Some(path) = candidate {
            if path.exists() {
                std::env::set_var("REMOTEMEDIA_PYTHON_SRC", path);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    default_python_env();

    // Honor RUST_LOG so `RUST_LOG=info,remotemedia_webrtc=debug` works.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    // CLI: just a port flag; default 8081. Keep this minimal — the
    // SPA reads the URL from its own config.
    let mut args = std::env::args().skip(1);
    let mut port: u16 = 8081;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => {
                port = args
                    .next()
                    .ok_or("--port requires a value")?
                    .parse()
                    .map_err(|e| format!("bad --port: {e}"))?;
            }
            // Accepted for CLI compat — WebSocketSignalingServer binds
            // 0.0.0.0 internally, so this is effectively ignored.
            "--host" => {
                let _ = args.next().ok_or("--host requires a value")?;
            }
            "-h" | "--help" => {
                eprintln!(
                    "lfm2_audio_webrtc_server [--host ADDR] [--port PORT]\n\n\
                     env:\n  \
                     LFM2_AUDIO_BACKEND=mlx   use Apple Silicon MLX backend\n  \
                     PYTHON_ENV_MODE=managed  use uv-managed per-node venvs\n  \
                     PYTHON_VERSION=3.12      pin managed-venv Python version\n  \
                     REMOTEMEDIA_PYTHON_SRC   path to `clients/python` for editable install"
                );
                return Ok(());
            }
            other => {
                eprintln!("unrecognized arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let manifest = Arc::new(build_manifest());

    // Timestamped session-id prefix so iceoryx2 channels (named
    // `{session_id}_{node_id}_{input,output}`) don't collide with
    // leftovers from previous runs. Without this, a restart reuses
    // `session_0_audio_output` and the new subscriber picks up
    // historical messages from the old run's shared memory, which
    // shows up as the model "replying" with its previous turn.
    let mut exec_cfg = ExecutorConfig::default();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    exec_cfg.session_id_prefix = format!("s{}", ts);

    // Cold-start budget for the `audio` node. The scheduler's default
    // per-node execution timeout (DEFAULT_TIMEOUT_MS in
    // streaming_scheduler.rs) is 30 s, which is fine for LFM2's torch
    // path but blows up PersonaPlex-7B: the MLX kernel JIT for the
    // audio-streaming step takes 50-70 s on first run, and the Python
    // runner sends READY to Rust *before* `initialize()` finishes so
    // it can buffer input during model loading. Combined, the first
    // chunk sits in Python's queue waiting for warmup while Rust's
    // 30 s clock ticks down. Give it a 180 s first-call budget;
    // steady-state latency after warmup is <1 s/frame so this only
    // matters once per session. Override with `AUDIO_TIMEOUT_MS` env
    // var for cold-start measurement.
    let audio_timeout_ms = std::env::var("AUDIO_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(180_000);
    exec_cfg.scheduler_config = exec_cfg
        .scheduler_config
        .clone()
        .with_node_timeout("audio", audio_timeout_ms);

    let executor = Arc::new(PipelineExecutor::with_config(exec_cfg)?);

    // WebRTC transport defaults are fine for a local demo: STUN at
    // stun.l.google.com:19302, Opus at 48kHz/mono.
    let config = Arc::new(WebRtcTransportConfig::default());

    let server =
        WebSocketSignalingServer::new(port, config, executor, manifest);
    let handle = server.start().await?;

    println!("READY ws://127.0.0.1:{port}/ws");
    println!(
        "Backend:        {}",
        std::env::var("LFM2_AUDIO_BACKEND").unwrap_or_else(|_| "torch".to_string())
    );
    println!(
        "PYTHON_ENV_MODE={}",
        std::env::var("PYTHON_ENV_MODE").unwrap_or_else(|_| "(unset)".to_string())
    );
    println!(
        "PYTHON_VERSION={}",
        std::env::var("PYTHON_VERSION").unwrap_or_else(|_| "(unset)".to_string())
    );
    println!(
        "REMOTEMEDIA_PYTHON_SRC={}",
        std::env::var("REMOTEMEDIA_PYTHON_SRC")
            .unwrap_or_else(|_| "(unset)".to_string())
    );
    println!("Press Ctrl-C to stop.");

    // Block until Ctrl-C. `WebSocketServerHandle::Drop` shuts the accept
    // loop, so we don't need to wire signal handling explicitly.
    tokio::signal::ctrl_c().await?;
    drop(handle);
    Ok(())
}
