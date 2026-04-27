//! OpenAI speech-to-speech WebRTC demo server.
//!
//! Replaces the locally-launched MLX/Qwen LLM in
//! `qwen_s2s_webrtc_server.rs` with a remote **OpenAI-compatible**
//! chat-completion endpoint.  Everything else — Whisper STT, Kokoro
//! TTS, VAD, turn coordination — stays identical so the same browser
//! UI (`examples/web/lfm2-audio-webrtc/`) works unchanged.
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
//!                                                             │   llm (OpenAIChatNode, streams tokens)
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
//! Control-bus endpoints exposed to the browser (same contract as
//! `qwen_s2s_webrtc_server.rs`):
//!
//! - subscribe `vad.out`        — per-chunk speech state
//! - subscribe `stt_in.out`     — user transcript
//! - subscribe `audio.out`      — LLM token stream + TTS audio envelopes
//! - subscribe `coordinator.out` — authoritative turn_state events
//!                                 (`turn_id`, phase, `cancelled_turn_id`,
//!                                 `error`)
//! - publish `audio.in.barge_in`     — interrupt generation (audio)
//! - publish `audio.in.reset`        — wipe chat history
//!
//! Barge-in path: identical to the Qwen variant.  VAD `speech_start`
//! is fan-out to `audio.in.barge_in` (client-side) and the server-side
//! `coordinator` observes the same VAD event on its wired input to
//! advance `turn_id` and gate late text from the cancelled turn.
//!
//! # Environment variables
//!
//! | Variable              | Default                                      | Description                              |
//! |-----------------------|----------------------------------------------|------------------------------------------|
//! | `OPENAI_API_KEY`      | *(required)*                                 | API key (or pass via manifest config)    |
//! | `OPENAI_BASE_URL`     | `http://127.0.0.1:8888`                      | API endpoint                             |
//! | `OPENAI_MODEL`        | `unsloth/qwen36`                             | Model identifier                         |
//! | `OPENAI_SYSTEM_PROMPT`| *(built-in conversational prompt)*           | System message                           |
//! | `OPENAI_MAX_TOKENS`   | `4096`                                       | Max tokens per turn                      |
//! | `OPENAI_TEMPERATURE`  | `0.7`                                        | Sampling temperature                     |
//! | `OPENAI_TOP_P`        | `0.8`                                        | Nucleus sampling cutoff                  |
//! | `QWEN_TTS_ENGINE`     | `kokoro`                                     | `kokoro` or `qwen`                       |
//! | `KOKORO_LANG`         | `a`                                          | Kokoro language code                     |
//! | `KOKORO_VOICE`        | `af_heart`                                   | Kokoro voice preset                      |
//! | `QWEN_TTS_REPO`       | `mlx-community/Qwen3-TTS-12Hz-1.7B-…`       | Qwen-TTS repo (when engine=qwen)         |
//! | `QWEN_TTS_VOICE`      | `serena`                                     | Qwen-TTS voice preset                    |
//!
//! # Usage
//!
//! ```bash
//! OPENAI_API_KEY=sk-… cargo run --example openai_s2s_webrtc_server \
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn parse_env_f32(key: &str) -> Option<f32> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

// ---------------------------------------------------------------------------
// Manifest builder
// ---------------------------------------------------------------------------

