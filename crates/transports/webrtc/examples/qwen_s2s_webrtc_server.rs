//! Qwen speech-to-speech WebRTC demo server (llama.cpp backend).
//!
//! Emulates a single S2S model by composing three separate nodes in series:
//!
//!   Whisper (STT) → Qwen3.6-27B-GGUF via LlamaCppGenerationNode (LLM) → Kokoro (TTS)
//!
//! Uses the native Rust llama.cpp inference engine instead of the Python/MLX
//! variant in `qwen_s2s_webrtc_server.rs`. The LLM node is
//! `LlamaCppGenerationNode` which wraps llama.cpp's C library through safe
//! Rust bindings (`llama-cpp-4`).
//!
//! Topology:
//!
//! ```text
//!   mic (48k) → resample_in (48k→16k) → chunker (512) → vad ──┬──→ accumulator
//!                                                             │
//!                                                             │      ▼
//!                                                             │   stt_in (Whisper, text)
//!                                                             │      │
//!                                                             │      ▼
//!                                                             │   llm (LlamaCppGenerationNode, streams text)
//!                                                             │      │
//!                                                             └──→ coordinator (turn phase, sentencer)
//!                                                                    │
//!                                                                    ▼
//!                                                                 kokoro_tts (Kokoro, 24 kHz)
//!                                                                    │
//!                                                                    ▼
//!                                                                 audio (resample 24→48k, SINK)
//! ```
//!
//! Control-bus endpoints exposed to the browser:
//!
//! - subscribe `vad.out`        — per-chunk speech state
//! - subscribe `stt_in.out`     — user transcript
//! - subscribe `audio.out`      — LLM token stream + TTS audio envelopes
//! - subscribe `coordinator.out` — authoritative turn_state events
//!                                 (`turn_id`, phase, `cancelled_turn_id`,
//!                                 `error`)
//! - publish `audio.in.barge_in`     — interrupt generation (llm + kokoro_tts)
//! - publish `audio.in.reset`        — wipe chat history
//!
//! Barge-in path: VAD `speech_start` (client) →
//!   publish `audio.in.barge_in` (halts KokoroTTSNode synthesis)
//!   The server-side `coordinator` observes the same VAD event on its
//!   wired input to advance `turn_id` and gate late text from the
//!   cancelled turn.
//!
//! # Environment variables
//!
//! | Variable                  | Default                                  | Description                               |
//! |---------------------------|------------------------------------------|-------------------------------------------|
//! | `QWEN_MODEL_PATH`         | `unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL`   | GGUF model path (local or hf:// URL)      |
//! | `QWEN_SYSTEM_PROMPT`      | *(built-in voice-assistant prompt)*      | System message                            |
//! | `QWEN_CONTEXT_SIZE`       | `8192`                                   | Context window (tokens)                   |
//! | `QWEN_MAX_TOKENS`         | `2048`                                   | Max tokens per generation                 |
//! | `QWEN_GPU_OFFLOAD`        | `all`                                    | `none`, `all`, or layer count (e.g. `32`) |
//! | `QWEN_FLASH_ATTENTION`    | `true`                                   | Enable Flash Attention 2                  |
//! | `QWEN_THREADS`            | *(auto)*                                 | Computation threads (0 = auto)            |
//! | `QWEN_TTS_ENGINE`         | `kokoro`                                 | `kokoro` or `qwen`                        |
//! | `KOKORO_LANG`             | `a`                                      | Kokoro language code                      |
//! | `KOKORO_VOICE`            | `af_heart`                               | Kokoro voice preset                       |
//! | `QWEN_TTS_REPO`           | `mlx-community/Qwen3-TTS-…`              | Qwen-TTS repo (when engine=qwen)          |
//! | `QWEN_TTS_VOICE`          | `serena`                                 | Qwen-TTS voice preset                     |
//!
//! # Usage
//!
//! ```bash
//! # First, download the GGUF model (or set QWEN_MODEL_PATH to an existing file):
//! huggingface-cli download unsloth/Qwen3.6-27B-GGUF UD-Q4_K_XL.gguf \
//!     --local-dir ./models --local-dir-use-symlinks false
//!
//! QWEN_MODEL_PATH=./models/UD-Q4_K_XL.gguf \
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

