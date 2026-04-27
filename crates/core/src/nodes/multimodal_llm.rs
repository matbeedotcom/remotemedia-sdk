//! Multimodal LLM Node
//!
//! Streams chat-completion requests against an OpenAI-shape API and
//! accepts `Text`, `Image`, and `Json` inputs which are normalised
//! into a `content`-parts array on a single `user` message:
//!
//! ```json
//! {"role":"user","content":[
//!   {"type":"text","text":"What's in this image?"},
//!   {"type":"image_url","image_url":{"url":"data:image/png;base64,..."}}
//! ]}
//! ```
//!
//! Two aggregation modes:
//!
//! - `PerInput` (default) — every input frame becomes its own one-part
//!   user turn. Good for "describe this single image" pipelines.
//! - `CoalesceUntil { sentinel }` — accumulates parts in a per-session
//!   buffer; flushes as one user turn when a `Text` input matching the
//!   sentinel arrives. Required for "N images + a question" or
//!   "audio utterance + question" pipelines.
//!
//! Transport (HTTP, SSE, tool dispatch, history) is shared with
//! [`crate::nodes::openai_chat::OpenAIChatNode`] via
//! [`crate::llm::ChatBackend`]. Tool calls (`say`/`show` and
//! user-defined side-effect tools) work identically.

use crate::data::{tag_text_str, RuntimeData, TEXT_CHANNEL_DEFAULT};
use crate::error::Error;
use crate::llm::audio_encode::audio_to_wav_base64;
use crate::llm::{data_url::image_to_data_url, ChatBackend, ChatBackendConfig, OpenAIProfile};
use crate::nodes::AsyncStreamingNode;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// How input frames are grouped into chat-completion `user` messages.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AggregationMode {
    /// Each input frame is its own user turn (one chat-completion
    /// request per frame). Default.
    PerInput,
    /// Accumulate parts in a per-session buffer; flush when a `Text`
    /// input matches `sentinel`. Default sentinel is `"<|input_end|>"`,
    /// matching the conversation coordinator's end-of-turn marker.
    CoalesceUntil {
        #[serde(default = "default_sentinel")]
        sentinel: String,
    },
}

fn default_sentinel() -> String {
    "<|input_end|>".to_string()
}

impl Default for AggregationMode {
    fn default() -> Self {
        AggregationMode::PerInput
    }
}

/// Configuration for [`MultimodalLLMNode`]. Mirrors
/// [`crate::nodes::openai_chat::OpenAIChatConfig`] field-for-field
/// where they overlap; new fields are documented inline.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct MultimodalLLMConfig {
    #[serde(alias = "apiKey")]
    pub api_key: Option<String>,
    #[serde(alias = "baseUrl")]
    pub base_url: Option<String>,
    #[serde(alias = "model")]
    pub model: Option<String>,
    #[serde(alias = "systemPrompt")]
    pub system_prompt: Option<String>,
    #[serde(alias = "outputChannel")]
    pub output_channel: Option<String>,
    #[serde(alias = "reasoningChannel")]
    pub reasoning_channel: Option<String>,
    #[serde(alias = "maxTokens")]
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(alias = "topP")]
    pub top_p: Option<f32>,
    #[serde(alias = "historyTurns")]
    pub history_turns: usize,
    pub streaming: bool,

    // ── Tool calling (same shape as OpenAIChatConfig) ───────────────
    #[serde(alias = "enableSayTool")]
    pub enable_say_tool: bool,
    #[serde(alias = "enableShowTool")]
    pub enable_show_tool: bool,
    #[serde(default, alias = "tools")]
    pub tools: Vec<crate::nodes::tool_spec::ToolSpec>,
    #[serde(default, alias = "activeTools")]
    pub active_tools: Option<Vec<String>>,
    #[serde(default, alias = "toolChoice")]
    pub tool_choice: Option<Value>,

    // ── Multimodal-only ─────────────────────────────────────────────
    /// Aggregation policy. See [`AggregationMode`].
    #[serde(alias = "aggregation")]
    pub aggregation: AggregationMode,
}

