//! LlamaCppGenerationNode — text generation via llama.cpp
//!
//! Accepts `RuntimeData::Text` (user prompt) or `RuntimeData::Json`
//! (structured messages) and streams generated tokens downstream.
//!
//! # Architecture
//!
//! llama.cpp types (`LlamaBackend`, `LlamaModel`, `LlamaContext`,
//! `LlamaBatch`) contain raw C pointers and are not `Send`. They cannot
//! cross tokio task boundaries.
//!
//! To get a single load + reuse on every turn, this node spawns a
//! dedicated `std::thread` during `initialize()`. That thread:
//!   1. Initializes the llama backend (registers ggml-cuda etc.)
//!   2. Loads the GGUF model with the configured GPU-offload setting
//!   3. Creates a long-lived `LlamaContext`
//!   4. Sits in a request loop, decoding prompts as they arrive
//!
//! Async callers send `(prompt, oneshot::Sender<chunks>)` over an
//! `mpsc::Sender<WorkerRequest>`. `initialize()` blocks the pipeline's
//! readiness signal on a successful model load — so when the frontend
//! sees `"ready"`, weights really are on the GPU.

use crate::data::RuntimeData;
use crate::error::Error;
use crate::nodes::streaming_node::{
    AsyncStreamingNode, InitializeContext, StreamingNode, StreamingNodeFactory,
};
use serde_json::Value;
use std::sync::{Arc, OnceLock};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};

use super::config::LlamaCppGenerationConfig;

#[cfg(feature = "llama-cpp")]
enum WorkerRequest {
    Generate {
        prompt: String,
        result_tx: oneshot::Sender<Result<Vec<String>, Error>>,
    },
}

/// Llama.cpp text generation node.
pub struct LlamaCppGenerationNode {
    node_id: String,
    config: LlamaCppGenerationConfig,
    #[cfg(feature = "llama-cpp")]
    worker_tx: OnceLock<mpsc::Sender<WorkerRequest>>,
}

impl LlamaCppGenerationNode {
    /// Create a new generation node.
    pub fn new(node_id: impl Into<String>, config: &LlamaCppGenerationConfig) -> Result<Self, Error> {
        config.validate().map_err(|e| Error::Execution(format!("Invalid config: {}", e)))?;

        Ok(Self {
            node_id: node_id.into(),
            config: config.clone(),
            #[cfg(feature = "llama-cpp")]
            worker_tx: OnceLock::new(),
        })
    }

    /// Create from JSON parameters.
    pub fn from_params(node_id: impl Into<String>, params: &Value) -> Result<Self, Error> {
        let config: LlamaCppGenerationConfig = serde_json::from_value(params.clone())
            .map_err(|e| Error::Execution(format!("Invalid config JSON: {}", e)))?;
        Self::new(node_id, &config)
    }

    /// Send a generation request to the worker thread and await all
    /// produced chunks.
    #[cfg(feature = "llama-cpp")]
    async fn generate(&self, prompt: &str) -> Result<Vec<String>, Error> {
        let tx = self
            .worker_tx
            .get()
            .ok_or_else(|| Error::Execution(
                "LlamaCppGenerationNode worker not running — initialize() was \
                 not called or model load failed".into()))?;

        let (result_tx, result_rx) = oneshot::channel();
        tx.send(WorkerRequest::Generate {
            prompt: prompt.to_string(),
            result_tx,
        })
        .await
        .map_err(|_| Error::Execution("LlamaCpp worker thread is gone".into()))?;

        result_rx
            .await
            .map_err(|_| Error::Execution("LlamaCpp worker dropped result channel".into()))?
    }

