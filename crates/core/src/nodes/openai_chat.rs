//! OpenAI-compatible Chat Completion Node
//!
//! Makes streaming HTTP calls to an OpenAI-compatible API (OpenAI, Azure,
//! local vLLM/Ollama/LLama.cpp servers) and emits channel-tagged
//! `RuntimeData::Text` tokens downstream.
//!
//! Pipeline flow:
//!   UserText → OpenAIChatNode → [streaming tokens] → TextCollector → TTS
//!
//! The node accepts `RuntimeData::Text` (user message) and
//! `RuntimeData::Json` (structured message with role/content), then
//! streams assistant responses as tagged text on the `"tts"` channel
//! by default. Use `output_channel` to override (e.g., `"ui"` for
//! display-only text).

use crate::data::{tag_text_str, RuntimeData, TEXT_CHANNEL_DEFAULT};
use crate::error::Error;
use crate::nodes::AsyncStreamingNode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_stream::StreamExt;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the OpenAI-compatible chat node.
///
/// All fields support both `snake_case` and `camelCase` keys via
/// `#[serde(alias)]` so manifests can use either convention.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct OpenAIChatConfig {
    /// OpenAI API key. Also read from `OPENAI_API_KEY` env var if omitted.
    #[serde(alias = "apiKey")]
    pub api_key: Option<String>,

    /// Base URL for the API endpoint.
    /// Default: `https://api.openai.com/v1`
    #[serde(alias = "baseUrl")]
    pub base_url: Option<String>,

    /// Model identifier (e.g. `"gpt-4o"`, `"gpt-4o-mini"`, `"qwen2.5-7b"`).
    #[serde(alias = "model")]
    pub model: Option<String>,

    /// System prompt sent as the first message in every request.
    /// Omit to use no system message.
    #[serde(alias = "systemPrompt")]
    pub system_prompt: Option<String>,

    /// Output channel tag for emitted text tokens.
    /// Default: `"tts"` (spoken). Use `"ui"` for display-only text.
    #[serde(alias = "outputChannel")]
    pub output_channel: Option<String>,

    /// Channel tag for reasoning / thinking tokens emitted by reasoning
    /// models (Qwen3, DeepSeek, etc.) on the SSE field
    /// `delta.reasoning_content`. These tokens are forwarded so the
    /// pipeline sees continuous LLM activity (preventing the coordinator's
    /// silence watchdog from firing during long reasoning phases) but
    /// land on a non-`tts` channel so TTS does not speak them.
    ///
    /// Default: `"think"`. Set to `null`/empty to drop reasoning content
    /// entirely (which will re-introduce silence-watchdog timeouts on
    /// reasoning models — usually you don't want that).
    #[serde(alias = "reasoningChannel")]
    pub reasoning_channel: Option<String>,

    /// Maximum tokens to generate. Default: `4096`.
    #[serde(alias = "maxTokens")]
    pub max_tokens: Option<u32>,

    /// Temperature for sampling (0.0–2.0). Default: `1.0`.
    pub temperature: Option<f32>,

    /// Top-p nucleus sampling cutoff (0.0–1.0). Default: `1.0`.
    #[serde(alias = "topP")]
    pub top_p: Option<f32>,

    /// Number of conversation turns to retain as context history.
    /// Default: `10` (i.e. last 10 user+assistant pairs).
    /// Set to `0` for stateless single-turn mode.
    #[serde(alias = "historyTurns")]
    pub history_turns: usize,

    /// Whether to enable streaming responses. Default: `true`.
    /// When `false`, the full response is buffered then emitted as one chunk.
    #[serde(alias = "streaming")]
    pub streaming: bool,

    // ── Tool calling ────────────────────────────────────────────────
    //
    // Mirrors the registry on the Python `QwenTextMlxNode`. When any
    // tools are active, the request body includes a `tools` array and
    // `tool_choice` field; the SSE parser accumulates
    // `delta.tool_calls` and dispatches `side_effect` tools (`say`,
    // `show`, ...) to the appropriate downstream channel without
    // feeding a result back to the model.
    /// Register the built-in `say` tool. Routes its `text` argument
    /// to the `output_channel` (default `tts`) so it's spoken
    /// immediately.
    #[serde(alias = "enableSayTool")]
    pub enable_say_tool: bool,

    /// Register the built-in `show` tool. Routes its `content`
    /// argument to the `ui` channel for display-only rendering.
    #[serde(alias = "enableShowTool")]
    pub enable_show_tool: bool,

    /// Additional user-defined tools registered alongside the
    /// built-ins. Tool names must be unique across the union.
    #[serde(default, alias = "tools")]
    pub tools: Vec<crate::nodes::tool_spec::ToolSpec>,

    /// Subset of registered tool names exposed to the model.
    /// `None` means "expose every registered tool".
    #[serde(default, alias = "activeTools")]
    pub active_tools: Option<Vec<String>>,

    /// Tool-choice hint sent to the model. Common values: `"auto"`,
    /// `"required"`, `"none"`. JSON-shaped values like
    /// `{"type":"function","function":{"name":"say"}}` are also
    /// accepted (the value is forwarded verbatim).
    #[serde(default, alias = "toolChoice")]
    pub tool_choice: Option<Value>,
}

