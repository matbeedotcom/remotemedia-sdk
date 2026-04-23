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
        "temperature": 0.7,
        "top_p": 0.9,
        "system_prompt": llm_system_prompt,
        "enable_say_tool": true,
        "enable_show_tool": true,
        "active_tools": ["say", "show"],
        // With dedicated tools the model shouldn't emit free text outside a
        // call, but leave the escape hatch open: if it slips and writes
        // some stray prose, it still flows through the ui channel rather
        // than hitting the TTS and getting spoken.
        "emit_display_text": true,
        // Two passes.
        //
        // Qwen can emit multiple tool calls (e.g. `say` → `show` → `say`)
        // in one assistant turn when the template includes them — that's
        // the happy path and stays single-pass. But some turns end after
        // just the opening `say`, in which case pass 2 gives the model a
        // chance to continue with `show` + closing `say`. Tool results
        // for the emitted calls are injected as empty `{role:"tool"}`
        // turns before pass 2 starts (see `max_tool_passes` docs in
        // qwen_text_mlx.py).
        //
        // Kept at 2 (not higher) because each extra pass is ~6 s of
        // latency AND adds audible gaps between fragments; the 30 s
        // per-node scheduler budget also caps us somewhere around 4.
        "max_tool_passes": 2,
    });
    let llm_deps = vec!["mlx-lm==0.31.3".to_string(), "numpy>=1.24".to_string()];

    // KokoroTTS experiment — swapping QwenTTS to see if the Kokoro
    // 82 M model produces audio with lower first-audio latency than
    // Qwen3-TTS. Kokoro is a 24 kHz mono synth; we add a downstream
    // FastResampleNode (renamed to `audio` to keep the `audio.out`
    // control-bus topic contract) to reach the Opus track's 48 kHz.
    //
    // Trade-offs versus Qwen3-TTS:
    //   - Kokoro doesn't emit `<|audio_end|>` or pass text through;
    //     half-duplex mic-gating and the live transcript tap on
    //     `audio.out` will be degraded until we wire those back in.
    //   - Kokoro doesn't honour the `audio.in.barge_in` aux port.
    //   - Voice set is Kokoro's (`af_heart`, `am_*`, etc.), not
    //     Qwen3-TTS's (`serena` et al.).
    let _ = tts_repo; // unused by Kokoro; kept for easy swap-back.
    let _ = tts_voice;
    let tts_params = serde_json::json!({
        "lang_code": std::env::var("KOKORO_LANG").unwrap_or_else(|_| "a".to_string()),
        "voice": std::env::var("KOKORO_VOICE").unwrap_or_else(|_| "af_heart".to_string()),
        "speed": 1.0,
        "sample_rate": 24000,
        "stream_chunks": true,
        // Sentinels from the LLM/TextCollector stream that should not be
        // read aloud.
        "skip_tokens": [
            "<|text_end|>", "<|audio_end|>",
            "<|im_end|>", "<|im_start|>",
        ],
    });
    // Kokoro's misaki G2P pulls spaCy and needs the `en_core_web_sm`
    // model present inside the SAME managed venv. Declaring it as a
    // PEP-508 URL dep forces the runtime's venv provisioner to install
    // it into the node's interpreter; the default `spacy download`
    // command lands in whatever interpreter `sys.executable` points at
    // (often the user's global 3.10.9) and Kokoro's 3.12 venv stays
    // empty — which is what the E050 crash was.
    let tts_deps = vec![
        "kokoro>=0.9.4".to_string(),
        "soundfile".to_string(),
        "en-core-web-sm @ https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl".to_string(),
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
            // Qwen text chat with tool calling. The `say` tool is
            // active — the model is instructed (via system_prompt) to
            // emit its spoken reply as a `say(text=...)` call and keep
            // non-spoken commentary out of the call. Only the `text`
            // argument of a `say` call flows downstream as plain
            // `RuntimeData.text`, which the sentencer collapses into
            // sentences before TTS. Display-channel text is suppressed
            // here (`emit_display_text: false` in `llm_params`) until a
            // UI subscriber for the display envelopes exists.
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
            // KokoroTTS synthesises at 24 kHz mono. A downstream
            // resampler (named `audio`) upsamples to 48 kHz so the
            // `audio.out` topic contract and the Opus track both see
            // the expected rate. If the session router ends up
            // batching the Kokoro per-chunk yields into the resampler
            // (the concern documented against Qwen3-TTS's internal
            // upsampler) this topology will reveal it as all-chunks-
            // at-end timing — worth measuring before reaching back for
            // an in-node upsampler.
            NodeManifest {
                id: "kokoro_tts".to_string(),
                node_type: "KokoroTTSNode".to_string(),
                params: tts_params,
                python_deps: Some(tts_deps),
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
                to: "kokoro_tts".to_string(),
            },
            Connection {
                from: "kokoro_tts".to_string(),
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