    #[cfg(not(feature = "llama-cpp"))]
    async fn generate(&self, prompt: &str) -> Result<Vec<String>, Error> {
        Ok(vec![format!(
            "[llama-cpp disabled: {}]",
            &prompt[..prompt.len().min(30)]
        )])
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for LlamaCppGenerationNode {
    fn node_type(&self) -> &str {
        "LlamaCppGenerationNode"
    }

    async fn initialize(&self, ctx: &InitializeContext) -> Result<(), Error> {
        info!(
            node = %self.node_id,
            model = %self.config.model_path,
            context_size = self.config.context_size,
            "Initializing LlamaCppGenerationNode"
        );

        ctx.emit_progress(
            "loading_model",
            &format!("Loading model: {}", self.config.model_path),
        );

        #[cfg(feature = "llama-cpp")]
        {
            // Spawn the dedicated inference thread. It does the
            // backend/model/ctx load synchronously and reports the
            // result via `init_tx` — we await that so `initialize()`
            // doesn't return success until weights are on the GPU.
            let (req_tx, req_rx) = mpsc::channel::<WorkerRequest>(8);
            let (init_tx, init_rx) =
                oneshot::channel::<Result<(), Error>>();

            let config = self.config.clone();
            let node_id = self.node_id.clone();
            std::thread::Builder::new()
                .name(format!("llama-cpp-gen[{}]", self.node_id))
                .spawn(move || {
                    worker_main(node_id, config, req_rx, init_tx)
                })
                .map_err(|e| Error::Execution(
                    format!("Failed to spawn llama.cpp worker thread: {}", e)))?;

            // Wait for the worker to finish its load (success or error).
            init_rx
                .await
                .map_err(|_| Error::Execution(
                    "llama.cpp worker exited before reporting init result".into()))??;

            // Stash the request channel for `generate()`.
            if self.worker_tx.set(req_tx).is_err() {
                return Err(Error::Execution(
                    "LlamaCppGenerationNode worker channel already set \
                     (initialize() called twice?)".into()));
            }
        }

        ctx.emit_progress("ready", "LlamaCppGenerationNode ready");
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        let prompt = match &data {
            RuntimeData::Text(text) => text.clone(),
            RuntimeData::Json(value) => value
                .get("prompt")
                .or(value.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string()),
            other => {
                return Err(Error::Execution(format!(
                    "LlamaCppGenerationNode accepts Text or Json, got {}",
                    other.data_type()
                )));
            }
        };

        let chunks = self.generate(&prompt).await?;
        Ok(RuntimeData::Text(chunks.join("")))
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        _session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        let prompt = match &data {
            RuntimeData::Text(text) => text.clone(),
            RuntimeData::Json(value) => value
                .get("prompt")
                .or(value.get("text"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| value.to_string()),
            other => {
                return Err(Error::Execution(format!(
                    "LlamaCppGenerationNode accepts Text or Json, got {}",
                    other.data_type()
                )));
            }
        };

        let chunks = self.generate(&prompt).await?;

        let mut count = 0;
        for chunk in chunks {
            callback(RuntimeData::Text(chunk))?;
            count += 1;
        }

        Ok(count)
    }

    async fn process_control_message(
        &self,
        message: RuntimeData,
        _session_id: Option<String>,
    ) -> Result<bool, Error> {
        if let RuntimeData::ControlMessage { message_type, .. } = &message {
            match message_type {
                crate::data::ControlMessageType::CancelSpeculation { .. } => {
                    debug!("Received cancel speculation message for LlamaCpp generation");
                    return Ok(true);
                }
                _ => {}
            }
        }
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Worker thread (`!Send` llama.cpp objects live here)
// ---------------------------------------------------------------------------

#[cfg(feature = "llama-cpp")]
fn install_llama_log_filter() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        unsafe extern "C" fn filter(
            level: llama_cpp_sys_4::ggml_log_level,
            text: *const std::os::raw::c_char,
            _user_data: *mut std::os::raw::c_void,
        ) {
            // Drop everything below WARN. WARN/ERROR get forwarded to
            // stderr verbatim (llama.cpp's text already includes a
            // trailing newline).
            if level >= llama_cpp_sys_4::GGML_LOG_LEVEL_WARN && !text.is_null() {
                let s = std::ffi::CStr::from_ptr(text).to_string_lossy();
                eprint!("{}", s);
            }
        }
        unsafe {
            llama_cpp_sys_4::llama_log_set(Some(filter), std::ptr::null_mut());
        }
    });
}

#[cfg(feature = "llama-cpp")]
fn worker_main(
    node_id: String,
    config: LlamaCppGenerationConfig,
    mut req_rx: mpsc::Receiver<WorkerRequest>,
    init_tx: oneshot::Sender<Result<(), Error>>,
) {
    use llama_cpp_4::llama_backend::LlamaBackend;
    use llama_cpp_4::model::params::LlamaModelParams;
    use llama_cpp_4::model::LlamaModel;
    use std::time::Instant;
    use super::config::GpuOffload;

    let started = Instant::now();
    info!(node = %node_id, "llama.cpp worker: initializing backend");
    let backend = match LlamaBackend::init() {
        Ok(b) => b,
        Err(e) => {
            let err = Error::Execution(format!("LlamaBackend::init failed: {}", e));
            error!(node = %node_id, "{}", err);
            let _ = init_tx.send(Err(err));
            return;
        }
    };

    // Filter llama.cpp / ggml stderr spam. Their default logger emits a
    // line per CUDA graph reuse, per kv-cache resize, etc. — none of which
    // is useful in a streaming voice pipeline. We forward only WARN/ERROR;
    // our own tracing logs cover load progress and generation events.
    // `llama_log_set` is process-global, so installing once is sufficient.
    install_llama_log_filter();

    let n_gpu_layers = match config.backend.gpu_offload {
        GpuOffload::None => 0,
        GpuOffload::All => 1000,
        GpuOffload::Layers(n) => n as u32,
    };
    info!(
        node = %node_id,
        model = %config.model_path,
        n_gpu_layers,
        "llama.cpp worker: loading model (this may take 30-60 s for large GGUF files)"
    );
    let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);

    let model = match LlamaModel::load_from_file(&backend, &config.model_path, &model_params) {
        Ok(m) => m,
        Err(e) => {
            let err = Error::Execution(format!("Model load failed: {}", e));
            error!(node = %node_id, "{}", err);
            let _ = init_tx.send(Err(err));
            return;
        }
    };
    info!(
        node = %node_id,
        load_ms = started.elapsed().as_millis() as u64,
        "llama.cpp worker: model loaded"
    );

    let ctx_started = Instant::now();
    info!(
        node = %node_id,
        n_ctx = config.context_size,
        n_batch = config.batch_size,
        flash_attention = config.backend.flash_attention,
        "llama.cpp worker: creating context"
    );

    let mut ctx_params = llama_cpp_4::context::params::LlamaContextParams::default();
    ctx_params = ctx_params.with_n_ctx(std::num::NonZeroU32::new(config.context_size));
    ctx_params = ctx_params.with_n_batch(config.batch_size);
    if config.backend.flash_attention {
        ctx_params = ctx_params.with_flash_attention(true);
    }
    if let Some(threads) = config.backend.threads {
        ctx_params = ctx_params.with_n_threads(threads as i32);
    }

    let mut llama_ctx = match model.new_context(&backend, ctx_params) {
        Ok(c) => c,
        Err(e) => {
            let err = Error::Execution(format!("Context creation failed: {}", e));
            error!(node = %node_id, "{}", err);
            let _ = init_tx.send(Err(err));
            return;
        }
    };
    info!(
        node = %node_id,
        ctx_ms = ctx_started.elapsed().as_millis() as u64,
        total_ms = started.elapsed().as_millis() as u64,
        "llama.cpp worker: ready for inference"
    );

    // Persistent chat state: keep the message history so multi-turn
    // context survives, but re-decode the full conversation each turn.
    //
    // We previously tried cross-turn KV reuse (truncate via
    // `kv_cache_seq_rm` to the longest common prefix and only decode the
    // diff). That trips an M-RoPE invariant in Qwen3-VL-class models
    // ("X < Y" position check) because `seq_rm` doesn't reset the memory
    // module's max-position tracker — the next decode then refuses to
    // place new tokens before the stored max. Until we have a model-aware
    // detector, full clear + full prefill each turn is the only safe path
    // that works for both standard-RoPE and M-RoPE checkpoints.
    let mut chat = ChatState::new();
    if let Some(sys) = config.system_prompt.as_ref().filter(|s| !s.is_empty()) {
        match llama_cpp_4::model::LlamaChatMessage::new(
            "system".to_string(),
            sys.clone(),
        ) {
            Ok(m) => chat.messages.push(m),
            Err(e) => {
                error!(node = %node_id, "invalid system prompt: {}", e);
            }
        }
    }

    if init_tx.send(Ok(())).is_err() {
        // Caller dropped before we finished; just exit.
        return;
    }

    while let Some(req) = req_rx.blocking_recv() {
        match req {
            WorkerRequest::Generate { prompt, result_tx } => {
                let t0 = Instant::now();
                debug!(
                    node = %node_id,
                    prompt_len = prompt.len(),
                    history_messages = chat.messages.len(),
                    "llama.cpp worker: generation request"
                );
                let result = run_turn_incremental(
                    &mut chat,
                    &model,
                    &mut llama_ctx,
                    &config,
                    &prompt,
                );
                match &result {
                    Ok((chunks, stats)) => {
                        let total_chars: usize = chunks.iter().map(|c| c.len()).sum();
                        info!(
                            node = %node_id,
                            n_chunks = chunks.len(),
                            n_chars = total_chars,
                            n_decoded = stats.n_decoded,
                            n_reused = stats.n_reused,
                            elapsed_ms = t0.elapsed().as_millis() as u64,
                            "llama.cpp worker: generation complete"
                        );
                    }
                    Err(e) => {
                        error!(node = %node_id, "llama.cpp worker: generation failed: {}", e);
                    }
                }
                let _ = result_tx.send(result.map(|(chunks, _)| chunks));
            }
        }
    }

    info!(node = %node_id, "llama.cpp worker: channel closed, shutting down");
    drop(llama_ctx);
    drop(model);
    drop(backend);
}

#[cfg(feature = "llama-cpp")]
struct ChatState {
    messages: Vec<llama_cpp_4::model::LlamaChatMessage>,
}

#[cfg(feature = "llama-cpp")]
impl ChatState {
    fn new() -> Self {
        Self { messages: Vec::new() }
    }
}

/// Streaming filter that strips `<think>...</think>` blocks emitted by
/// reasoning-mode models (Qwen3, etc.). Tag-aware across token boundaries:
/// holds back any trailing bytes that could be a partial opening tag and
/// suppresses everything between `<think>` and `</think>` inclusive.
#[cfg(feature = "llama-cpp")]
struct ThinkStripper {
    in_think: bool,
    /// Pending bytes we haven't decided whether to emit yet (potential
    /// partial-tag prefix).
    buffer: String,
}

#[cfg(feature = "llama-cpp")]
impl ThinkStripper {
    const OPEN: &'static str = "<think>";
    const CLOSE: &'static str = "</think>";

