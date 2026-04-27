//! LLM tool-call schema + side-effect dispatch hints.
//!
//! Direct Rust port of the Python `ToolSpec` dataclass in
//! `clients/python/remotemedia/nodes/ml/qwen_text_mlx.py`. Field shape,
//! `kind` semantics, and the default `say` / `show` tool descriptions
//! are preserved verbatim so a model trained or prompted against the
//! Python descriptions behaves identically when routed through a Rust
//! LLM node (e.g. `OpenAIChatNode`).
//!
//! Layering:
//!
//! - [`ToolSpec`] is the wire-shape an LLM node uses to advertise a
//!   tool to the model and decide what to do when the model invokes
//!   it.
//! - [`ToolKind`] picks the dispatch contract:
//!   - [`ToolKind::SideEffect`] — the LLM node consumes the call inline
//!     (e.g. `say` yields its `text` argument as TTS-channel output).
//!     No tool-result is fed back to the model.
//!   - [`ToolKind::ReturnValue`] — reserved for the classic two-pass
//!     "generate → execute → feed result back → regenerate" flow. Not
//!     yet implemented in Rust streaming dispatch; declaring such a
//!     tool currently logs and is dropped at dispatch time.
//! - [`default_say_tool`] / [`default_show_tool`] are the canonical
//!   built-ins. The LLM node typically toggles them via config flags
//!   rather than asking callers to construct them.
//! - [`to_openai_tools_array`] renders a slice of specs as the
//!   `tools` field of an OpenAI chat-completions request body.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Dispatch contract for a tool call.
///
/// Mirrors the Python `Literal["side_effect", "return_value"]` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Tool is consumed inline by the LLM node — the call IS the
    /// output. No result is fed back to the model.
    SideEffect,
    /// Tool's return value should be fed back to the model on a second
    /// generation pass. Not implemented in the Rust SSE pipeline yet.
    ReturnValue,
}

impl Default for ToolKind {
    fn default() -> Self {
        Self::SideEffect
    }
}

/// Schema + dispatch hint for a tool the LLM may call.
///
/// `parameters` is a JSON-Schema object that gets passed verbatim to
/// the model inside the chat-completions `tools` array.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    #[serde(default)]
    pub kind: ToolKind,
}

impl ToolSpec {
    /// Render as one entry in an OpenAI chat-completions `tools`
    /// array: `{ "type": "function", "function": { name, description,
    /// parameters } }`.
    pub fn to_openai_function(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// Render a slice of specs as a JSON array suitable for the
/// chat-completions `tools` request field.
pub fn to_openai_tools_array(specs: &[ToolSpec]) -> Value {
    Value::Array(specs.iter().map(ToolSpec::to_openai_function).collect())
}

/// Built-in `say` tool. Description is kept identical to the Python
/// `_default_say_tool()` so prompt-time behaviour is the same across
/// Python and Rust LLM nodes.
pub fn default_say_tool() -> ToolSpec {
    ToolSpec {
        name: "say".to_string(),
        description: "Speak a sentence aloud to the user. The REQUIRED `text` \
parameter is the exact words to speak — if you omit it or \
leave it empty, nothing is synthesised and the user hears \
silence. Put the actual words inside the tool call; never \
write them after it.\n\n\
Correct: say(text=\"Hi Mathieu, here's your script.\")\n\
Wrong:   say()  followed by text outside the call.\n\n\
Use `say` for anything the user should HEAR: greetings, \
conversational answers, short summaries, confirmations. \
Use plain prose only — no markdown, no code, no lists."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description":
                        "The words to speak aloud. MUST be a non-empty \
string of plain prose. Example: \"Sure thing, here's the Python script.\"",
                    "minLength": 1
                }
            },
            "required": ["text"]
        }),
        kind: ToolKind::SideEffect,
    }
}

/// Built-in `show` tool. Description matches Python `_default_show_tool()`.
pub fn default_show_tool() -> ToolSpec {
    ToolSpec {
        name: "show".to_string(),
        description: "Display written content to the user as markdown. The REQUIRED \
`content` parameter is the markdown text itself — if you omit \
it or leave it empty, nothing is rendered. Put all written \
content inside the tool call; never write it after the call.\n\n\
Correct: show(content=\"```python\\ndef hi(): ...\\n```\")\n\
Wrong:   show()  followed by markdown outside the call.\n\n\
Use `show` for anything the user should READ rather than hear: \
code blocks (triple-backtick fences with a language tag), \
tables, lists, file paths, long explanations, command output."
            .to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description":
                        "The markdown text to render. MUST be a non-empty \
string. Example: \"```python\\nprint('hi')\\n```\"",
                    "minLength": 1
                }
            },
            "required": ["content"]
        }),
        kind: ToolKind::SideEffect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn say_tool_has_required_text_param() {
        let spec = default_say_tool();
        assert_eq!(spec.name, "say");
        assert_eq!(spec.kind, ToolKind::SideEffect);
        assert_eq!(spec.parameters["required"][0], "text");
    }

    #[test]
    fn show_tool_has_required_content_param() {
        let spec = default_show_tool();
        assert_eq!(spec.name, "show");
        assert_eq!(spec.parameters["required"][0], "content");
    }

    #[test]
    fn to_openai_function_shape() {
        let v = default_say_tool().to_openai_function();
        assert_eq!(v["type"], "function");
        assert_eq!(v["function"]["name"], "say");
        assert!(v["function"]["parameters"].is_object());
    }

    #[test]
    fn array_render_preserves_order() {
        let specs = vec![default_say_tool(), default_show_tool()];
        let arr = to_openai_tools_array(&specs);
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["function"]["name"], "say");
        assert_eq!(arr[1]["function"]["name"], "show");
    }
}
