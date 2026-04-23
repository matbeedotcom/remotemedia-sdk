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
//!   mic (48k) → resample_in (48k→16k) → chunker (512) → vad ──┬──→ accumulator
//!                                                             │
//!                                                             │      ▼
//!                                                             │   stt_in (Whisper, text)
//!                                                             │      │
//!                                                             │      ▼
//!                                                             │   llm (Qwen3.5-9B, streams tool args)
//!                                                             │      │
//!                                                             └──→ coordinator (turn phase, sentencer)
//!                                                                    │
//!                                                                    ▼
//!                                                                 audio (TTS, 24 kHz)
//!                                                                    │
//!                                                                    ▼
//!                                                                 resample_out (24→48k, SINK)
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
//! - publish `audio.in.context`     — inject knowledge text (llm node)
//! - publish `audio.in.system_prompt` — override persona (llm node)
//! - publish `audio.in.barge_in`     — interrupt generation (llm + audio)
//! - publish `audio.in.reset`        — wipe chat history
//!
//! Barge-in path (iteration 1 — client still drives fanout):
//!   VAD speech_start (client) →
//!     publish `llm.in.barge_in`  (halts QwenTextMlxNode generation)
//!     publish `audio.in.barge_in` (halts QwenTTSMlxNode synthesis)
//!     control.flush_audio        (drains server WebRTC ring buffer)
//!   In parallel, the server-side `coordinator` observes the SAME VAD
//!   event on its own wired input and (a) advances `turn_id`,
//!   (b) drops any late LLM text from the cancelled turn at the
//!   coordinator gate before it can reach TTS, and (c) publishes a
//!   `turn_state` with `cancelled_turn_id` so the UI can mark the
//!   turn as barged-in. A future iteration will move the aux-port
//!   fanout server-side into the coordinator itself and retire the
//!   client-side publishes (requires exposing `SessionControlBus` to
//!   Rust nodes).
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
        .unwrap_or_else(|_| "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-6bit".to_string());
    let tts_voice = std::env::var("QWEN_TTS_VOICE").unwrap_or_else(|_| "serena".to_string());

    let whisper_params = serde_json::json!({
        "model_id": "openai/whisper-tiny.en",
        "language": "en",
    });
    let whisper_deps = vec![
        "transformers>=4.40.0".to_string(),
        "torch>=2.1".to_string(),
        "accelerate>=0.33".to_string(),
    ];

    // Tool-call-driven output split:
    //   - ``say(text=...)`` tool calls → plain text frames to TTS
    //     (what the user HEARS).
    //   - Everything else the model emits is markdown intended for a
    //     written/display UI. That stream is currently suppressed here
    //     (`emit_display_text: false`) because this pipeline has no
    //     display-aux-port consumer wired; without suppression the
    //     envelope JSON would flow into `sentencer` → TTS and get
    //     spoken. Flip back to ``true`` once a UI client subscribes to
    //     the ``llm.out`` display envelopes.
    //
    // Tool-only output contract:
    //   - `say(text=...)`    → `channel="tts"` → sentencer → TTS → audio
    //   - `show(content=...)` → `channel="ui"` → sentencer passthrough →
    //                           TTS passthrough → WebRTC audio.out
    //   - Any free text outside a tool call is treated as `channel="ui"`
    //     fallback too, but the prompt below instructs the model to
    //     always use the tools so this path should stay empty in
    //     practice.
    //
    // Why two tools instead of "say + free-form markdown"? Qwen is trained
    // to STOP generation after a `<tool_call>` waiting for a tool-result
    // turn. If we only have `say`, any markdown the model meant to write
    // AFTER saying something gets cut off. With `show` as its own tool,
    // the model can emit BOTH tool calls in a single assistant turn (Qwen
    // does support multiple tool_calls in one generation), and we dispatch
    // each to its own routing channel without any prompt gymnastics about
    // "write the markdown first, then say".
    let llm_system_prompt = std::env::var("QWEN_LLM_SYSTEM_PROMPT").unwrap_or_else(|_| {
        "You reply to the user ONLY through tools. Every tool call MUST \
         carry its required argument — an empty call produces no output \
         and the user gets nothing.\n\n\
         Tools:\n\
         - `say(text=\"<spoken prose>\")`     → spoken aloud\n\
         - `show(content=\"<markdown>\")`     → shown on screen\n\n\
         Reply structure (emit tools in this order, include only the slots \
         your reply needs):\n\
         1. Opening `say` — short spoken lead-in. Optional.\n\
         2. `show` — the written deliverable (code, table, long text). Optional.\n\
         3. Closing `say` — short spoken wrap-up. Optional.\n\n\
         Concrete examples of well-formed replies:\n\n\
         Example — conversational answer:\n\
         User: \"How are your Python skills?\"\n\
         Assistant calls: say(text=\"Pretty solid — I can help with \
         scripts, debugging, and explanations. What did you have in mind?\")\n\n\
         Example — code request:\n\
         User: \"Write me a hello-world Python script.\"\n\
         Assistant calls, in order:\n\
           1. say(text=\"Here's a simple hello-world script.\")\n\
           2. show(content=\"```python\\ndef hello():\\n    print('Hello, \
         world!')\\n\\nhello()\\n```\")\n\
           3. say(text=\"Let me know if you'd like it tweaked.\")\n\n\
         Hard rules:\n\
         - Both tools REQUIRE their argument. Never call `say()` or \
         `show()` with no text/content — that emits silence.\n\
         - At most one opening and one closing `say`; at most one `show`.\n\
         - Never dictate code aloud in `say`. Never duplicate between the \
         spoken and written channels.\n\
         - Emit the entire reply as tool calls in a single turn. Stop \
         when done. Do not pad."
            .to_string()
    });

    let llm_params = serde_json::json!({
        "hf_repo": llm_repo,
        "max_new_tokens": 60000,
        // Qwen3 instruct / non-thinking sampling recipe, per Alibaba's
        // model card. Keep these in sync with the defaults in
        // `qwen_text_mlx.py`; overriding here makes the per-demo
        // configuration explicit instead of relying on whatever the
        // node's defaults happen to be.
        "temperature": 0.7,
        "top_p": 0.8,
        "top_k": 20,
        "min_p": 0.0,
        "presence_penalty": 1.5,
        "repetition_penalty": 1.0,
        "system_prompt": llm_system_prompt,
        "enable_say_tool": true,
        "enable_show_tool": true,
        "active_tools": ["say", "show"],
        // With dedicated tools the model shouldn't emit free text outside a
        // call, but leave the escape hatch open: if it slips and writes
        // some stray prose, it still flows through the ui channel rather
        // than hitting the TTS and getting spoken.
        "emit_display_text": true,
        // Tool-call pass budget.
        //
        // Qwen's tool-call protocol always terminates each generation
        // pass with <|im_end|> right after a tool_call, regardless of
        // whether the model intended to continue. The only unambiguous
        // "I'm done" signal is a pass that emits ZERO tool calls — and
        // that's what terminates our loop naturally.
        //
        // To cover the longest well-formed reply shape
        // (`say` → `show` → `say`, three tool calls) AND the trailing
        // zero-tool termination pass, we need at least FOUR passes.
        // Anything less cuts the loop short before the model can signal
        // completion — which is what produced the "still pending"
        // debug line when this was capped at 2.
        //
        // Ceiling is the 30 s per-node scheduler budget: at ~6 s per
        // pass that's ~5 passes max before the scheduler kills the
        // turn. 4 keeps us safely under with room for a slow pass.
        "max_tool_passes": 4,
    });
    let llm_deps = vec!["mlx-lm==0.31.3".to_string(), "numpy>=1.24".to_string()];

    // TTS engine selector. `QWEN_TTS_ENGINE=kokoro` (default) uses
    // Kokoro 82M; `QWEN_TTS_ENGINE=qwen` swaps in Qwen3-TTS. Either way
    // the terminal sink is named `audio` so the `audio.out`
    // control-bus topic and web-UI contract stay identical between
    // engines.
    //
    // Default is Kokoro because on M-series (see
    // `clients/python/bench/tts_compare.py`) Qwen3-TTS runs at RTF
    // 0.34-0.63 — slower than realtime — while Kokoro runs at RTF
    // 3.8-6.3 with ~4× lower first-audio latency. Switch to Qwen if
    // you want its voice quality and are on faster hardware.
    let tts_engine =
        std::env::var("QWEN_TTS_ENGINE").unwrap_or_else(|_| "kokoro".to_string());
    let (tts_stage_nodes, tts_stage_connections) = build_tts_stage(
        &tts_engine, &tts_repo, &tts_voice,
    );

    // Build the non-TTS nodes up front. The TTS stage (either QwenTTS
    // alone, or KokoroTTS + downstream resampler) is appended below
    // based on `QWEN_TTS_ENGINE`.
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
            // Qwen text chat with tool calling. The `say` tool is
            // active — the model is instructed (via system_prompt) to
            // emit its spoken reply as a `say(text=...)` call and keep
            // non-spoken commentary out of the call. The LLM now
            // STREAMS the `text` argument chunk-by-chunk as the model
            // generates it (parser decodes JSON escapes inline and
            // emits a flush `\n` at the closing `"`), so the downstream
            // `TextCollectorNode` can flush complete sentences to TTS
            // as soon as they're seen — cutting first-audio latency
            // from "entire tool_call wrapper" to "first sentence".
            NodeManifest {
                id: "llm".to_string(),
                node_type: "QwenTextMlxNode".to_string(),
                params: llm_params,
                python_deps: Some(llm_deps),
                ..Default::default()
            },
            // Conversation coordinator. Replaces the standalone
            // TextCollectorNode `sentencer` with a turn-aware sentencer
            // that ALSO observes `vad.out` directly. It owns the
            // authoritative turn-phase state machine: on VAD
            // `is_speech_start` it bumps `turn_id` and drops any
            // buffered tts-channel text from the cancelled turn before
            // it reaches TTS — the coordinator is the gate. Yields a
            // `turn_state` Json envelope on every phase change, tapped
            // on the control bus as `coordinator.out`. A lazy LLM
            // silence watchdog forces a clean `<|text_end|>` downstream
            // if the LLM wedges mid-turn.
            NodeManifest {
                id: "coordinator".to_string(),
                node_type: "ConversationCoordinatorNode".to_string(),
                params: serde_json::json!({
                    // Same boundary behaviour as the prior sentencer.
                    "split_pattern": r"[.!?,;:\n]+",
                    "min_sentence_length": 2,
                    "yield_partial_on_end": true,
                    // Generous watchdog: Qwen3-9B on MLX frequently
                    // takes 8-15s to emit its first token for a
                    // cold-start turn, and tool-call passes stack
                    // further latency. 60s still catches a truly
                    // wedged model while leaving ample room for a
                    // slow-but-alive one. The coordinator now also
                    // refreshes activity on any incoming LLM text
                    // BEFORE the watchdog check, so a legitimate
                    // late-arriving first token doesn't self-trip.
                    "llm_silence_timeout_ms": 60000,
                    "user_speech_debounce_ms": 150,
                }),
                ..Default::default()
            },
        ];
    nodes.extend(tts_stage_nodes);

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
            // Fan-out from vad: the coordinator watches speech_start /
            // speech_end JSON events directly (without stealing them
            // from the accumulator) so barge detection lives on the
            // server, not the browser.
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
    // The TTS stage was built assuming it receives text directly from
    // `llm`. The coordinator now sits between them and both sentence-
    // splits the tts-channel text and gates cancelled-turn frames.
    // Rewrite each connection whose source is `llm` to source from
    // `coordinator` instead — the rest of the stage (kokoro → audio,
    // or the single Qwen TTS sink) is unchanged.
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
            name: "qwen-s2s-webrtc".to_string(),
            description: Some(format!(
                "Emulated speech-to-speech: Whisper → Qwen3.5 (MLX) → {} \
                 over WebRTC, compatible with the LFM2 web UI",
                match tts_engine.as_str() {
                    "kokoro" => "KokoroTTS",
                    _ => "Qwen3-TTS",
                }
            )),
            ..Default::default()
        },
        nodes,
        connections,
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