    fn new() -> Self {
        Self {
            in_think: false,
            buffer: String::new(),
        }
    }

    /// Feed one decoded piece. Returns the chunk that's safe to emit
    /// (or `None` if everything was held back / dropped).
    fn push(&mut self, piece: &str) -> Option<String> {
        if piece.is_empty() {
            return None;
        }
        self.buffer.push_str(piece);

        let mut out = String::new();
        loop {
            if self.in_think {
                if let Some(idx) = self.buffer.find(Self::CLOSE) {
                    self.buffer.drain(..idx + Self::CLOSE.len());
                    self.in_think = false;
                    continue;
                }
                // Keep just enough trailing bytes to recognise a split CLOSE
                // tag on the next push; drop the rest.
                let keep = Self::CLOSE.len().saturating_sub(1);
                if self.buffer.len() > keep {
                    let drop_to = self.buffer.len() - keep;
                    // Only drain on a char boundary.
                    let drop_to = (0..=drop_to)
                        .rev()
                        .find(|&i| self.buffer.is_char_boundary(i))
                        .unwrap_or(0);
                    self.buffer.drain(..drop_to);
                }
                break;
            }

            if let Some(idx) = self.buffer.find(Self::OPEN) {
                out.push_str(&self.buffer[..idx]);
                self.buffer.drain(..idx + Self::OPEN.len());
                self.in_think = true;
                continue;
            }

            // No OPEN tag in buffer — emit everything except the trailing
            // bytes that could form a partial OPEN tag on the next call.
            let safe_end = self.buffer.len().saturating_sub(Self::OPEN.len() - 1);
            let safe_end = (0..=safe_end)
                .rev()
                .find(|&i| self.buffer.is_char_boundary(i))
                .unwrap_or(0);
            if safe_end > 0 {
                let head: String = self.buffer.drain(..safe_end).collect();
                out.push_str(&head);
            }
            break;
        }

        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// End-of-stream flush. If we're still inside a `<think>` block (no
    /// closing tag was ever seen), drop everything. Otherwise emit the
    /// remaining buffer verbatim.
    fn flush(&mut self) -> Option<String> {
        if self.in_think {
            self.buffer.clear();
            return None;
        }
        if self.buffer.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut self.buffer))
    }
}