fn build_manifest() -> Manifest {
    // ---- LLM (OpenAI-compatible chat) ----

    // Default voice-agent system prompt that primes the model to use
    // `say()` for spoken replies and `show()` for written content.
    // Override via `OPENAI_SYSTEM_PROMPT` to disable.
    let default_system_prompt = "You are a helpful voice assistant. \
You have two tools available:\n\n\
• say(text): speak the given text aloud — use for ALL conversational \
replies, greetings, summaries, and confirmations.\n\
• show(content): display markdown to the user as written content — \
use for code blocks, tables, file paths, command output.\n\n\
You may call say() multiple times in one turn. Plain prose only \
inside say(); no markdown.";

    let llm_params = serde_json::json!({
        "api_key": std::env::var("OPENAI_API_KEY").ok(),
        "base_url": std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8888".to_string()),
        "model": std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "unsloth/qwen36:instruct".to_string()),
        "system_prompt": std::env::var("OPENAI_SYSTEM_PROMPT")
            .unwrap_or_else(|_| default_system_prompt.to_string()),
        "max_tokens": parse_env_u32("OPENAI_MAX_TOKENS").unwrap_or(4096),
        "temperature": parse_env_f32("OPENAI_TEMPERATURE").unwrap_or(0.7),
        "top_p": parse_env_f32("OPENAI_TOP_P").unwrap_or(0.8),
        "history_turns": 10,
        "streaming": true,
        "output_channel": "tts",
        // Tool calling: register the built-in `say` and `show` tools.
        // The model speaks via say() (routed to TTS) and displays
        // markdown via show() (routed to the `ui` channel, never
        // synthesised). Note: we deliberately do NOT instruct the
        // model to call say() before thinking — on reasoning models
        // that prompt actually *increases* time-to-first-audio because
        // the model spends extra <think> cycles deciding what to say
        // first. The speculative first-clause emission in the
        // coordinator handles TTFA latency instead.
        "enable_say_tool": true,
        "enable_show_tool": true,
        // `auto` lets the model emit plain content too (some servers
        // refuse `required` mode without strong tool grounding).
        // Set to `"required"` if you want to force every reply
        // through say()/show().
        "tool_choice": "auto",
    });

    // ---- Whisper STT ----

    let whisper_params = serde_json::json!({
        // tiny.en is unreliable for utterances under 2 s — switch to base.en,
        // which is the smallest variant that consistently transcribes short
        // conversational clips on CPU.
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
        // SpeculativeVADCoordinator: forwards audio to downstream
        // immediately (zero VAD-induced latency on the audio path),
        // runs Silero VAD in parallel, and emits a typed
        // `RuntimeData::ControlMessage(CancelSpeculation)` if a
        // forwarded segment turns out to be a false positive
        // (shorter than `min_speech_duration_ms`). It also emits
        // standard VAD JSON events (is_speech_start /
        // is_speech_end), consumed by SpeculativeAudioCommitNode
        // below for utterance segmentation + cancellation handling.
        NodeManifest {
            id: "vad".to_string(),
            node_type: "SpeculativeVADCoordinator".to_string(),
            params: serde_json::json!({
                "vad_threshold": 0.5,
                "sample_rate": 16000,
                "min_speech_duration_ms": 250,
                "min_silence_duration_ms": 900,
                "lookback_ms": 150,
                "speech_pad_ms": 160,
            }),
            ..Default::default()
        },
        // SpeculativeAudioCommitNode: holds the buffer in
        // PENDING_COMMIT for `commit_delay_ms` after speech_end so a
        // mid-sentence pause that resumes inside that window MERGES
        // into the same utterance instead of being split. Also
        // honours `RuntimeData::ControlMessage(CancelSpeculation)`
        // emitted by the coordinator when a forwarded segment turns
        // out to be a VAD false positive (shorter than
        // min_speech_duration_ms) — the in-flight utterance is
        // discarded before it ever reaches Whisper. The 1500 ms
        // commit delay covers natural think-pauses (see "Knuckles"
        // bug) without making true end-of-turn feel laggy.
        NodeManifest {
            id: "accumulator".to_string(),
            node_type: "SpeculativeAudioCommitNode".to_string(),
            params: serde_json::json!({
                "commit_delay_ms": 1500,
                "pre_roll_ms": 200,
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
        // OpenAI-compatible chat completion (pure Rust, no Python/MLX).
        NodeManifest {
            id: "llm".to_string(),
            node_type: "OpenAIChatNode".to_string(),
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
                // Watchdog must accommodate reasoning models. Qwen3 /
                // DeepSeek-style chains can spend 30–90 s inside the
                // `<think>` block before emitting any visible content.
                // Reasoning tokens now arrive on the `think` channel
                // and reset this timer (see OpenAIChatNode +
                // ConversationCoordinator), but we still want a
                // generous absolute ceiling for "model genuinely
                // wedged" detection.
                "llm_silence_timeout_ms": 120000,
                "user_speech_debounce_ms": 150,
                // Resumability: when VAD detects the user has resumed
                // speaking, fan-out a barge-in to these nodes so they
                // abort their in-flight work. Override the default
                // ("llm", "audio") because in this example "audio" is
                // the FastResampleNode that doesn't speak the barge
                // protocol — the actual TTS is kokoro_tts.
                "barge_in_targets": ["llm", "kokoro_tts"],
                // Speculative first-chunk emission. The first audio
                // chunk fires as soon as the LLM has streamed ~25
                // chars and reached a soft boundary (space/comma),
                // instead of waiting for the first full sentence.
                // Trades a little prosody on the opening clause for
                // ~300–600 ms lower time-to-first-audio.
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
            name: "openai-s2s-webrtc".to_string(),
            description: Some(format!(
                "Speech-to-speech via OpenAI-compatible API: Whisper → OpenAIChatNode → {} \
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
// TTS stage builder (shared with qwen_s2s_webrtc_server.rs)
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
                    "[openai_s2s] unknown QWEN_TTS_ENGINE={:?}; defaulting to 'kokoro'",
                    other_if_unknown
                );
            }
            let kokoro_skip_tokens: Vec<String> = vec![
                "<|text_end|>".to_string(),
                "```".to_string(),
                "```python".to_string(),
                "```rust".to_string(),
                "```javascript".to_string(),
                "```typescript".to_string(),
                "```bash".to_string(),
                "```json".to_string(),
                "```yaml".to_string(),
                "```xml".to_string(),
                "```css".to_string(),
                "```html".to_string(),
                "```sql".to_string(),
                "```markdown".to_string(),
                "```text".to_string(),
                "```csv".to_string(),
                "```toml".to_string(),
                "```lua".to_string(),
                "```go".to_string(),
                "```c".to_string(),
                "```cpp".to_string(),
                "```java".to_string(),
                "```kotlin".to_string(),
                "```swift".to_string(),
                "```dart".to_string(),
                "```r".to_string(),
                "```perl".to_string(),
                "```php".to_string(),
                "```ruby".to_string(),
                "```scala".to_string(),
                "```shell".to_string(),
                "```powershell".to_string(),
                "```batch".to_string(),
                "```vb".to_string(),
                "```fortran".to_string(),
                "```haskell".to_string(),
                "```elixir".to_string(),
                "```erlang".to_string(),
                "```clojure".to_string(),
                "```fsharp".to_string(),
                "```julia".to_string(),
                "```objective-c".to_string(),
                "```prolog".to_string(),
                "```racket".to_string(),
                "```reason".to_string(),
                "```scheme".to_string(),
                "```solidity".to_string(),
                "```vue".to_string(),
                "```svelte".to_string(),
                "```astro".to_string(),
                "```graphql".to_string(),
                "```dockerfile".to_string(),
                "```makefile".to_string(),
                "```cmake".to_string(),
                "```nginx".to_string(),
                "```apache".to_string(),
                "```terraform".to_string(),
                "```hcl".to_string(),
                "```coffeescript".to_string(),
                "```less".to_string(),
                "```scss".to_string(),
                "```stylus".to_string(),
                "```diff".to_string(),
                "```ini".to_string(),
                "```properties".to_string(),
                "```regex".to_string(),
                "```x86asm".to_string(),
                "```armasm".to_string(),
                "```avrasm".to_string(),
                "```basic".to_string(),
                "```bluespec".to_string(),
                "```csharp".to_string(),
                "```crystal".to_string(),
                "```d".to_string(),
                "```docker".to_string(),
                "```elm".to_string(),
                "```fish".to_string(),
                "```glsl".to_string(),
                "```gnuplot".to_string(),
                "```haml".to_string(),
                "```j".to_string(),
                "```lisp".to_string(),
                "```logtalk".to_string(),
                "```ml".to_string(),
                "```nasm".to_string(),
                "```ocaml".to_string(),
                "```pascal".to_string(),
                "```pony".to_string(),
                "```pure".to_string(),
                "```sas".to_string(),
                "```smalltalk".to_string(),
                "```tcl".to_string(),
                "```v".to_string(),
                "```zig".to_string(),
            ];

            let kokoro_params = serde_json::json!({
                "lang_code": std::env::var("KOKORO_LANG")
                    .unwrap_or_else(|_| "a".to_string()),
                "voice": std::env::var("KOKORO_VOICE")
                    .unwrap_or_else(|_| "af_heart".to_string()),
                "speed": 1.0,
                "sample_rate": 24000,
                "stream_chunks": true,
                "skip_tokens": kokoro_skip_tokens,
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
                    "openai_s2s_webrtc_server [--host ADDR] [--port PORT]\n\n\
                     env:\n  \
                     OPENAI_API_KEY         OpenAI API key (required)\n  \
                     OPENAI_BASE_URL        API endpoint (default: http://127.0.0.1:8888)\n  \
                     OPENAI_MODEL           Model id (default: unsloth/qwen36)\n  \
                     OPENAI_SYSTEM_PROMPT   System message\n  \
                     OPENAI_MAX_TOKENS      Max tokens per turn (default: 4096)\n  \
                     OPENAI_TEMPERATURE     Sampling temperature (default: 0.7)\n  \
                     OPENAI_TOP_P           Nucleus sampling cutoff (default: 0.8)\n  \
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

    // Per-node scheduler budgets. Reasoning models (Qwen3,
    // DeepSeek-style) routinely spend 60–120 s inside a `<think>`
    // block before emitting visible content. The `llm` node's whole
    // streaming call must fit inside its scheduler timeout, so this
    // has to be larger than `llm_silence_timeout_ms` AND larger than
    // any plausible reasoning + reply duration. 300 s gives a
    // comfortable ceiling.
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
    println!("Pipeline:       Whisper STT → OpenAIChatNode → TTS");
    println!(
        "OpenAI model:   {}",
        std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "unsloth/qwen36".to_string())
    );
    println!(
        "OpenAI base:    {}",
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| {
            "http://127.0.0.1:8888".to_string()
        })
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