fn build_manifest() -> Manifest {
    // ---- LLM (llama.cpp / GGUF) ----

    // Thinking mode is disabled via the Jinja `enable_thinking=false`
    // kwarg in the LLM node's chat template renderer (auto-fills a
    // closed `<think></think>` onto the open assistant turn). The
    // streaming `<think>...</think>` stripper in the LLM node is the
    // safety net for any reasoning that still leaks through.
    let default_system_prompt = "You are a helpful, friendly voice assistant. \
You speak conversationally and keep responses concise. \
When the user asks for code, commands, or structured data, provide it \
in markdown code blocks. Otherwise, respond in natural prose.";

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
        // Qwen3 sampling recipe (per Alibaba model card).
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

    // ---- TTS stage ----

    let tts_engine =
        std::env::var("QWEN_TTS_ENGINE").unwrap_or_else(|_| "kokoro".to_string());
    let tts_repo = std::env::var("QWEN_TTS_REPO")
        .unwrap_or_else(|_| "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-6bit".to_string());
    let tts_voice = std::env::var("QWEN_TTS_VOICE").unwrap_or_else(|_| "serena".to_string());

    let (tts_stage_nodes, tts_stage_connections) =
        build_tts_stage(&tts_engine, &tts_repo, &tts_voice);

    // ---- Nodes ----

    let mut nodes: Vec<NodeManifest> = vec![
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
        // Silero VAD — speech/activity detection on 16 kHz audio.
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
        // Whisper STT — on the main path, feeds the LLM.
        NodeManifest {
            id: "stt_in".to_string(),
            node_type: "WhisperSTTNode".to_string(),
            params: whisper_params,
            python_deps: Some(whisper_deps),
            ..Default::default()
        },
        // Llama.cpp text generation — native Rust, no Python/MLX.
        // Runs inference on a dedicated blocking thread (llama.cpp types
        // contain raw C pointers and are not Send).
        NodeManifest {
            id: "llm".to_string(),
            node_type: "LlamaCppGenerationNode".to_string(),
            params: llm_params,
            ..Default::default()
        },
        // Conversation coordinator — turn-phase state machine + sentencer.
        NodeManifest {
            id: "coordinator".to_string(),
            node_type: "ConversationCoordinatorNode".to_string(),
            params: serde_json::json!({
                "split_pattern": r"[.!?,;:\n]+",
                "min_sentence_length": 2,
                "yield_partial_on_end": true,
                // llama.cpp generation is synchronous (full response
                // returned at once), so the watchdog mainly catches
                // model-load hangs rather than mid-generation stalls.
                "llm_silence_timeout_ms": 120000,
                "user_speech_debounce_ms": 150,
                "barge_in_targets": ["llm", "kokoro_tts"],
                "first_chunk_min_chars": 25,
            }),
            ..Default::default()
        },
    ];
    nodes.extend(tts_stage_nodes);

    // ---- Connections ----

    let mut connections: Vec<Connection> = vec![
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
        // Coordinator watches VAD events for barge detection.
        Connection {
            from: "vad".to_string(),
            to: "coordinator".to_string(),
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
            to: "coordinator".to_string(),
        },
    ];

    // TTS stage connections: rewrite `llm` → `coordinator` as the
    // source so the coordinator gates cancelled-turn text.
    for c in tts_stage_connections {
        let from = if c.from == "llm" {
            "coordinator".to_string()
        } else {
            c.from
        };
        connections.push(Connection { from, to: c.to });
    }

    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "qwen-llama-s2s-webrtc".to_string(),
            description: Some(format!(
                "Speech-to-speech via llama.cpp: Whisper → Qwen3.6-27B-GGUF (LlamaCppGenerationNode) → {} \
                 over WebRTC",
                match tts_engine.as_str() {
                    "kokoro" => "KokoroTTS",
                    _ => "Qwen3-TTS",
                }
            )),
            ..Default::default()
        },
        nodes,
        connections,
        python_env: Some(ManifestPythonEnv {
            python_version: Some("3.12".to_string()),
            scope: None,
            extra_deps: Vec::new(),
        }),
    }
}