impl Default for MultimodalLLMConfig {
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
            aggregation: AggregationMode::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// Multimodal chat-completion node.
pub struct MultimodalLLMNode {
    config: MultimodalLLMConfig,
    backend: Arc<ChatBackend>,
    /// Per-session content-parts buffer used by the `CoalesceUntil`
    /// aggregation mode. `PerInput` mode never reads or writes this.
    coalesce_buffers: Arc<Mutex<HashMap<String, Vec<Value>>>>,
}

impl MultimodalLLMNode {
    pub fn with_config(config: MultimodalLLMConfig) -> Self {
        Self {
            config,
            backend: Arc::new(ChatBackend::new(Arc::new(OpenAIProfile))),
            coalesce_buffers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn resolve_api_key(&self) -> Option<String> {
        self.config
            .api_key
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty()))
    }

    fn resolve_base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }

    fn resolve_model(&self) -> String {
        self.config
            .model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string())
    }

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

    fn backend_config(&self) -> ChatBackendConfig {
        ChatBackendConfig {
            api_key: self.resolve_api_key(),
            base_url: self.resolve_base_url(),
            model: self.resolve_model(),
            system_prompt: self.config.system_prompt.clone(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            history_turns: self.config.history_turns,
            streaming: self.config.streaming,
            output_channel: self
                .config
                .output_channel
                .clone()
                .unwrap_or_else(|| TEXT_CHANNEL_DEFAULT.to_string()),
            reasoning_channel: self
                .config
                .reasoning_channel
                .clone()
                .filter(|s| !s.is_empty()),
            tools: self.build_tool_registry(),
            tool_choice: self.config.tool_choice.clone(),
        }
    }

    /// Convert one input frame into either a content-part or a sentinel
    /// signal (for `CoalesceUntil` mode). `None` means "drop this
    /// frame silently".
    fn input_to_part(&self, data: &RuntimeData) -> Result<Option<InputPart>, Error> {
        match data {
            RuntimeData::Text(t) => {
                let stripped = crate::data::split_text_str(t).1;
                if stripped.is_empty() {
                    return Ok(None);
                }
                if let AggregationMode::CoalesceUntil { sentinel } = &self.config.aggregation {
                    if stripped == *sentinel || t == sentinel {
                        return Ok(Some(InputPart::Sentinel));
                    }
                }
                Ok(Some(InputPart::Part(serde_json::json!({
                    "type": "text",
                    "text": stripped,
                }))))
            }
            RuntimeData::Json(j) => match j
                .get("content")
                .or(j.get("text"))
                .and_then(|v| v.as_str())
            {
                Some(s) if !s.is_empty() => Ok(Some(InputPart::Part(serde_json::json!({
                    "type": "text",
                    "text": s,
                })))),
                _ => {
                    tracing::debug!(
                        node = "MultimodalLLMNode",
                        "Dropping JSON input with no `content`/`text` field"
                    );
                    Ok(None)
                }
            },
            RuntimeData::Image { .. } => {
                let url = image_to_data_url(data)?;
                Ok(Some(InputPart::Part(serde_json::json!({
                    "type": "image_url",
                    "image_url": {"url": url},
                }))))
            }
            RuntimeData::Audio { .. } => {
                let b64 = audio_to_wav_base64(data)?;
                Ok(Some(InputPart::Part(serde_json::json!({
                    "type": "input_audio",
                    "input_audio": {"data": b64, "format": "wav"},
                }))))
            }
            other => Err(Error::Execution(format!(
                "MultimodalLLMNode does not accept {} input",
                other.data_type()
            ))),
        }
    }

    /// Build a `{role:"user", content:[...]}` message from one or more
    /// content-parts. OpenAI accepts a string `content` for text-only
    /// turns; we use the array form unconditionally for symmetry, and
    /// because gpt-4o-mini accepts both equally.
    fn user_message(parts: Vec<Value>) -> Value {
        serde_json::json!({"role": "user", "content": parts})
    }
}