impl Default for OpenAIChatConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: None,
            model: None,
            system_prompt: None,
            output_channel: None,
            reasoning_channel: Some("think".to_string()),
            max_tokens: None,
            temperature: None,
            top_p: None,
            history_turns: 10,
            streaming: true,
            enable_say_tool: false,
            enable_show_tool: false,
            tools: Vec::new(),
            active_tools: None,
            tool_choice: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Message history entry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MessageEntry {
    role: String,
    content: String,
}

// ---------------------------------------------------------------------------
// Streaming tool-call accumulator
// ---------------------------------------------------------------------------

/// Per-index accumulator for `delta.tool_calls` deltas. Each tool call
/// arrives split across many SSE chunks; we hold partial state here
/// until the stream ends and then parse `arguments` as JSON before
/// dispatching.
#[derive(Debug, Default)]
struct ToolCallAccum {
    name: String,
    /// Stringified JSON fragment that, once concatenated, parses to
    /// the tool-call argument object. Kept as `String` (not `Value`)
    /// because mid-stream content is almost always non-parseable.
    arguments: String,
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// OpenAI-compatible streaming chat completion node.
///
/// Cancellation note: this node intentionally has no barge-in
/// awareness. The runtime (see `session_router::spawn_node_pipeline`)
/// intercepts `<node>.in.barge_in` envelopes and cancels the in-flight
/// `process_streaming_async` future by dropping it. Dropping the
/// future cascades into dropping the reqwest stream, which closes
/// the HTTP connection and stops upstream token generation. We rely
/// on that universal mechanism instead of duplicating per-node
/// cancellation plumbing.
pub struct OpenAIChatNode {
    config: OpenAIChatConfig,
    /// Per-session conversation history (role, content).
    history: Arc<parking_lot::Mutex<HashMap<String, Vec<MessageEntry>>>>,
    /// Compiled HTTP client (shared across calls).
    client: Arc<reqwest::Client>,
}

impl OpenAIChatNode {
    /// Create from a config struct.
    pub fn with_config(config: OpenAIChatConfig) -> Self {
        Self {
            config,
            history: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            client: Arc::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(120))
                    // Disable Nagle's algorithm. With it on, small SSE
                    // delta frames from a local LLM (vLLM/Ollama on
                    // 127.0.0.1) get held up to ~40 ms each waiting
                    // for more bytes to coalesce, which shows up as
                    // step-function token latency for short replies.
                    .tcp_nodelay(true)
                    // Force HTTP/1.1. Local LLM servers almost always
                    // speak h1; advertising h2 can trigger an ALPN
                    // round-trip on connection setup. We're already
                    // pooling connections via this shared client so
                    // there's no multiplexing benefit to gain.
                    .http1_only()
                    .build()
                    .expect("build reqwest client"),
            ),
        }
    }

    /// Resolve the effective API key (config → env var).
    ///
    /// Returns `None` when neither source provides a key. Callers decide
    /// whether that's an error (cloud OpenAI) or acceptable (local
    /// vLLM/Ollama/llama.cpp servers without auth).
    fn resolve_api_key(&self) -> Option<String> {
        self.config
            .api_key
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty()))
    }

    /// Resolve the effective base URL.
    fn resolve_base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }

    /// Resolve the effective model name.
    fn resolve_model(&self) -> String {
        self.config
            .model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string())
    }

    /// Build the active tool registry for this node.
    ///
    /// Combines the built-in `say` / `show` tools (gated by config
    /// flags) with any user-defined tools, then filters by
    /// `active_tools` if set. Names are deduplicated; later entries
    /// for the same name override earlier ones, so a user-supplied
    /// `say` overrides the built-in.
    fn build_tool_registry(&self) -> Vec<crate::nodes::tool_spec::ToolSpec> {
        use crate::nodes::tool_spec::{default_say_tool, default_show_tool, ToolSpec};
        let mut out: Vec<ToolSpec> = Vec::new();
        if self.config.enable_say_tool {
            out.push(default_say_tool());
        }
        if self.config.enable_show_tool {
            out.push(default_show_tool());
        }
        for spec in &self.config.tools {
            // Override-by-name semantics — later wins.
            if let Some(existing) = out.iter_mut().find(|t| t.name == spec.name) {
                *existing = spec.clone();
            } else {
                out.push(spec.clone());
            }
        }
        if let Some(ref active) = self.config.active_tools {
            let active: std::collections::HashSet<&str> =
                active.iter().map(String::as_str).collect();
            out.retain(|t| active.contains(t.name.as_str()));
        }
        out
    }

    /// Look up a registered tool spec by name.
    fn lookup_tool<'a>(
        registry: &'a [crate::nodes::tool_spec::ToolSpec],
        name: &str,
    ) -> Option<&'a crate::nodes::tool_spec::ToolSpec> {
        registry.iter().find(|t| t.name == name)
    }

    /// Build the `/chat/completions` request body.
    fn build_request_body(
        &self,
        session_id: &str,
        user_content: &str,
    ) -> Result<Value, Error> {
        let mut messages: Vec<Value> = Vec::new();

        // System prompt
        if let Some(ref sys) = self.config.system_prompt {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys,
            }));
        }

        // Conversation history (bounded by history_turns)
        {
            let hist = self.history.lock();
            if let Some(entries) = hist.get(session_id) {
                let max_turns = self.config.history_turns;
                let start = if entries.len() > max_turns * 2 {
                    entries.len() - max_turns * 2
                } else {
                    0
                };
                for entry in entries.iter().skip(start) {
                    messages.push(serde_json::json!({
                        "role": entry.role,
                        "content": entry.content,
                    }));
                }
            }
        }

        // Current user message
        messages.push(serde_json::json!({
            "role": "user",
            "content": user_content,
        }));

        let mut body = serde_json::json!({
            "model": self.resolve_model(),
            "messages": messages,
            "stream": self.config.streaming,
        });

        if let Some(max_tokens) = self.config.max_tokens {
            body["max_tokens"] = Value::Number(max_tokens.into());
        }
        if let Some(temperature) = self.config.temperature {
            if let Some(num) = serde_json::Number::from_f64(temperature as f64) {
                body["temperature"] = Value::Number(num);
            }
        }
        if let Some(top_p) = self.config.top_p {
            if let Some(num) = serde_json::Number::from_f64(top_p as f64) {
                body["top_p"] = Value::Number(num);
            }
        }

        // Tool registry: only emit `tools` / `tool_choice` when at
        // least one tool is active. Sending an empty `tools: []` to
        // some servers makes them refuse to generate.
        let tools = self.build_tool_registry();
        if !tools.is_empty() {
            body["tools"] = crate::nodes::tool_spec::to_openai_tools_array(&tools);
            if let Some(ref tc) = self.config.tool_choice {
                body["tool_choice"] = tc.clone();
            }
        }

        Ok(body)
    }

    /// Append a message to per-session history.
    fn append_history(&self, session_id: &str, role: &str, content: &str) {
        let mut hist = self.history.lock();
        hist.entry(session_id.to_string())
            .or_insert_with(Vec::new)
            .push(MessageEntry {
                role: role.to_string(),
                content: content.to_string(),
            });
    }
}