/// Build the TTS stage (nodes + connections from `sentencer → sink`) for
/// the selected engine. The terminal sink is always named `audio` so
/// the `audio.out` control-bus topic and the WebRTC track wiring are
/// engine-agnostic — everything upstream of the sentencer, and the web
/// UI, stays identical.
///
/// ``QWEN_TTS_ENGINE``:
///   - ``qwen`` (default): Qwen3-TTS, upsamples 24→48 kHz in-node; sink
///     is the TTS node itself.
///   - ``kokoro``: KokoroTTS (82 M), emits 24 kHz; a ``FastResampleNode``
///     named ``audio`` is appended to hit 48 kHz.
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
                // Qwen3-TTS native rate. The node upsamples to
                // `output_sample_rate` (48 kHz, matching the Opus track)
                // before yielding, so no downstream resampler is needed.
                "sample_rate": 24000,
                "output_sample_rate": 48000,
                "streaming_interval": 0.32,
                "speed": 1.0,
                "passthrough_text": true,
            });
            let qwen_deps =
                vec!["mlx-audio>=0.1".to_string(), "numpy>=1.24".to_string()];

            return (
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
            );
        }
        other_if_unknown => {
            if other_if_unknown != "kokoro" {
                eprintln!(
                    "[qwen_s2s] unknown QWEN_TTS_ENGINE={:?}; defaulting to 'kokoro'",
                    other_if_unknown
                );
            }
            // Kokoro's misaki G2P needs `en_core_web_sm` inside the
            // node's managed venv — `spacy download` lands in the host
            // interpreter, which the runtime's provisioned venv can't
            // see. PEP-508 URL dep forces install into the right venv.
            let kokoro_params = serde_json::json!({
                "lang_code": std::env::var("KOKORO_LANG")
                    .unwrap_or_else(|_| "a".to_string()),
                "voice": std::env::var("KOKORO_VOICE")
                    .unwrap_or_else(|_| "af_heart".to_string()),
                "speed": 1.0,
                "sample_rate": 24000,
                "stream_chunks": true,
                "skip_tokens": [
                    "<|text_end|>", "<|audio_end|>",
                    "<|im_end|>", "<|im_start|>",
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
    let executor = Arc::new(PipelineExecutor::with_config(exec_cfg)?);

    let config = Arc::new(WebRtcTransportConfig::default());
    let server = WebSocketSignalingServer::new(port, config, executor, manifest);
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
        std::env::var("REMOTEMEDIA_PYTHON_SRC").unwrap_or_else(|_| "(unset)".to_string())
    );
    println!("Press Ctrl-C to stop.");

    tokio::signal::ctrl_c().await?;
    drop(handle);
    Ok(())
}