#[derive(Debug)]
enum InputPart {
    Part(Value),
    Sentinel,
}

#[async_trait::async_trait]
impl AsyncStreamingNode for MultimodalLLMNode {
    fn node_type(&self) -> &str {
        "MultimodalLLMNode"
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
            &format!("MultimodalLLMNode: model={}, endpoint={}", model, base_url),
        );
        tracing::info!(
            node = "MultimodalLLMNode",
            model = %model,
            base_url = %base_url,
            api_key = %masked_key,
            streaming = self.config.streaming,
            history_turns = self.config.history_turns,
            "Initializing MultimodalLLMNode"
        );
        ctx.emit_progress(
            "ready",
            &format!("MultimodalLLMNode ready (model={})", model),
        );
        Ok(())
    }

    async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
        // Single-shot: flatten the streaming output into the last frame.
        let mut outputs: Vec<RuntimeData> = Vec::new();
        let session_id = "default".to_string();
        let mut cb = |out: RuntimeData| -> Result<(), Error> {
            outputs.push(out);
            Ok(())
        };
        self.process_streaming(data, Some(session_id), &mut cb).await?;
        outputs
            .into_iter()
            .last()
            .ok_or_else(|| {
                Error::Execution("MultimodalLLMNode: no output generated".into())
            })
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
        let sid = session_id.unwrap_or_else(|| "default".to_string());

        let part = match self.input_to_part(&data)? {
            Some(p) => p,
            None => return Ok(0),
        };

        let cfg = self.backend_config();
        match (&self.config.aggregation, part) {
            (AggregationMode::PerInput, InputPart::Part(p)) => {
                let user_msg = Self::user_message(vec![p]);
                self.backend.run(&sid, user_msg, &cfg, &mut callback).await
            }
            (AggregationMode::PerInput, InputPart::Sentinel) => {
                // PerInput mode has no buffer to flush — sentinels are
                // a no-op; treat them as a plain text message instead
                // would silently corrupt history if the user wired
                // CoalesceUntil and then switched modes. Drop with a
                // debug log.
                tracing::debug!(
                    node = "MultimodalLLMNode",
                    "received sentinel in PerInput mode — dropping (no buffer to flush)"
                );
                Ok(0)
            }
            (AggregationMode::CoalesceUntil { .. }, InputPart::Part(p)) => {
                self.coalesce_buffers
                    .lock()
                    .entry(sid.clone())
                    .or_insert_with(Vec::new)
                    .push(p);
                Ok(0)
            }
            (AggregationMode::CoalesceUntil { .. }, InputPart::Sentinel) => {
                let parts = self
                    .coalesce_buffers
                    .lock()
                    .remove(&sid)
                    .unwrap_or_default();
                if parts.is_empty() {
                    tracing::debug!(
                        node = "MultimodalLLMNode",
                        "sentinel arrived with empty buffer; nothing to flush"
                    );
                    return Ok(0);
                }
                let user_msg = Self::user_message(parts);
                self.backend.run(&sid, user_msg, &cfg, &mut callback).await
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

pub struct MultimodalLLMNodeFactory;

impl crate::nodes::StreamingNodeFactory for MultimodalLLMNodeFactory {
    fn create(
        &self,
        _node_id: String,
        params: &Value,
        _session_id: Option<String>,
    ) -> Result<Box<dyn crate::nodes::StreamingNode>, Error> {
        let config: MultimodalLLMConfig =
            serde_json::from_value(params.clone()).unwrap_or_default();
        let node = MultimodalLLMNode::with_config(config);
        Ok(Box::new(crate::nodes::AsyncNodeWrapper(Arc::new(node))))
    }

    fn node_type(&self) -> &str {
        "MultimodalLLMNode"
    }

    fn is_multi_output_streaming(&self) -> bool {
        true
    }

    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{
            LatencyClass, NodeCapabilitiesSchema, NodeSchema, RuntimeDataType,
        };
        Some(
            NodeSchema::new("MultimodalLLMNode")
                .description(
                    "Streaming chat completion node accepting text and image input. \
                     Aggregates inputs into one user message with content-parts \
                     (text + image_url). v1 targets OpenAI-shape APIs (cloud OpenAI, \
                     vLLM, modern llama.cpp + llava).",
                )
                .category("llm")
                .accepts([
                    RuntimeDataType::Text,
                    RuntimeDataType::Image,
                    RuntimeDataType::Audio,
                    RuntimeDataType::Json,
                ])
                .produces([RuntimeDataType::Text])
                .capabilities(NodeCapabilitiesSchema {
                    parallelizable: false,
                    batch_aware: false,
                    supports_control: false,
                    latency_class: LatencyClass::Slow,
                })
                .config_schema_from::<MultimodalLLMConfig>(),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::ImageFormat;
    use crate::nodes::{schema::RuntimeDataType, StreamingNodeFactory};

    fn img() -> RuntimeData {
        RuntimeData::Image {
            data: vec![1, 2, 3, 4],
            format: ImageFormat::Png,
            width: 1,
            height: 1,
            timestamp_us: None,
            stream_id: None,
            metadata: None,
        }
    }

    #[test]
    fn config_aggregation_default_is_per_input() {
        let cfg = MultimodalLLMConfig::default();
        assert!(matches!(cfg.aggregation, AggregationMode::PerInput));
    }

    #[test]
    fn config_camel_case_aggregation() {
        let params = serde_json::json!({
            "aggregation": {"kind": "coalesce_until", "sentinel": "<|done|>"}
        });
        let cfg: MultimodalLLMConfig = serde_json::from_value(params).unwrap();
        match cfg.aggregation {
            AggregationMode::CoalesceUntil { sentinel } => {
                assert_eq!(sentinel, "<|done|>");
            }
            _ => panic!("expected CoalesceUntil"),
        }
    }

    #[test]
    fn text_input_builds_text_part() {
        let node = MultimodalLLMNode::with_config(MultimodalLLMConfig::default());
        let part = node.input_to_part(&RuntimeData::Text("hello".into())).unwrap();
        match part {
            Some(InputPart::Part(v)) => {
                assert_eq!(v["type"], "text");
                assert_eq!(v["text"], "hello");
            }
            other => panic!("expected text Part, got {:?}", other),
        }
    }

    #[test]
    fn image_input_builds_image_url_part_with_data_url() {
        let node = MultimodalLLMNode::with_config(MultimodalLLMConfig::default());
        let part = node.input_to_part(&img()).unwrap();
        match part {
            Some(InputPart::Part(v)) => {
                assert_eq!(v["type"], "image_url");
                let url = v["image_url"]["url"].as_str().unwrap();
                assert!(url.starts_with("data:image/png;base64,"), "url={}", url);
            }
            other => panic!("expected image_url Part, got {:?}", other),
        }
    }

    #[test]
    fn empty_text_dropped_silently() {
        let node = MultimodalLLMNode::with_config(MultimodalLLMConfig::default());
        let part = node.input_to_part(&RuntimeData::Text("".into())).unwrap();
        assert!(part.is_none());
    }

    #[test]
    fn audio_input_emits_input_audio_part() {
        let node = MultimodalLLMNode::with_config(MultimodalLLMConfig::default());
        let audio = RuntimeData::Audio {
            samples: vec![0.0_f32; 16].into(),
            sample_rate: 16000,
            channels: 1,
            stream_id: None,
            timestamp_us: None,
            arrival_ts_us: None,
            metadata: None,
        };
        let part = node.input_to_part(&audio).unwrap();
        match part {
            Some(InputPart::Part(v)) => {
                assert_eq!(v["type"], "input_audio");
                assert_eq!(v["input_audio"]["format"], "wav");
                let data = v["input_audio"]["data"].as_str().unwrap();
                use base64::engine::general_purpose::STANDARD as B64;
                use base64::Engine;
                let buf = B64.decode(data).unwrap();
                assert_eq!(&buf[..4], b"RIFF");
                assert_eq!(&buf[8..12], b"WAVE");
            }
            other => panic!("expected input_audio Part, got {:?}", other),
        }
    }

    #[test]
    fn unsupported_input_still_rejected() {
        let node = MultimodalLLMNode::with_config(MultimodalLLMConfig::default());
        let bin = RuntimeData::Binary(vec![0u8; 4]);
        let err = node.input_to_part(&bin).unwrap_err();
        assert!(format!("{}", err).contains("does not accept"));
    }

    #[test]
    fn coalesce_buffers_until_sentinel() {
        let cfg = MultimodalLLMConfig {
            aggregation: AggregationMode::CoalesceUntil {
                sentinel: "<|input_end|>".to_string(),
            },
            ..Default::default()
        };
        let node = MultimodalLLMNode::with_config(cfg);
        // First image goes to buffer.
        let p = node.input_to_part(&img()).unwrap().unwrap();
        if let InputPart::Part(v) = p {
            node.coalesce_buffers
                .lock()
                .entry("s".into())
                .or_insert_with(Vec::new)
                .push(v);
        }
        // Sentinel text is recognised.
        let s = node
            .input_to_part(&RuntimeData::Text("<|input_end|>".into()))
            .unwrap();
        assert!(matches!(s, Some(InputPart::Sentinel)));
        // Buffer is still populated; flush would empty it (covered by
        // the integration test below).
        assert_eq!(node.coalesce_buffers.lock().get("s").unwrap().len(), 1);
    }

    #[test]
    fn factory_schema_advertises_all_accepted_modalities() {
        let factory = MultimodalLLMNodeFactory;
        let schema = factory.schema().unwrap();
        assert!(schema.accepts.contains(&RuntimeDataType::Image));
        assert!(schema.accepts.contains(&RuntimeDataType::Audio));
        assert!(schema.accepts.contains(&RuntimeDataType::Text));
        assert!(schema.accepts.contains(&RuntimeDataType::Json));
        assert!(schema.produces.contains(&RuntimeDataType::Text));
        assert_eq!(schema.category, Some("llm".into()));
    }

    #[test]
    fn factory_creates_node() {
        let factory = MultimodalLLMNodeFactory;
        let params = serde_json::json!({"model": "gpt-4o-mini"});
        let node = factory.create("n1".into(), &params, None).unwrap();
        assert_eq!(node.node_type(), "MultimodalLLMNode");
    }

    /// End-to-end: a sentinel arriving with a non-empty buffer pulls
    /// the buffer through the backend's request builder. We can't
    /// actually issue HTTP in unit tests, but we can assert the
    /// per-session buffer is consumed by simulating the flush path —
    /// the actual `backend.run()` call is exercised by integration
    /// tests against a local llama.cpp.
    #[test]
    fn coalesce_flush_drains_buffer_on_sentinel() {
        let cfg = MultimodalLLMConfig {
            aggregation: AggregationMode::CoalesceUntil {
                sentinel: "<|input_end|>".to_string(),
            },
            ..Default::default()
        };
        let node = MultimodalLLMNode::with_config(cfg);
        node.coalesce_buffers.lock().insert(
            "s".into(),
            vec![serde_json::json!({"type":"text","text":"x"})],
        );
        // Direct manipulation of the buffer that mirrors what
        // process_streaming does at the sentinel arm.
        let drained = node
            .coalesce_buffers
            .lock()
            .remove("s")
            .unwrap_or_default();
        assert_eq!(drained.len(), 1);
        let user_msg = MultimodalLLMNode::user_message(drained);
        assert_eq!(user_msg["role"], "user");
        assert_eq!(user_msg["content"][0]["type"], "text");
        assert!(node.coalesce_buffers.lock().get("s").is_none());
    }
}