#[cfg(feature = "llama-cpp")]
struct TurnStats {
    /// Tokens decoded for the prompt portion of this turn (system +
    /// history + new user + assistant opener). Always equals the full
    /// formatted conversation length under the safe full-prefill design.
    n_decoded: usize,
    /// Tokens reused from the prior turn's KV cache. Always 0 today —
    /// kept in the struct so we can re-enable cross-turn reuse later
    /// without changing the log schema.
    n_reused: usize,
}

/// Run one chat turn. Always clears the KV cache and re-decodes the full
/// formatted conversation, then samples the assistant response. Pushes
/// the user + assistant messages into `state.messages` so the next turn
/// sees a growing context.
#[cfg(feature = "llama-cpp")]
fn run_turn_incremental(
    state: &mut ChatState,
    model: &llama_cpp_4::model::LlamaModel,
    ctx: &mut llama_cpp_4::context::LlamaContext,
    config: &LlamaCppGenerationConfig,
    user_text: &str,
) -> Result<(Vec<String>, TurnStats), Error> {
    use llama_cpp_4::llama_batch::LlamaBatch;
    use llama_cpp_4::model::{AddBos, LlamaChatMessage, Special};
    use llama_cpp_4::sampling::LlamaSampler;

    // 1. Compose [history + new user] and apply the chat template so the
    //    model sees a properly framed turn (otherwise chat-tuned models
    //    emit EOS immediately).
    let user_msg = LlamaChatMessage::new("user".to_string(), user_text.to_string())
        .map_err(|e| Error::Execution(format!("invalid user msg: {}", e)))?;
    let mut probe_messages = state.messages.clone();
    probe_messages.push(user_msg);

    let mut formatted = model
        .apply_chat_template(None, &probe_messages, true)
        .map_err(|e| Error::Execution(format!("chat template apply: {}", e)))?;

    // Prefill an empty thinking block onto the assistant turn so
    // reasoning-mode models (Qwen3 etc.) skip chain-of-thought entirely
    // and go straight to the answer. This is what `enable_thinking=false`
    // does in the Jinja-side template — we just append the same tokens
    // here since the simple `apply_chat_template` C API doesn't take
    // template kwargs.
    formatted.push_str("<think></think>\n\n");

    let prompt_tokens = model
        .str_to_token(&formatted, AddBos::Always)
        .map_err(|e| Error::Execution(format!("tokenize: {}", e)))?;
    let n_prompt = prompt_tokens.len();
    if n_prompt == 0 {
        return Ok((
            Vec::new(),
            TurnStats {
                n_decoded: 0,
                n_reused: 0,
            },
        ));
    }

    // 2. Reset the cache. Required for M-RoPE models because
    //    `kv_cache_seq_rm` doesn't reset the memory module's max-position
    //    tracker; without a full clear the next decode rejects new
    //    positions ≤ the stored max with `X < Y` errors.
    ctx.clear_kv_cache();

    // 3. Prefill the entire prompt in one batch.
    let mut batch = LlamaBatch::new(config.batch_size as usize, 1);
    for (i, &tok) in prompt_tokens.iter().enumerate() {
        let last = i == n_prompt - 1;
        batch
            .add(tok, i as i32, &[0], last)
            .map_err(|e| Error::Execution(format!("batch add (prefill): {}", e)))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| Error::Execution(format!("decode (prefill): {}", e)))?;

    let mut pos = n_prompt as i32;

    // 4. Build the sampler chain.
    let mut chain_top_k: Vec<LlamaSampler> = Vec::new();
    let mut chain_no_top_k: Vec<LlamaSampler> = Vec::new();
    if config.min_p > 0.0 {
        chain_top_k.push(LlamaSampler::min_p(config.min_p, 1));
        chain_no_top_k.push(LlamaSampler::min_p(config.min_p, 1));
    }
    chain_top_k.extend([
        LlamaSampler::top_k(config.top_k as i32),
        LlamaSampler::top_p(config.top_p, 1),
        LlamaSampler::temp(config.temperature),
        LlamaSampler::dist(config.seed as u32),
    ]);
    chain_no_top_k.extend([
        LlamaSampler::top_p(config.top_p, 1),
        LlamaSampler::temp(config.temperature),
        LlamaSampler::dist(config.seed as u32),
    ]);
    let sampler = if config.top_k > 0 {
        LlamaSampler::chain_simple(chain_top_k)
    } else {
        LlamaSampler::chain_simple(chain_no_top_k)
    };

    // 5. Sample assistant tokens until EOG / max_tokens.
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut stripper = ThinkStripper::new();
    let mut chunks: Vec<String> = Vec::new();
    let mut response = String::new();

    for _ in 0..config.max_tokens {
        let token = sampler.sample(ctx, batch.n_tokens() - 1);
        if model.is_eog_token(token) {
            break;
        }
        let bytes = model
            .token_to_bytes(token, Special::Tokenize)
            .map_err(|e| Error::Execution(format!("token decode: {}", e)))?;
        // `decode_to_string` only writes when `dst` has spare capacity
        // ≥ `max_utf8_buffer_length(src.len())`. With an empty `String`
        // it silently returns `OutputFull`, which is why early runs
        // produced zero visible output for valid bytes.
        let cap = decoder
            .max_utf8_buffer_length(bytes.len())
            .unwrap_or(bytes.len() * 4 + 4);
        let mut piece = String::with_capacity(cap);
        let _ = decoder.decode_to_string(&bytes, &mut piece, false);

        if let Some(out) = stripper.push(&piece) {
            response.push_str(&out);
            chunks.push(out);
        }

        batch.clear();
        batch
            .add(token, pos, &[0], true)
            .map_err(|e| Error::Execution(format!("batch add (gen): {}", e)))?;
        ctx.decode(&mut batch)
            .map_err(|e| Error::Execution(format!("decode (gen): {}", e)))?;
        pos += 1;
    }

    // Flush any UTF-8 bytes still buffered inside the streaming decoder
    // (e.g. an incomplete multi-byte codepoint at the end of generation),
    // then drain whatever's left in the think-stripper buffer.
    let cap = decoder.max_utf8_buffer_length(0).unwrap_or(8);
    let mut tail = String::with_capacity(cap);
    let _ = decoder.decode_to_string(&[], &mut tail, true);
    if let Some(out) = stripper.push(&tail) {
        response.push_str(&out);
        chunks.push(out);
    }
    if let Some(out) = stripper.flush() {
        response.push_str(&out);
        chunks.push(out);
    }

    // 6. Persist the turn into history. Skip empty assistant responses
    //    so the model doesn't see an empty `assistant` block on the
    //    next turn (which can confuse chat templates and tokenization).
    let user_msg = probe_messages
        .pop()
        .expect("probe_messages always has at least the new user msg");
    if !response.is_empty() {
        state.messages.push(user_msg);
        let asst_msg = LlamaChatMessage::new("assistant".to_string(), response)
            .map_err(|e| Error::Execution(format!("invalid asst msg: {}", e)))?;
        state.messages.push(asst_msg);
    }

    Ok((
        chunks,
        TurnStats {
            n_decoded: n_prompt,
            n_reused: 0,
        },
    ))
}

