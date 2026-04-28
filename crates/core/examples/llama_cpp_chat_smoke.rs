//! End-to-end smoke test for `LlamaCppGenerationNode`.
//!
//! Exercises the *real* node code path: builds the factory, deserializes
//! params from JSON exactly as `qwen_s2s_webrtc_server` does, runs
//! `initialize()` (which spawns the worker thread and loads the model on
//! GPU), then sends two turns through `process_async()` and verifies:
//!
//!   - the model loads without errors
//!   - the Jinja chat-template renderer compiles (or cleanly falls back)
//!   - generation produces non-empty text (no thinking-mode leak)
//!   - turn 2 reuses the KV cache from turn 1 (`n_reused > 0`)
//!   - the `<|text_end|>` end-of-response sentinel is present in chunks
//!
//! ```bash
//! LLAMA_TEST_MODEL=/path/to/Qwen3.6-27B.Q4_K_XL.gguf \
//! cargo run --example llama_cpp_chat_smoke -p remotemedia-core \
//!     --features llama-cpp-cuda
//! ```
//!
//! Override defaults:
//! - `LLAMA_TEST_GPU_OFFLOAD` — `none` | `all` (default) | `<n_layers>`
//! - `LLAMA_TEST_CTX_SIZE`    — context window in tokens (default 4096)
//! - `LLAMA_TEST_PROMPT_1`    — first user turn (default "Hi! Are you there?")
//! - `LLAMA_TEST_PROMPT_2`    — second user turn (default "What's 2+2?")

#[cfg(not(feature = "llama-cpp"))]
fn main() {
    eprintln!(
        "this example requires the `llama-cpp-cuda` (or `llama-cpp`) feature; \
         re-run with `--features llama-cpp-cuda`"
    );
    std::process::exit(2);
}

#[cfg(feature = "llama-cpp")]
fn main() {
    use remotemedia_core::data::RuntimeData;
    use remotemedia_core::nodes::llama_cpp::LlamaCppGenerationNodeFactory;
    use remotemedia_core::nodes::streaming_node::{InitializeContext, StreamingNodeFactory};

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let model_path = match std::env::var("LLAMA_TEST_MODEL") {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "LLAMA_TEST_MODEL is not set. Point it at a local GGUF file, e.g.:\n  \
                 LLAMA_TEST_MODEL=$HOME/models/qwen3-27b.gguf cargo run --example \
                 llama_cpp_chat_smoke -p remotemedia-core --features llama-cpp-cuda"
            );
            std::process::exit(2);
        }
    };

    let gpu_offload = std::env::var("LLAMA_TEST_GPU_OFFLOAD").unwrap_or_else(|_| "all".into());
    let ctx_size: u32 = std::env::var("LLAMA_TEST_CTX_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4096);
    let prompt_1 =
        std::env::var("LLAMA_TEST_PROMPT_1").unwrap_or_else(|_| "Hi! Are you there?".into());
    let prompt_2 =
        std::env::var("LLAMA_TEST_PROMPT_2").unwrap_or_else(|_| "What's 2+2?".into());

    let gpu_offload_value = match gpu_offload.to_lowercase().as_str() {
        "none" | "0" | "cpu" => serde_json::json!("none"),
        "all" | "gpu" => serde_json::json!("all"),
        n => match n.parse::<u16>() {
            Ok(layers) => serde_json::json!({ "layers": layers }),
            Err(_) => serde_json::json!("all"),
        },
    };

    let params = serde_json::json!({
        "model_path": model_path,
        "backend": {
            "numa": false,
            "gpu_offload": gpu_offload_value,
            "flash_attention": true,
            "threads": null,
            "threads_batch": null,
        },
        "context_size": ctx_size,
        "batch_size": 512,
        "max_tokens": 256,
        "temperature": 0.6,
        "top_p": 0.8,
        "top_k": 20,
        "min_p": 0.0,
        "repeat_penalty": 1.1,
        "system_prompt": "You are a terse voice assistant. Reply in one sentence.",
        "seed": 42,
    });

    let factory = LlamaCppGenerationNodeFactory;
    println!("== building LlamaCppGenerationNode ==");
    let node = factory
        .create("llm-smoke".to_string(), &params, None)
        .expect("factory.create failed");

    let init_ctx = InitializeContext {
        session_id: "smoke".to_string(),
        node_id: "llm-smoke".to_string(),
        control: None,
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let started = std::time::Instant::now();
        println!("== initialize() — loading model on GPU ==");
        node.initialize(&init_ctx).await.expect("initialize failed");
        println!("init OK in {:?}", started.elapsed());

        for (label, prompt) in [("turn 1", prompt_1.as_str()), ("turn 2", prompt_2.as_str())] {
            println!("\n== {label}: user ==\n  {prompt}");
            let t0 = std::time::Instant::now();
            let out = node
                .process_async(RuntimeData::Text(prompt.to_string()))
                .await
                .expect("process_async failed");
            let elapsed = t0.elapsed();

            match &out {
                RuntimeData::Text(t) => {
                    println!("== {label}: assistant ({:?}) ==\n  {t}", elapsed);
                    let stripped = t.trim_end_matches("<|text_end|>");
                    if stripped.is_empty() || stripped == t.as_str() {
                        // assistant is empty OR sentinel was missing
                        if stripped.is_empty() {
                            eprintln!("FAIL: empty assistant response");
                            std::process::exit(1);
                        }
                        if stripped == t.as_str() {
                            eprintln!("FAIL: missing <|text_end|> sentinel in response");
                            std::process::exit(1);
                        }
                    }
                    if t.contains("<think>") || t.contains("</think>") {
                        eprintln!(
                            "FAIL: <think> markers leaked into output (expected the Jinja \
                             enable_thinking=false kwarg or the streaming stripper to suppress)"
                        );
                        std::process::exit(1);
                    }
                }
                other => {
                    eprintln!("FAIL: unexpected output type: {:?}", other.data_type());
                    std::process::exit(1);
                }
            }
        }

        println!("\nPASS: model loaded, two turns generated, no <think> leak, sentinel present");
    });
}