// ---------------------------------------------------------------------------
// TTS stage builder
// ---------------------------------------------------------------------------

fn build_tts_stage(
    engine: &str,
    qwen_tts_repo: &str,
    qwen_tts_voice: &str,
) -> (Vec<NodeManifest>, Vec<Connection>) {
    match engine.to_ascii_lowercase().as_str() {
        "qwen" => {
            let qwen_params = serde_json::json!({
                "hf_repo": qwen_tts_repo,
                "voice": qwen_tts_voice,
                "sample_rate": 24000,
                "output_sample_rate": 48000,
                "streaming_interval": 0.32,
                "speed": 1.0,
                "passthrough_text": true,
            });
            let qwen_deps =
                vec!["mlx-audio>=0.1".to_string(), "numpy>=1.24".to_string()];

            (
                vec![NodeManifest {
                    id: "audio".to_string(),
                    node_type: "QwenTTSMlxNode".to_string(),
                    params: qwen_params,
                    python_deps: Some(qwen_deps),
                    ..Default::default()
                }],
                vec![Connection {
                    from: "llm".to_string(),
                    to: "audio".to_string(),
                }],
            )
        }
        other_if_unknown => {
            if other_if_unknown != "kokoro" {
                eprintln!(
                    "[qwen_s2s] unknown QWEN_TTS_ENGINE={:?}; defaulting to 'kokoro'",
                    other_if_unknown
                );
            }
            let kokoro_params = serde_json::json!({
                "lang_code": std::env::var("KOKORO_LANG")
                    .unwrap_or_else(|_| "a".to_string()),
                "voice": std::env::var("KOKORO_VOICE")
                    .unwrap_or_else(|_| "af_heart".to_string()),
                "speed": 1.0,
                "sample_rate": 24000,
                "stream_chunks": true,
                "skip_tokens": [
                    "<|text_end|>", "```",
                ],
            });
            let kokoro_deps = vec![
                "kokoro>=0.9.4".to_string(),
                "soundfile".to_string(),
                "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl".to_string(),
            ];

            (
                vec![
                    NodeManifest {
                        id: "kokoro_tts".to_string(),
                        node_type: "KokoroTTSNode".to_string(),
                        params: kokoro_params,
                        python_deps: Some(kokoro_deps),
                        ..Default::default()
                    },
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
                ],
                vec![
                    Connection {
                        from: "llm".to_string(),
                        to: "kokoro_tts".to_string(),
                    },
                    Connection {
                        from: "kokoro_tts".to_string(),
                        to: "audio".to_string(),
                    },
                ],
            )
        }
    }
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
                    "qwen_s2s_webrtc_server [--host ADDR] [--port PORT]\n\n\
                     llama.cpp S2S: Whisper STT → Qwen3.6-27B-GGUF → Kokoro TTS\n\n\
                     env:\n  \
                     QWEN_MODEL_PATH        GGUF model path (default: unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL)\n  \
                     QWEN_SYSTEM_PROMPT     System message\n  \
                     QWEN_CONTEXT_SIZE      Context window in tokens (default: 8192)\n  \
                     QWEN_MAX_TOKENS        Max tokens per generation (default: 2048)\n  \
                     QWEN_GPU_OFFLOAD       GPU offload: none, all, or layer count (default: all)\n  \
                     QWEN_FLASH_ATTENTION   Enable Flash Attention 2 (default: true)\n  \
                     QWEN_THREADS           Computation threads (default: auto)\n  \
                     QWEN_TEMPERATURE       Sampling temperature (default: 0.6)\n  \
                     QWEN_TOP_P             Nucleus sampling cutoff (default: 0.8)\n  \
                     QWEN_TOP_K             Top-k sampling cutoff (default: 20)\n  \
                     QWEN_MIN_P             Min-p sampling cutoff (default: 0.0)\n  \
                     QWEN_TTS_ENGINE        TTS engine: kokoro (default) or qwen\n  \
                     KOKORO_LANG            Kokoro language code (default: a)\n  \
                     KOKORO_VOICE           Kokoro voice preset (default: af_heart)\n  \
                     QWEN_TTS_REPO          Qwen-TTS repo (when engine=qwen)\n  \
                     QWEN_TTS_VOICE         Qwen-TTS voice preset (default: serena)\n  \
                     PYTHON_ENV_MODE=managed  use uv-managed per-node venvs\n  \
                     PYTHON_VERSION=3.12      pin managed-venv Python version\n  \
                     REMOTEMEDIA_PYTHON_SRC   path to `clients/python` for editable install\n  \
                     REMOTEMEDIA_RECORD_DIR   if set, every session dumps a JSONL frame trace\n                           \
                                              to <dir>/<session_id>.jsonl. Inspect with the\n                           \
                                              `session-replay` CLI (examples/cli/session-replay)."
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

    // Per-node scheduler budgets. The LlamaCppGenerationNode loads the
    // full GGUF model on first inference call. For a 27B Q4 model (~16 GB
    // VRAM), first-call model load + warmup can take 30-60 s. Subsequent
    // calls are much faster (model stays loaded in the node instance).
    // Kokoro synthesis is chunk-streamed but benefits from extra headroom
    // for long sentences.
    exec_cfg.scheduler_config = exec_cfg
        .scheduler_config
        .with_node_timeout("llm", 300_000)
        .with_node_timeout("audio", 120_000)
        .with_node_timeout("kokoro_tts", 120_000);

    let executor = Arc::new(PipelineExecutor::with_config(exec_cfg)?);

    let config = Arc::new(WebRtcTransportConfig::default());
    let server = WebSocketSignalingServer::new(port, config, executor, manifest);
    let handle = server.start().await?;

    println!("READY ws://127.0.0.1:{port}/ws");
    println!("Pipeline:       Whisper STT → Qwen3.6-27B-GGUF (llama.cpp) → TTS");
    println!(
        "Model path:     {}",
        std::env::var("QWEN_MODEL_PATH")
            .unwrap_or_else(|_| "unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL".to_string())
    );
    println!(
        "GPU offload:    {}",
        std::env::var("QWEN_GPU_OFFLOAD").unwrap_or_else(|_| "all".to_string())
    );
    println!(
        "Context size:   {}",
        std::env::var("QWEN_CONTEXT_SIZE").unwrap_or_else(|_| "8192".to_string())
    );
    println!(
        "Max tokens:     {}",
        std::env::var("QWEN_MAX_TOKENS").unwrap_or_else(|_| "2048".to_string())
    );
    println!(
        "Temperature:    {}",
        std::env::var("QWEN_TEMPERATURE").unwrap_or_else(|_| "0.6".to_string())
    );
    println!(
        "Top-p:          {}",
        std::env::var("QWEN_TOP_P").unwrap_or_else(|_| "0.8".to_string())
    );
    println!(
        "Top-k:          {}",
        std::env::var("QWEN_TOP_K").unwrap_or_else(|_| "20".to_string())
    );
    println!(
        "Min-p:          {}",
        std::env::var("QWEN_MIN_P").unwrap_or_else(|_| "0.0".to_string())
    );
    println!(
        "TTS engine:     {}",
        std::env::var("QWEN_TTS_ENGINE").unwrap_or_else(|_| "kokoro".to_string())
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
        "PYTHON_VERSION={}",
        std::env::var("PYTHON_VERSION").unwrap_or_else(|_| "(unset)".to_string())
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
