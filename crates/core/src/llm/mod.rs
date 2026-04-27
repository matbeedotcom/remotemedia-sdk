//! LLM transport layer shared across chat-completion-shaped LLM nodes.
//!
//! Modality-agnostic: the backend transports a `messages` array plus
//! tools and emits streaming `RuntimeData::Text` outputs. The owning
//! node (e.g. [`crate::nodes::openai_chat::OpenAIChatNode`]) is
//! responsible for shaping each input modality into a chat-completions
//! `user` message before handing it off.
//!
//! Two layers:
//!
//! - [`ChatBackend`] — owns history, HTTP client, SSE parser, tool
//!   dispatch.
//! - [`ProviderProfile`] — vendor wire-shaping (endpoint URL, auth
//!   header, request body, SSE chunk parsing). v1 ships
//!   [`OpenAIProfile`]; Anthropic / Gemini land in step 5 of the
//!   multimodal-LLM plan.

pub mod audio_encode;
pub mod chat_backend;
pub mod data_url;
pub mod history;
pub mod provider;
pub mod tool_dispatch;

pub use chat_backend::{ChatBackend, ChatBackendConfig};
pub use history::HistoryEntry;
pub use provider::{ChatRequest, ChatStreamEvent, OpenAIProfile, ProviderProfile};
pub use tool_dispatch::{dispatch_tool_call, ToolCallAccum};
