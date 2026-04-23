//! Qwen speech-to-speech WebRTC demo server.
//!
//! Emulates a single S2S model by composing three separate MLX-backed
//! nodes in series:
//!
//!   Whisper (STT) → Qwen3.5-9B-MLX (LLM) → Qwen3-TTS (TTS)
//!
//! Sibling of `lfm2_audio_webrtc_server.rs`. All control-bus topics are
//! named to match the LFM2 version so the same browser UI
//! (`examples/web/lfm2-audio-webrtc/`) works unchanged — the `audio`
//! node here is the TTS, which passes upstream LLM text frames through
//! verbatim so `audio.out` carries both live tokens and synthesised
//! waveform chunks.
//!
//! Topology:
//!
//! ```text
//!   mic (48k) → resample_in (48k→16k) → chunker (512) → vad → accumulator
//!                                                                 │
//!                                                                 ▼
//!                                                        stt_in (Whisper, text)
//!                                                                 │
//!                                                                 ▼
//!                                                        llm (Qwen3.5-9B, text)
//!                                                                 │
//!                                                                 ▼
//!                                                        audio (Qwen3-TTS, 24 kHz)
//!                                                                 │
//!                                                                 ▼
//!                                                        resample_out (24→48k, SINK)
//! ```
//!
//! Control-bus endpoints exposed to the browser:
//!
//! - subscribe `vad.out`       — per-chunk speech state
//! - subscribe `stt_in.out`    — user transcript
//! - subscribe `audio.out`     — LLM token stream + TTS audio envelopes
//! - publish `audio.in.context`     — inject knowledge text (llm node)
//! - publish `audio.in.system_prompt` — override persona (llm node)
//! - publish `audio.in.barge_in`     — interrupt generation (llm + audio)
//! - publish `audio.in.reset`        — wipe chat history
//!
//! Barge-in path:
//!   VAD speech_start (client) →
//!     publish `llm.in.barge_in`  (halts QwenTextMlxNode generation)
//!     publish `audio.in.barge_in` (halts QwenTTSMlxNode synthesis)
//!     control.flush_audio        (drains server WebRTC ring buffer)
//! The client-side gate allows barge-in as soon as the user's
//! transcript is in flight, not just after the first audio chunk has
//! played — see `interruptible` in session.ts.
//!
//! Usage:
//!
//! ```bash
//! cargo run --example qwen_s2s_webrtc_server \
//!     -p remotemedia-webrtc --features ws-signaling -- --port 8082
//! ```

#![cfg(feature = "ws-signaling")]

use remotemedia_core::manifest::{
    Connection, Manifest, ManifestMetadata, ManifestPythonEnv, NodeManifest,
};
use remotemedia_core::transport::{ExecutorConfig, PipelineExecutor};
use remotemedia_webrtc::config::WebRtcTransportConfig;
use remotemedia_webrtc::signaling::WebSocketSignalingServer;
use std::sync::Arc;

#[allow(unused_imports)]
use remotemedia_python_nodes as _python_nodes_link;