#[async_trait::async_trait]
impl AsyncStreamingNode for OpenAIChatNode {
    fn node_type(&self) -> &str {
        "OpenAIChatNode"
    }

    async fn initialize(&self, ctx: &crate::nodes::InitializeContext) -> Result<(), Error> {
        let api_key = self.resolve_api_key();
        let base_url = self.resolve_base_url();
        let model = self.resolve_model();

        let masked_key = match api_key.as_deref() {
            Some(k) if k.len() > 8 => format!("{}****{}", &k[..4], &k[k.len() - 4..]),
            Some(_) => "****".to_string(),
            None => "(none)".to_string(),
        };

        ctx.emit_progress(
            "loading_node",
            &format!(
                "OpenAIChatNode: model={}, endpoint={}",
                model, base_url
            ),
        );

        tracing::info!(
            node = "OpenAIChatNode",
            model = %model,
            base_url = %base_url,
            api_key = %masked_key,
            streaming = self.config.streaming,
            history_turns = self.config.history_turns,
            "Initializing OpenAIChatNode"
        );

        ctx.emit_progress("ready", &format!("OpenAIChatNode ready (model={})", model));

        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // For single-shot processing we buffer the full response.
        // The streaming path (process_streaming) is preferred.
        // Aux-port envelopes (barge_in, etc.) are filtered by the
        // runtime in `session_router::spawn_node_pipeline` and never
        // reach this method.
        let mut outputs: Vec<RuntimeData> = Vec::new();
        let session_id = "default".to_string();

        let user_text = match &data {
            RuntimeData::Text(t) => t.clone(),
            RuntimeData::Json(j) => {
                match j
                    .get("content")
                    .or(j.get("text"))
                    .and_then(|v| v.as_str())
                {
                    Some(s) => s.to_string(),
                    None => {
                        tracing::debug!(
                            node = "OpenAIChatNode",
                            "Dropping JSON input with no `content`/`text` field"
                        );
                        return Ok(RuntimeData::Text(tag_text_str("", TEXT_CHANNEL_DEFAULT)));
                    }
                }
            }
            _ => {
                return Err(Error::Execution(format!(
                    "OpenAIChatNode expects Text or Json input, got: {}",
                    data.data_type()
                )))
            }
        };

        if user_text.trim().is_empty() {
            return Ok(RuntimeData::Text(tag_text_str("", TEXT_CHANNEL_DEFAULT)));
        }

        self.process_streaming_internal(&session_id, &user_text, &mut |out| {
            outputs.push(out);
            Ok(())
        })
        .await?;

        // Return the last output (full accumulated text) for single-shot mode.
        outputs
            .into_iter()
            .last()
            .ok_or_else(|| Error::Execution("OpenAIChatNode: no output generated".into()))
    }