/// Wrapper for StreamingNode trait.
pub struct LlamaCppGenerationNodeWrapper(pub Arc<LlamaCppGenerationNode>);

#[async_trait::async_trait]
impl StreamingNode for LlamaCppGenerationNodeWrapper {
    fn node_type(&self) -> &str {
        self.0.node_type()
    }

    fn node_id(&self) -> &str {
        &self.0.node_id
    }

    async fn initialize(&self, ctx: &InitializeContext) -> Result<(), Error> {
        AsyncStreamingNode::initialize(self.0.as_ref(), ctx).await
    }

    async fn process_async(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        self.0.process(data).await
    }

    async fn process_multi_async(
        &self,
        inputs: std::collections::HashMap<String, RuntimeData>,
    ) -> Result<RuntimeData, Error> {
        if let Some((_, data)) = inputs.into_iter().next() {
            self.process_async(data).await
        } else {
            Err(Error::Execution("No input data provided".into()))
        }
    }

    fn is_multi_input(&self) -> bool {
        false
    }

    fn capability_behavior(&self) -> crate::capabilities::CapabilityBehavior {
        crate::capabilities::CapabilityBehavior::Static
    }
}

/// Factory for LlamaCppGenerationNode.
pub struct LlamaCppGenerationNodeFactory;