fn build_manifest() -> Manifest {
    let llm_repo = std::env::var("QWEN_LLM_REPO")
        .unwrap_or_else(|_| "mlx-community/Qwen3.5-9B-MLX-4bit".to_string());
    let tts_repo = std::env::var("QWEN_TTS_REPO")
        .unwrap_or_else(|_| {
            "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-6bit".to_string()
        });
    let tts_voice =
        std::env::var("QWEN_TTS_VOICE").unwrap_or_else(|_| "serena".to_string());

    let whisper_params = serde_json::json!({
        "model_id": "openai/whisper-tiny.en",
        "language": "en",
    });
    let whisper_deps = vec![
        "transformers>=4.40.0".to_string(),
        "torch>=2.1".to_string(),
        "accelerate>=0.33".to_string(),
    ];

    let llm_params = serde_json::json!({
        "hf_repo": llm_repo,
        "max_new_tokens": 256,
        "temperature": 0.7,
        "top_p": 0.9,
    });
    let llm_deps = vec![
        "mlx-lm==0.31.3".to_string(),
        "numpy>=1.24".to_string(),
    ];

    let tts_params = serde_json::json!({
        "hf_repo": tts_repo,
        "voice": tts_voice,
        // Qwen3-TTS native rate. The node upsamples to
        // `output_sample_rate` (48 kHz, matching the Opus track) before
        // yielding, so no downstream resampler is needed.
        "sample_rate": 24000,
        "output_sample_rate": 48000,
        "streaming_interval": 0.32,
        "speed": 1.0,
        "passthrough_text": true,
    });
    let tts_deps = vec![
        "mlx-audio>=0.1".to_string(),
        "numpy>=1.24".to_string(),
    ];

    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "qwen-s2s-webrtc".to_string(),
            description: Some(
                "Emulated speech-to-speech: Whisper → Qwen3.5 (MLX) → Qwen3-TTS \
                 over WebRTC, compatible with the LFM2 web UI"
                    .to_string(),
            ),
            ..Default::default()
        },
        nodes: vec![
            // 48 kHz Opus → 16 kHz mono for Silero VAD.
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
            // SileroVAD wants exactly 512-sample input chunks.
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
                    // Same hysteresis tuning as the LFM2 variant —
                    // prob >= 0.6 enters speech, must drop below 0.35
                    // to start counting silence. Keeps soft-phoneme
                    // dips mid-word from triggering a false end.
                    "threshold": 0.6,
                    "neg_threshold": 0.35,
                    "sample_rate": 16000,
                    "min_speech_duration_ms": 250,
                    "min_silence_duration_ms": 500,
                    "speech_pad_ms": 150,
                }),
                ..Default::default()
            },
            // Buffer audio during speech; release one utterance on silence.
            NodeManifest {
                id: "accumulator".to_string(),
                node_type: "AudioBufferAccumulatorNode".to_string(),
                params: serde_json::json!({
                    "min_utterance_duration_ms": 300,
                    "max_utterance_duration_ms": 30000,
                }),
                ..Default::default()
            },
            // Whisper STT — now on the MAIN path, not a side tap.
            // Its output feeds the LLM. The control-bus topic
            // `stt_in.out` remains the user-transcript tap.
            NodeManifest {
                id: "stt_in".to_string(),
                node_type: "WhisperSTTNode".to_string(),
                params: whisper_params,
                python_deps: Some(whisper_deps),
                ..Default::default()
            },
            // Qwen text chat. Emits streamed text tokens + a
            // `<|text_end|>` sentinel at end-of-reply.
            NodeManifest {
                id: "llm".to_string(),
                node_type: "QwenTextMlxNode".to_string(),
                params: llm_params,
                python_deps: Some(llm_deps),
                ..Default::default()
            },
            // Collapse the LLM's per-token text stream into
            // complete sentences before handing them to TTS. Without
            // this, QwenTTSMlxNode would buffer the entire reply
            // until `<|text_end|>`, then synthesise ~60 s of audio in
            // one call — longer than the session router's 30 s
            // per-process timeout. Per-sentence flushes keep each
            // TTS call short and stream audio to the client with
            // much lower first-audio latency.
            NodeManifest {
                id: "sentencer".to_string(),
                node_type: "TextCollectorNode".to_string(),
                params: serde_json::json!({
                    "min_sentence_length": 3,
                    "yield_partial_on_end": true,
                }),
                ..Default::default()
            },
            // Qwen3-TTS. Named `audio` so its output stream is
            // published on the `audio.out` topic, preserving the
            // same contract as LFM2AudioMlxNode for the web UI.
            NodeManifest {
                id: "audio".to_string(),
                node_type: "QwenTTSMlxNode".to_string(),
                params: tts_params,
                python_deps: Some(tts_deps),
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
                to: "stt_in".to_string(),
            },
            Connection {
                from: "stt_in".to_string(),
                to: "llm".to_string(),
            },
            Connection {
                from: "llm".to_string(),
                to: "sentencer".to_string(),
            },
            Connection {
                from: "sentencer".to_string(),
                to: "audio".to_string(),
            },
        ],
        // mlx-vlm / mlx-audio wheels target Python 3.11+. Pin 3.12 to
        // match the LFM2 demo's managed venv so the two servers share
        // a venv provisioning cache where possible.
        python_env: Some(ManifestPythonEnv {
            python_version: Some("3.12".to_string()),
            scope: None,
            extra_deps: Vec::new(),
        }),
    }
}

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
                    "qwen_s2s_webrtc_server [--host ADDR] [--port PORT]\n\n\
                     env:\n  \
                     QWEN_LLM_REPO            override LLM repo id\n  \
                     QWEN_TTS_REPO            override TTS repo id\n  \
                     QWEN_TTS_VOICE           TTS voice preset (default: serena)\n  \
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

    let mut exec_cfg = ExecutorConfig::default();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    exec_cfg.session_id_prefix = format!("s{}", ts);
    let executor = Arc::new(PipelineExecutor::with_config(exec_cfg)?);

    let config = Arc::new(WebRtcTransportConfig::default());
    let server =
        WebSocketSignalingServer::new(port, config, executor, manifest);
    let handle = server.start().await?;

    println!("READY ws://127.0.0.1:{port}/ws");
    println!("Pipeline:       Whisper STT → Qwen LLM → Qwen TTS");
    println!(
        "LLM repo:       {}",
        std::env::var("QWEN_LLM_REPO")
            .unwrap_or_else(|_| "mlx-community/Qwen3.5-9B-MLX-4bit".to_string())
    );
    println!(
        "TTS repo:       {}",
        std::env::var("QWEN_TTS_REPO").unwrap_or_else(|_| {
            "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-6bit".to_string()
        })
    );
    println!(
        "TTS voice:      {}",
        std::env::var("QWEN_TTS_VOICE").unwrap_or_else(|_| "serena".to_string())
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

    tokio::signal::ctrl_c().await?;
    drop(handle);
    Ok(())
}