    async fn process_streaming<F>(
        &self,
        data: RuntimeData,
        session_id: Option<String>,
        mut callback: F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error> + Send,
    {
        // Aux-port envelopes (barge_in, etc.) are filtered by the
        // runtime in `session_router::spawn_node_pipeline` and never
        // reach this method.
        let sid = session_id.unwrap_or_else(|| "default".to_string());

        let user_text = match &data {
            RuntimeData::Text(t) => t.clone(),
            RuntimeData::Json(j) => {
                match j
                    .get("content")
                    .or(j.get("text"))
                    .and_then(|v| v.as_str())
                {
                    Some(s) => s.to_string(),
                    None => {
                        // Don't stringify arbitrary JSON into the prompt —
                        // that's how control/tap frames leak into the LLM.
                        // Drop and move on.
                        tracing::debug!(
                            node = "OpenAIChatNode",
                            session_id = %sid,
                            "Dropping JSON input with no `content`/`text` field"
                        );
                        return Ok(0);
                    }
                }
            }
            _ => {
                return Err(Error::Execution(format!(
                    "OpenAIChatNode expects Text or Json input, got: {}",
                    data.data_type()
                )))
            }
        };

        if user_text.trim().is_empty() {
            return Ok(0);
        }

        self.process_streaming_internal(&sid, &user_text, &mut callback).await
    }
}

impl OpenAIChatNode {
    /// Dispatch one accumulated tool call.
    ///
    /// Routing matches the Python `_handle_tool_call` in
    /// `qwen_text_mlx.py:1663`:
    ///
    /// - `say` → emit `text` argument on the LLM's `output_channel`
    ///   (default `tts`) with a forced trailing `\n` so the
    ///   coordinator's sentencer flushes it as a complete utterance
    ///   immediately. Falls through several alias keys
    ///   (`text`/`content`/`message`/`body`/`spoken`) to tolerate
    ///   models that mis-shape the arg.
    /// - `show` → emit `content` argument on the `ui` channel.
    /// - any other registered `side_effect` tool → log + drop. We
    ///   don't yet have a generic dispatch surface for user-provided
    ///   handlers (Python's `handler` callable doesn't translate
    ///   directly to a JSON-manifest config). Future: emit a
    ///   `RuntimeData::Json` envelope on a `tools` channel so a
    ///   downstream `ToolDispatcherNode` can act on it.
    /// - `return_value` tools → log + drop (multi-pass not
    ///   implemented; matches Python).
    fn dispatch_tool_call<F>(
        registry: &[crate::nodes::tool_spec::ToolSpec],
        call: &ToolCallAccum,
        output_channel: &str,
        callback: &mut F,
    ) -> Result<(), Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error>,
    {
        use crate::nodes::tool_spec::ToolKind;

        if call.name.is_empty() {
            tracing::warn!(
                node = "OpenAIChatNode",
                "tool call with no name received; dropping"
            );
            return Ok(());
        }

        let spec = match Self::lookup_tool(registry, &call.name) {
            Some(s) => s,
            None => {
                tracing::warn!(
                    node = "OpenAIChatNode",
                    tool = %call.name,
                    "model called unregistered tool; dropping"
                );
                return Ok(());
            }
        };

        if spec.kind == ToolKind::ReturnValue {
            tracing::warn!(
                node = "OpenAIChatNode",
                tool = %call.name,
                "return_value tools require a second generation pass \
                 (not yet implemented in the OpenAI streaming path); skipping"
            );
            return Ok(());
        }

        // Parse arguments JSON. On failure, fall back to empty args
        // so alias-key lookup at least tries the raw string path.
        let args: Value = serde_json::from_str(&call.arguments).unwrap_or_else(|e| {
            tracing::warn!(
                node = "OpenAIChatNode",
                tool = %call.name,
                error = %e,
                raw = %call.arguments,
                "tool call arguments did not parse as JSON; treating as empty"
            );
            Value::Object(serde_json::Map::new())
        });

        let extract_string =
            |keys: &[&str]| -> Option<String> {
                for k in keys {
                    if let Some(s) = args.get(*k).and_then(Value::as_str) {
                        if !s.is_empty() {
                            return Some(s.to_string());
                        }
                    }
                }
                // Tolerate models that hand back raw text instead of
                // a JSON object: if `arguments` is itself a quoted
                // string, use it.
                if let Value::String(s) = &args {
                    if !s.is_empty() {
                        return Some(s.clone());
                    }
                }
                None
            };

        match call.name.as_str() {
            "say" => {
                let spoken = extract_string(&["text", "content", "message", "body", "spoken"]);
                if let Some(text) = spoken {
                    // Force a trailing newline. The coordinator's
                    // sentencer (`split_pattern: "[.!?,;:\n]+"`)
                    // flushes on `\n`, so this guarantees the say()
                    // body becomes a complete utterance the moment
                    // dispatch fires — no holding it inside the
                    // sentence buffer until end-of-turn.
                    let flushable = if text.ends_with('\n') {
                        text
                    } else {
                        format!("{}\n", text)
                    };
                    callback(RuntimeData::Text(tag_text_str(&flushable, output_channel)))?;
                    // Mirror to the default text channel so the
                    // frontend transcript displays the spoken reply.
                    // When the model uses tools-only (no plain
                    // `delta.content`), this is the ONLY path that
                    // populates the assistant transcript. Skip the
                    // mirror if `output_channel` is already the
                    // default to avoid duplicate emission.
                    if output_channel != TEXT_CHANNEL_DEFAULT {
                        callback(RuntimeData::Text(tag_text_str(
                            &flushable,
                            TEXT_CHANNEL_DEFAULT,
                        )))?;
                    }
                } else {
                    tracing::warn!(
                        node = "OpenAIChatNode",
                        args = %call.arguments,
                        "`say` tool call had no recognisable text arg; nothing to synthesise"
                    );
                }
            }
            "show" => {
                let written = extract_string(&["content", "markdown", "text", "body"]);
                if let Some(text) = written {
                    callback(RuntimeData::Text(tag_text_str(&text, "ui")))?;
                    // Mirror to the default channel so the assistant
                    // transcript shows the displayed content too.
                    callback(RuntimeData::Text(tag_text_str(
                        &text,
                        TEXT_CHANNEL_DEFAULT,
                    )))?;
                } else {
                    tracing::warn!(
                        node = "OpenAIChatNode",
                        args = %call.arguments,
                        "`show` tool call had no recognisable content arg"
                    );
                }
            }
            other => {
                // Generic side_effect tool we don't have a built-in
                // route for. Drop with a debug log.
                tracing::debug!(
                    node = "OpenAIChatNode",
                    tool = %other,
                    args = %call.arguments,
                    "side_effect tool dispatched; no built-in handler — dropping"
                );
            }
        }
        Ok(())
    }