impl Default for LlamaCppGenerationNodeFactory {
    fn default() -> Self {
        Self
    }
}

impl StreamingNodeFactory for LlamaCppGenerationNodeFactory {
    fn create(
        &self,
        node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn StreamingNode>, Error> {
        let node = LlamaCppGenerationNode::from_params(node_id, params)?;
        Ok(Box::new(LlamaCppGenerationNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "LlamaCppGenerationNode"
    }

    fn capability_behavior(&self) -> crate::capabilities::CapabilityBehavior {
        crate::capabilities::CapabilityBehavior::Static
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("LlamaCppGenerationNode")
                .description(
                    "Text generation via llama.cpp (GGUF models). \
                     Accepts RuntimeData::Text or RuntimeData::Json prompts \
                     and streams generated tokens downstream. \
                     Supports CUDA/Metal/Vulkan GPU acceleration. \
                     Runs inference on a dedicated worker thread (llama.cpp \
                     types are not Send). Model is loaded eagerly during \
                     initialize() and reused across calls.",
                )
                .category("llm")
                .accepts([RuntimeDataType::Text, RuntimeDataType::Json])
                .produces([RuntimeDataType::Text])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: true,
                    latency_class: LatencyClass::Slow,
                })
                .config_schema_from::<LlamaCppGenerationConfig>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let mut config = LlamaCppGenerationConfig::default();
        config.model_path = "/path/to/model.gguf".to_string();
        let node = LlamaCppGenerationNode::new("test-gen", &config);
        assert!(node.is_ok());
    }

    #[test]
    fn test_factory() {
        let factory = LlamaCppGenerationNodeFactory;
        assert_eq!(factory.node_type(), "LlamaCppGenerationNode");
        assert!(factory.is_multi_output_streaming());
    }

    #[test]
    fn test_from_params() {
        let params = serde_json::json!({
            "model_path": "/path/to/model.gguf",
            "context_size": 2048,
            "temperature": 0.7,
        });
        let node = LlamaCppGenerationNode::from_params("test", &params);
        assert!(node.is_ok());
    }
}