    /// Core streaming logic shared by `process` and `process_streaming`.
    async fn process_streaming_internal<F>(
        &self,
        session_id: &str,
        user_content: &str,
        callback: &mut F,
    ) -> Result<usize, Error>
    where
        F: FnMut(RuntimeData) -> Result<(), Error>,
    {
        let api_key = self.resolve_api_key();
        let base_url = self.resolve_base_url();
        let body = self.build_request_body(session_id, user_content)?;

        // Record user message in history.
        self.append_history(session_id, "user", user_content);

        tracing::info!(
            node = "OpenAIChatNode",
            model = self.resolve_model(),
            base_url,
            streaming = self.config.streaming,
            "Sending chat completion request"
        );

        let output_channel = self
            .config
            .output_channel
            .clone()
            .unwrap_or_else(|| TEXT_CHANNEL_DEFAULT.to_string());

        // Reasoning channel: tokens emitted on `delta.reasoning_content`
        // by reasoning models go here. Non-empty channel name → forward
        // as a heartbeat so the coordinator sees activity. Empty/None →
        // drop entirely (legacy behavior; will silently re-enable
        // silence-watchdog timeouts on reasoning models).
        let reasoning_channel: Option<String> = self
            .config
            .reasoning_channel
            .clone()
            .filter(|s| !s.is_empty());

        if self.config.streaming {
            // ---- Streaming path (SSE) ----
            //
            // No per-call cancellation logic here: the runtime drops
            // this future when a `barge_in` arrives, which closes the
            // reqwest stream and the upstream HTTP connection. Any
            // partial state (full_text, history append) below is only
            // reached when the call completes normally.
            let mut req = self
                .client
                .post(format!("{}/chat/completions", base_url))
                .json(&body);
            if let Some(ref key) = api_key {
                req = req.bearer_auth(key);
            }
            let response = req
                .send()
                .await
                .map_err(|e| Error::Execution(format!("OpenAI HTTP request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let err_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<no body>".into());
                return Err(Error::Execution(format!(
                    "OpenAI API error {}: {}",
                    status, err_body
                )));
            }

            let mut full_text = String::new();
            let mut token_count = 0usize;

            // Tool-call accumulator. The OpenAI streaming protocol
            // splits a tool call across many SSE chunks: the first
            // delta carries `function.name`, subsequent deltas append
            // characters to `function.arguments` (a stringified JSON
            // fragment), and a final chunk has `finish_reason:"tool_calls"`.
            // We accumulate per-index, then dispatch on stream end.
            let tool_registry = self.build_tool_registry();
            let mut tool_calls: HashMap<u64, ToolCallAccum> = HashMap::new();

            // Parse SSE stream line by line.
            let mut stream = response.bytes_stream();
            let mut buf = Vec::<u8>::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = chunk_result.map_err(|e| {
                    Error::Execution(format!("OpenAI stream read error: {}", e))
                })?;

                buf.extend_from_slice(&chunk);

                // Process complete lines from the buffer.
                while let Some(newline_pos) = buf.iter().position(|&b| b == b'\n') {
                    let line_bytes: Vec<u8> = buf.drain(..=newline_pos).collect();
                    let line = match std::str::from_utf8(&line_bytes) {
                        Ok(s) => s.trim_end_matches(|c| c == '\n' || c == '\r'),
                        Err(_) => continue,
                    };

                    if line.starts_with("data: ") {
                        let payload = &line[6..];

                        // End of stream marker.
                        if payload.trim() == "[DONE]" {
                            break;
                        }

                        // Parse JSON chunk.
                        if let Ok(json) = serde_json::from_str::<Value>(payload) {
                            let delta = &json["choices"][0]["delta"];

                            // Reasoning tokens (Qwen3, DeepSeek-style):
                            // `delta.reasoning_content` is a separate
                            // SSE field for chain-of-thought. Forward
                            // it on the reasoning channel as a
                            // heartbeat so downstream nodes know the
                            // LLM is alive — but TTS gates by channel
                            // and won't speak it. Reasoning is NOT
                            // appended to `full_text` (which becomes
                            // assistant history) — only the visible
                            // reply lives in history.
                            if let Some(rc) = reasoning_channel.as_deref() {
                                if let Some(reasoning) =
                                    delta.get("reasoning_content").and_then(Value::as_str)
                                {
                                    if !reasoning.is_empty() {
                                        callback(RuntimeData::Text(tag_text_str(
                                            reasoning, rc,
                                        )))?;
                                    }
                                }
                            }

                            // Visible reply tokens.
                            if let Some(content) = delta.get("content").and_then(Value::as_str)
                            {
                                if !content.is_empty() {
                                    full_text.push_str(content);
                                    token_count += 1;
                                    callback(RuntimeData::Text(tag_text_str(
                                        content,
                                        &output_channel,
                                    )))?;
                                }
                            }

                            // Tool-call deltas. Accumulate per index;
                            // dispatch happens after the stream ends.
                            if let Some(tcs) =
                                delta.get("tool_calls").and_then(Value::as_array)
                            {
                                for tc in tcs {
                                    let idx = tc
                                        .get("index")
                                        .and_then(Value::as_u64)
                                        .unwrap_or(0);
                                    let entry = tool_calls.entry(idx).or_default();
                                    if let Some(n) = tc
                                        .pointer("/function/name")
                                        .and_then(Value::as_str)
                                    {
                                        if !n.is_empty() {
                                            entry.name = n.to_string();
                                        }
                                    }
                                    if let Some(a) = tc
                                        .pointer("/function/arguments")
                                        .and_then(Value::as_str)
                                    {
                                        entry.arguments.push_str(a);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Dispatch any tool calls collected during the stream.
            // Sort by index so multiple calls fire in protocol order.
            let mut indices: Vec<u64> = tool_calls.keys().copied().collect();
            indices.sort_unstable();
            for idx in indices {
                let entry = match tool_calls.remove(&idx) {
                    Some(e) => e,
                    None => continue,
                };
                Self::dispatch_tool_call(
                    &tool_registry,
                    &entry,
                    &output_channel,
                    callback,
                )?;
            }

            // Record assistant response in history. (If the runtime
            // cancelled us mid-stream this line is never reached
            // because the future was dropped.)
            if !full_text.is_empty() {
                self.append_history(session_id, "assistant", &full_text);
            }

            tracing::info!(
                node = "OpenAIChatNode",
                tokens = token_count,
                chars = full_text.len(),
                "Streaming complete"
            );

            Ok(token_count)
        } else {
            // ---- Non-streaming path ----
            let mut req = self
                .client
                .post(format!("{}/chat/completions", base_url))
                .json(&body);
            if let Some(ref key) = api_key {
                req = req.bearer_auth(key);
            }
            let response = req
                .send()
                .await
                .map_err(|e| Error::Execution(format!("OpenAI HTTP request failed: {}", e)))?;

            if !response.status().is_success() {
                let status = response.status();
                let err_body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<no body>".into());
                return Err(Error::Execution(format!(
                    "OpenAI API error {}: {}",
                    status, err_body
                )));
            }

            let json: Value = response
                .json()
                .await
                .map_err(|e| Error::Execution(format!("OpenAI JSON parse error: {}", e)))?;

            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();

            if content.is_empty() {
                return Ok(0);
            }

            // Record in history.
            self.append_history(session_id, "assistant", &content);

            callback(RuntimeData::Text(tag_text_str(&content, &output_channel)))?;

            tracing::info!(
                node = "OpenAIChatNode",
                chars = content.len(),
                "Non-streaming response complete"
            );

            Ok(1)
        }
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct OpenAIChatNodeFactory;

impl crate::nodes::StreamingNodeFactory for OpenAIChatNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn crate::nodes::StreamingNode>, Error> {
        let config: OpenAIChatConfig =
            serde_json::from_value(params.clone()).unwrap_or_default();
        let node = OpenAIChatNode::with_config(config);
        Ok(Box::new(crate::nodes::AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "OpenAIChatNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true // Emits one RuntimeData::Text per streaming token.
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("OpenAIChatNode")
                .description("Streaming chat completion node for OpenAI-compatible APIs (OpenAI, Azure, vLLM, Ollama, etc.)")
                .category("llm")
                .accepts([RuntimeDataType::Text, RuntimeDataType::Json])
                .produces([RuntimeDataType::Text])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: false,
                    latency_class: LatencyClass::Slow,
                })
                .config_schema_from::<OpenAIChatConfig>(),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::{StreamingNodeFactory, schema::RuntimeDataType};

    #[test]
    fn test_config_defaults() {
        let config = OpenAIChatConfig::default();
        assert_eq!(config.history_turns, 10);
        assert!(config.streaming);
        assert_eq!(config.output_channel, None);
    }

    #[test]
    fn test_config_from_camel_case() {
        let params = serde_json::json!({
            "apiKey": "sk-test",
            "baseUrl": "http://localhost:8080/v1",
            "model": "qwen2.5-7b",
            "systemPrompt": "You are helpful.",
            "outputChannel": "ui",
            "maxTokens": 2048,
            "temperature": 0.7,
            "topP": 0.9,
            "historyTurns": 5,
            "streaming": false,
        });
        let config: OpenAIChatConfig = serde_json::from_value(params).unwrap();
        assert_eq!(config.api_key, Some("sk-test".into()));
        assert_eq!(config.base_url, Some("http://localhost:8080/v1".into()));
        assert_eq!(config.model, Some("qwen2.5-7b".into()));
        assert_eq!(config.system_prompt, Some("You are helpful.".into()));
        assert_eq!(config.output_channel, Some("ui".into()));
        assert_eq!(config.max_tokens, Some(2048));
        assert!((config.temperature.unwrap() - 0.7).abs() < f32::EPSILON);
        assert!((config.top_p.unwrap() - 0.9).abs() < f32::EPSILON);
        assert_eq!(config.history_turns, 5);
        assert!(!config.streaming);
    }

    #[test]
    fn test_config_from_snake_case() {
        let params = serde_json::json!({
            "api_key": "sk-test",
            "base_url": "http://localhost:8080/v1",
            "model": "gpt-4o",
            "system_prompt": "Be concise.",
            "output_channel": "tts",
            "max_tokens": 1024,
            "temperature": 0.5,
            "top_p": 0.95,
            "history_turns": 20,
            "streaming": true,
        });
        let config: OpenAIChatConfig = serde_json::from_value(params).unwrap();
        assert_eq!(config.api_key, Some("sk-test".into()));
        assert_eq!(config.base_url, Some("http://localhost:8080/v1".into()));
        assert_eq!(config.model, Some("gpt-4o".into()));
        assert_eq!(config.system_prompt, Some("Be concise.".into()));
        assert_eq!(config.output_channel, Some("tts".into()));
        assert_eq!(config.max_tokens, Some(1024));
        assert_eq!(config.history_turns, 20);
        assert!(config.streaming);
    }

    #[test]
    fn test_node_type() {
        let config = OpenAIChatConfig::default();
        let node = OpenAIChatNode::with_config(config);
        assert_eq!(node.node_type(), "OpenAIChatNode");
    }

    #[test]
    fn test_resolve_base_url_default() {
        let config = OpenAIChatConfig::default();
        let node = OpenAIChatNode::with_config(config);
        assert_eq!(
            node.resolve_base_url(),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn test_resolve_model_default() {
        let config = OpenAIChatConfig::default();
        let node = OpenAIChatNode::with_config(config);
        assert_eq!(node.resolve_model(), "gpt-4o-mini");
    }

    #[test]
    fn test_missing_api_key_returns_none() {
        let config = OpenAIChatConfig::default();
        let node = OpenAIChatNode::with_config(config);
        // Ensure env var is not set.
        std::env::remove_var("OPENAI_API_KEY");
        assert!(node.resolve_api_key().is_none());
    }

    #[test]
    fn test_resolve_api_key_from_config() {
        std::env::remove_var("OPENAI_API_KEY");
        let mut config = OpenAIChatConfig::default();
        config.api_key = Some("sk-test".into());
        let node = OpenAIChatNode::with_config(config);
        assert_eq!(node.resolve_api_key().as_deref(), Some("sk-test"));
    }

    #[test]
    fn test_resolve_api_key_empty_string_treated_as_none() {
        std::env::remove_var("OPENAI_API_KEY");
        let mut config = OpenAIChatConfig::default();
        config.api_key = Some(String::new());
        let node = OpenAIChatNode::with_config(config);
        assert!(node.resolve_api_key().is_none());
    }

    #[test]
    fn test_build_request_body_includes_system_prompt() {
        let mut config = OpenAIChatConfig::default();
        config.api_key = Some("sk-test".into());
        config.system_prompt = Some("You are a translator.".into());
        config.model = Some("gpt-4o".into());
        let node = OpenAIChatNode::with_config(config);
        let body = node.build_request_body("sess1", "Hello").unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a translator.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello");
    }

    #[test]
    fn test_build_request_body_without_system_prompt() {
        let mut config = OpenAIChatConfig::default();
        config.api_key = Some("sk-test".into());
        let node = OpenAIChatNode::with_config(config);
        let body = node.build_request_body("sess1", "Hello").unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn test_history_append_and_retrieve() {
        let mut config = OpenAIChatConfig::default();
        config.api_key = Some("sk-test".into());
        let node = OpenAIChatNode::with_config(config);
        node.append_history("s1", "user", "Hi");
        node.append_history("s1", "assistant", "Hello!");

        let hist = node.history.lock();
        let entries = hist.get("s1").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].role, "user");
        assert_eq!(entries[0].content, "Hi");
        assert_eq!(entries[1].role, "assistant");
        assert_eq!(entries[1].content, "Hello!");
    }

    #[test]
    fn test_factory_node_type() {
        let factory = OpenAIChatNodeFactory;
        assert_eq!(factory.node_type(), "OpenAIChatNode");
        assert!(factory.is_multi_output_streaming());
    }

    #[test]
    fn test_factory_creates_node() {
        let factory = OpenAIChatNodeFactory;
        let params = serde_json::json!({
            "api_key": "sk-test",
            "model": "gpt-4o-mini",
        });
        let result = factory.create("node1".into(), &params, None);
        assert!(result.is_ok());
        let node = result.unwrap();
        assert_eq!(node.node_type(), "OpenAIChatNode");
    }

    #[test]
    fn test_factory_schema() {
        let factory = OpenAIChatNodeFactory;
        let schema = factory.schema().unwrap();
        assert_eq!(schema.node_type, "OpenAIChatNode");
        assert!(!schema.description.as_ref().unwrap().is_empty());
        assert_eq!(schema.category, Some("llm".into()));
        assert!(schema.accepts.contains(&RuntimeDataType::Text));
        assert!(schema.accepts.contains(&RuntimeDataType::Json));
        assert!(schema.produces.contains(&RuntimeDataType::Text));
        assert!(schema.config_schema.is_some());
    }

    #[test]
    fn test_sse_parse_basic() {
        // Simulate parsing an SSE data line.
        let line = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}";
        let payload = &line[6..];
        let json: Value = serde_json::from_str(payload).unwrap();
        let content = json["choices"][0]["delta"]["content"].as_str();
        assert_eq!(content, Some("Hello"));
    }

    #[test]
    fn test_sse_done_marker() {
        assert_eq!("data: [DONE]".len() > 6, true);
        let payload = "data: [DONE]";
        assert_eq!(payload[6..].trim(), "[DONE]");
    }
}
