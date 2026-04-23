"""
Qwen text-chat node — MLX backend for Apple Silicon.

Streams text tokens from an MLX-quantised Qwen checkpoint via the
``mlx-lm`` package. Counterpart to :class:`LFM2TextNode` (torch) with
the same aux-port envelope surface so it slots into the same
control-bus-driven demo plumbing:

    audio.in.context        → store RAG context applied on next turn
    audio.in.system_prompt  → replace persona
    audio.in.reset          → drop conversation history
    audio.in.barge_in       → interrupt current generation

Inputs:  ``RuntimeData.Text`` (one user turn — e.g. a Whisper transcript)
Outputs: streamed ``RuntimeData.Text`` chunks during generation, then
         a final ``RuntimeData.Text("<|text_end|>")`` sentinel so the
         downstream TTS node knows the reply is complete and can flush
         the remaining text to audio.

This node is intentionally paired with a dedicated TTS node; it does
NOT emit audio itself (unlike :class:`LFM2AudioMlxNode`).

## Requirements

    pip install mlx-lm numpy

Uses ``mlx-lm`` rather than ``mlx-vlm``: ``mlx-vlm`` is the vision-language
port and only registers a subset of model archs (e.g. no ``qwen3_5`` /
``qwen2``), so plain Qwen text checkpoints fail to load under it with
``Model type ... not supported``. ``mlx-lm`` covers every text Qwen
variant Liquid/Alibaba have released on the ``mlx-community`` HF org.
"""

from __future__ import annotations

import asyncio
import json
import logging
from dataclasses import dataclass, field
from typing import (
    Any,
    AsyncGenerator,
    Callable,
    Dict,
    Iterable,
    List,
    Literal,
    Optional,
    Set,
    Tuple,
    Union,
)

_ML_IMPORT_ERROR: Optional[BaseException] = None
try:
    # mlx-lm's public API. `stream_generate` yields GenerationResponse
    # objects with a `.text` attribute (per-chunk decoded string).
    import mlx.core as mx  # noqa: F401
    from mlx_lm import load as _mlxlm_load
    from mlx_lm import stream_generate as _mlxlm_stream_generate
    try:
        # Newer mlx-lm releases expose sampling helpers via
        # `mlx_lm.sample_utils`. The streamer accepts either a
        # pre-built `sampler` callable or the raw temp/top_p kwargs,
        # but kwarg form has been removed in 0.19+, so prefer the
        # sampler path when available.
        from mlx_lm.sample_utils import make_sampler as _mlxlm_make_sampler
    except Exception:  # noqa: BLE001
        _mlxlm_make_sampler = None  # type: ignore
    try:
        # KV-cache reuse across turns. Without this, every reply re-prefills
        # the full chat template (system + context + history + user), which
        # dominates first-token latency on long conversations.
        from mlx_lm.models.cache import make_prompt_cache as _mlxlm_make_prompt_cache
    except Exception:  # noqa: BLE001
        _mlxlm_make_prompt_cache = None  # type: ignore
    _ML_DEPS_AVAILABLE = True
except BaseException as _exc:  # noqa: BLE001 — broken installs raise everything
    _ML_DEPS_AVAILABLE = False
    _ML_IMPORT_ERROR = _exc
    mx = None  # type: ignore
    _mlxlm_load = None  # type: ignore
    _mlxlm_stream_generate = None  # type: ignore
    _mlxlm_make_sampler = None  # type: ignore
    _mlxlm_make_prompt_cache = None  # type: ignore
    logging.getLogger(__name__).warning(
        "QwenTextMlxNode mlx-lm imports failed (%s): %s",
        type(_exc).__name__, _exc,
    )

try:
    from remotemedia.core.multiprocessing.data import RuntimeData
    _HAS_RUNTIME_DATA = True
except ImportError:
    _HAS_RUNTIME_DATA = False
    RuntimeData = None  # type: ignore

from remotemedia.core.multiprocessing import (
    MultiprocessNode,
    NodeConfig,
    python_requires,
    register_node,
)

logger = logging.getLogger(__name__)


DEFAULT_HF_REPO = "mlx-community/Qwen3.5-9B-MLX-4bit"
DEFAULT_SYSTEM_PROMPT = (
    "You are a helpful, concise voice assistant. Answer in 1-3 short sentences "
    "unless the user asks for detail. Avoid lists and markdown — your reply "
    "will be spoken aloud."
)
AUX_PORT_KEY = "__aux_port__"

# Display-channel envelope emitted for non-tool text when tools are active.
# Consumers (e.g. a web UI) subscribe to this port; the existing TTS node
# treats it as an unknown aux port and skips it, which is exactly the
# routing we want — markdown doesn't reach the speaker.
DISPLAY_AUX_PORT = "display"

# Fallback tool-call markers. mlx-lm's Qwen tokenizer exposes these as
# `tokenizer.tool_call_start` / `tool_call_end`, but we also want this
# node to degrade gracefully on older builds or custom tokenizers.
_DEFAULT_TOOL_CALL_START = "<tool_call>"
_DEFAULT_TOOL_CALL_END = "</tool_call>"


@dataclass
class ToolSpec:
    """Schema + dispatch hint for a tool the LLM may call.

    ``kind`` controls what happens after the model emits the tool call:

    - ``"side_effect"``: the node handles it inline (e.g. ``say`` yields
      the ``text`` argument as audible-TTS output). No result is fed
      back to the model; the turn ends after the current generation
      pass. Good for "emit-only" plumbing where the tool IS the output.

    - ``"return_value"``: reserved. Intended for the classic
      two-pass Qwen flow (generate → execute handler → feed result
      back → regenerate). Not implemented in this first cut — adding
      a tool with this kind will currently log and be ignored at
      dispatch time.
    """

    name: str
    description: str
    parameters: Dict[str, Any]  # JSON-schema object
    kind: Literal["side_effect", "return_value"] = "side_effect"
    handler: Optional[Callable[..., Any]] = None


def _default_say_tool() -> ToolSpec:
    return ToolSpec(
        name="say",
        description=(
            "Speak a sentence aloud to the user. The REQUIRED `text` "
            "parameter is the exact words to speak — if you omit it or "
            "leave it empty, nothing is synthesised and the user hears "
            "silence. Put the actual words inside the tool call; never "
            "write them after it.\n\n"
            "Correct: say(text=\"Hi Mathieu, here's your script.\")\n"
            "Wrong:   say()  followed by text outside the call.\n\n"
            "Use `say` for anything the user should HEAR: greetings, "
            "conversational answers, short summaries, confirmations. "
            "Use plain prose only — no markdown, no code, no lists."
        ),
        parameters={
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": (
                        "The words to speak aloud. MUST be a non-empty "
                        "string of plain prose. Example: \"Sure thing, "
                        "here's the Python script.\""
                    ),
                    "minLength": 1,
                },
            },
            "required": ["text"],
        },
        kind="side_effect",
    )


def _default_show_tool() -> ToolSpec:
    return ToolSpec(
        name="show",
        description=(
            "Display written content to the user as markdown. The REQUIRED "
            "`content` parameter is the markdown text itself — if you omit "
            "it or leave it empty, nothing is rendered. Put all written "
            "content inside the tool call; never write it after the call.\n\n"
            "Correct: show(content=\"```python\\ndef hi(): ...\\n```\")\n"
            "Wrong:   show()  followed by markdown outside the call.\n\n"
            "Use `show` for anything the user should READ rather than hear: "
            "code blocks (triple-backtick fences with a language tag), "
            "tables, lists, file paths, long explanations, command output."
        ),
        parameters={
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": (
                        "The markdown text to render. MUST be a non-empty "
                        "string. Example: \"```python\\nprint('hi')\\n```\""
                    ),
                    "minLength": 1,
                },
            },
            "required": ["content"],
        },
        kind="side_effect",
    )


class _ToolCallStreamParser:
    """Splits a streaming text feed into display text and tool-call bodies.

    State machine:
        OUTSIDE: accumulating user-facing text; watching for ``start_tag``
        INSIDE:  accumulating tool-call body; watching for ``end_tag``

    While OUTSIDE we hold back ``len(start_tag) - 1`` tail chars in the
    buffer so a tag split across streaming chunks is still detected.
    Over-conservative (may hold a few extra safe chars) but never emits
    a partial tag. For ``<tool_call>`` (11 chars) the worst-case
    extra-latency is ~10 chars of display text — a few tokens.
    """

    def __init__(self, start_tag: str, end_tag: str) -> None:
        self.start_tag = start_tag or _DEFAULT_TOOL_CALL_START
        self.end_tag = end_tag or _DEFAULT_TOOL_CALL_END
        self.inside = False
        self.buffer = ""

    def feed(self, chunk: str) -> Tuple[str, List[str]]:
        """Consume a chunk. Returns (safe_display_text, completed_tool_bodies)."""
        if not chunk:
            return "", []
        self.buffer += chunk
        display_parts: List[str] = []
        tool_bodies: List[str] = []
        while True:
            if not self.inside:
                idx = self.buffer.find(self.start_tag)
                if idx == -1:
                    # Hold back potential partial-start-tag suffix.
                    hold = len(self.start_tag) - 1
                    safe_len = len(self.buffer) - hold
                    if safe_len > 0:
                        display_parts.append(self.buffer[:safe_len])
                        self.buffer = self.buffer[safe_len:]
                    break
                if idx > 0:
                    display_parts.append(self.buffer[:idx])
                self.buffer = self.buffer[idx + len(self.start_tag):]
                self.inside = True
            else:
                idx = self.buffer.find(self.end_tag)
                if idx == -1:
                    # Keep buffering; the body may arrive across many chunks.
                    break
                tool_bodies.append(self.buffer[:idx])
                self.buffer = self.buffer[idx + len(self.end_tag):]
                self.inside = False
        return "".join(display_parts), tool_bodies

    def flush(self) -> Tuple[str, List[str]]:
        """Drain any trailing state at end of generation."""
        if not self.inside:
            out = self.buffer
            self.buffer = ""
            return out, []
        # Unclosed tool call — likely truncated by max_tokens / barge-in.
        remaining = self.buffer
        self.buffer = ""
        self.inside = False
        logger.warning(
            "tool_call block at stream end had no closing tag; dropping %d chars",
            len(remaining),
        )
        return "", []


@register_node("QwenTextMlxNode")
@python_requires(
    [
        "mlx-lm==0.31.3",
        "numpy>=1.24",
    ]
)
class QwenTextMlxNode(MultiprocessNode):
    """Multi-turn text chat over MLX-Qwen, aux-port-aware."""

    def __init__(
        self,
        config: Union[NodeConfig, Dict[str, Any], None] = None,
        *,
        node_id: Optional[str] = None,
        name: Optional[str] = None,
        hf_repo: str = DEFAULT_HF_REPO,
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        max_new_tokens: int = 256,
        temperature: float = 0.7,
        top_p: float = 0.9,
        tools: Optional[Iterable[Union[ToolSpec, Dict[str, Any]]]] = None,
        enable_say_tool: bool = True,
        enable_show_tool: bool = True,
        active_tools: Optional[Iterable[str]] = None,
        emit_display_text: bool = True,
        max_tool_passes: int = 6,
        **kwargs: Any,
    ) -> None:
        if isinstance(config, str):
            raise TypeError(
                "QwenTextMlxNode requires NodeConfig or keyword-only params; "
                "bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "qwen_text_mlx",
                node_type="QwenTextMlxNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "qwen_text_mlx"),
                node_type=config.get("node_type", "QwenTextMlxNode"),
                params=config.get("params", {}),
            )

        super().__init__(config, **kwargs)

        params = config.params or {}
        self.hf_repo = params.get("hf_repo", hf_repo)
        self._system_prompt = params.get("system_prompt", system_prompt)
        self.max_new_tokens = int(params.get("max_new_tokens", max_new_tokens))
        self.temperature = float(params.get("temperature", temperature))
        self.top_p = float(params.get("top_p", top_p))

        self._model: Any = None
        self._processor: Any = None
        self._mlx_config: Any = None
        self._initialized = False

        self._context: str = ""
        self._history: List[Dict[str, str]] = []
        self._interrupt: bool = False
        self._prompt_cache: Any = None

        # Tool registry. ``_available_tools`` is the catalog of tools the
        # node knows how to describe to the model; ``_active_tool_names``
        # is the subset currently exposed via the chat template.
        self._available_tools: Dict[str, ToolSpec] = {}
        self._active_tool_names: Set[str] = set()

        enable_say_tool = bool(params.get("enable_say_tool", enable_say_tool))
        if enable_say_tool:
            say_spec = _default_say_tool()
            self._available_tools[say_spec.name] = say_spec

        enable_show_tool = bool(params.get("enable_show_tool", enable_show_tool))
        if enable_show_tool:
            show_spec = _default_show_tool()
            self._available_tools[show_spec.name] = show_spec

        raw_tools = params.get("tools", tools)
        if raw_tools:
            for spec in self._coerce_tool_specs(raw_tools):
                self._available_tools[spec.name] = spec

        raw_active = params.get("active_tools", active_tools)
        if raw_active is None:
            # Default: every registered tool is active.
            self._active_tool_names = set(self._available_tools.keys())
        else:
            self._active_tool_names = {
                str(n) for n in raw_active if str(n) in self._available_tools
            }

        # When False, non-tool display text (markdown) is silently dropped
        # instead of emitted as a `display` aux envelope. Use this in
        # pipelines whose downstream node (e.g. TextCollectorNode → TTS)
        # is NOT aux-port-aware and would otherwise try to speak the
        # envelope JSON. Defaults to True so a display-subscribing UI
        # sees the full reply.
        self._emit_display_text = bool(
            params.get("emit_display_text", emit_display_text)
        )

        # Multi-pass tool-call generation cap. Qwen's training terminates
        # a generation pass after emitting a `<tool_call>` block, expecting
        # a `{role:"tool", content:...}` turn with the tool's result
        # before resuming. For side-effect tools like `say` whose result
        # is vacuous, we inject an empty tool-result message and
        # regenerate — up to this many times per user turn — so the
        # model can keep talking across multiple `say` calls or
        # intersperse tool calls with markdown. A pass that ends with
        # no tool calls terminates the loop naturally.
        self._max_tool_passes = max(1, int(
            params.get("max_tool_passes", max_tool_passes)
        ))

        self.name = name or config.node_id
        self.is_streaming = True
        logger.info(
            "QwenTextMlxNode configured (repo=%s, active_tools=%s)",
            self.hf_repo, sorted(self._active_tool_names),
        )

    # ────── In-process control-plane-analog API ───────────────────────

    def _reset_prompt_cache(self) -> None:
        # System prompt / context / history changes invalidate the cached
        # prefix. The cache is rebuilt lazily on the next generation.
        self._prompt_cache = None

    def set_context(self, docs: str) -> None:
        self._context = docs or ""
        self._reset_prompt_cache()
        logger.debug("[%s] context set (%d chars)", self.node_id, len(self._context))

    def clear_context(self) -> None:
        self._context = ""
        self._reset_prompt_cache()

    def set_system_prompt(self, prompt: str) -> None:
        self._system_prompt = prompt or DEFAULT_SYSTEM_PROMPT
        self._reset_prompt_cache()

    def reset_history(self) -> None:
        self._history.clear()
        self._reset_prompt_cache()
        logger.info("[%s] history reset", self.node_id)

    def request_barge_in(self) -> None:
        self._interrupt = True
        logger.info("[%s] barge-in requested", self.node_id)

    # ────── Tool registry ────────────────────────────────────────────

    @staticmethod
    def _coerce_tool_specs(
        raw: Iterable[Union[ToolSpec, Dict[str, Any]]],
    ) -> List[ToolSpec]:
        specs: List[ToolSpec] = []
        for item in raw:
            if isinstance(item, ToolSpec):
                specs.append(item)
                continue
            if not isinstance(item, dict):
                logger.warning("ignoring non-dict tool spec of type %s", type(item).__name__)
                continue
            try:
                specs.append(
                    ToolSpec(
                        name=str(item["name"]),
                        description=str(item.get("description", "")),
                        parameters=item.get("parameters") or {
                            "type": "object",
                            "properties": {},
                        },
                        kind=item.get("kind", "side_effect"),
                    )
                )
            except KeyError as exc:
                logger.warning("ignoring malformed tool spec (missing %s): %r", exc, item)
        return specs

    def register_tool(self, spec: Union[ToolSpec, Dict[str, Any]]) -> None:
        """Add a tool to the catalog. Does NOT activate it — call set_active_tools
        (or enable_tool via aux port) to expose it to the model."""
        coerced = self._coerce_tool_specs([spec])
        if not coerced:
            return
        self._available_tools[coerced[0].name] = coerced[0]
        self._reset_prompt_cache()

    def unregister_tool(self, name: str) -> None:
        if name in self._available_tools:
            del self._available_tools[name]
        self._active_tool_names.discard(name)
        self._reset_prompt_cache()

    def enable_tool(self, name: str) -> None:
        if name not in self._available_tools:
            logger.warning("[%s] enable_tool: unknown tool %r (ignored)", self.node_id, name)
            return
        if name not in self._active_tool_names:
            self._active_tool_names.add(name)
            self._reset_prompt_cache()

    def disable_tool(self, name: str) -> None:
        if name in self._active_tool_names:
            self._active_tool_names.discard(name)
            self._reset_prompt_cache()

    def set_active_tools(self, names: Iterable[str]) -> None:
        wanted = {str(n) for n in names if str(n) in self._available_tools}
        if wanted != self._active_tool_names:
            self._active_tool_names = wanted
            self._reset_prompt_cache()

    def _active_tool_schemas(self) -> List[Dict[str, Any]]:
        out: List[Dict[str, Any]] = []
        for name in sorted(self._active_tool_names):
            spec = self._available_tools.get(name)
            if spec is None:
                continue
            out.append(
                {
                    "type": "function",
                    "function": {
                        "name": spec.name,
                        "description": spec.description,
                        "parameters": spec.parameters,
                    },
                }
            )
        return out

    # ────── MultiprocessNode contract ─────────────────────────────────

    async def initialize(self) -> None:
        if not _ML_DEPS_AVAILABLE:
            cause = _ML_IMPORT_ERROR
            detail = (
                f"{type(cause).__name__}: {cause}" if cause is not None
                else "unknown import failure"
            )
            raise RuntimeError(
                f"QwenTextMlxNode mlx-lm stack failed to import — {detail}. "
                "Install `mlx-lm` on an Apple Silicon Mac."
            ) from cause
        if self._initialized:
            return

        logger.info("[%s] loading %s via mlx-lm", self.node_id, self.hf_repo)
        # `load` can download weights on first call; push to a thread so
        # we don't block the asyncio loop for tens of seconds.
        # mlx-lm returns (model, tokenizer). We store the tokenizer in
        # `_processor` to keep the rest of the attribute names stable.
        self._model, self._processor = await asyncio.to_thread(
            _mlxlm_load, self.hf_repo
        )
        self._mlx_config = getattr(self._model, "config", None)

        self._initialized = True
        logger.info("[%s] Qwen MLX model loaded", self.node_id)

    async def cleanup(self) -> None:
        self._model = None
        self._processor = None
        self._mlx_config = None
        self._history.clear()
        self._prompt_cache = None
        self._initialized = False

    # ────── Aux-port envelope handling ────────────────────────────────

    def _extract_envelope(self, data: Any) -> Optional[tuple]:
        blob = self._to_dict(data)
        if not isinstance(blob, dict):
            return None
        port = blob.get(AUX_PORT_KEY)
        if not isinstance(port, str) or not port:
            return None
        payload = blob.get("payload")
        if not isinstance(payload, dict):
            payload = {"text": str(payload)} if payload is not None else {}
        return port, payload

    def _to_dict(self, data: Any) -> Any:
        if isinstance(data, dict):
            return data
        if isinstance(data, str):
            stripped = data.strip()
            if stripped.startswith("{"):
                try:
                    return json.loads(stripped)
                except json.JSONDecodeError:
                    return None
            return None
        if _HAS_RUNTIME_DATA and RuntimeData is not None and isinstance(data, RuntimeData):
            try:
                if data.is_text():
                    return self._to_dict(data.as_text())
            except Exception:  # noqa: BLE001
                return None
        return None

    def _handle_aux_port(self, port: str, payload: Dict[str, Any]) -> None:
        if port == "context":
            text = payload.get("text")
            if isinstance(text, str):
                self.set_context(text)
        elif port == "system_prompt":
            text = payload.get("text")
            if isinstance(text, str):
                self.set_system_prompt(text)
        elif port == "reset":
            self.reset_history()
        elif port == "barge_in":
            self.request_barge_in()
        elif port == "enable_tool":
            name = payload.get("name") or payload.get("text")
            if isinstance(name, str) and name:
                self.enable_tool(name)
        elif port == "disable_tool":
            name = payload.get("name") or payload.get("text")
            if isinstance(name, str) and name:
                self.disable_tool(name)
        elif port == "set_active_tools":
            names = payload.get("names")
            if isinstance(names, list):
                self.set_active_tools(names)
        elif port == "register_tool":
            # Spec-only registration (no callable). Useful for declaring
            # tools the UI will surface to the model without any in-node
            # handler. `say` and other side-effect tools are handled
            # internally; return-value tools declared this way will
            # currently log-and-ignore on dispatch (no handler wired).
            spec = payload.get("spec") or payload
            self.register_tool(spec)
            name = payload.get("name") if isinstance(payload.get("name"), str) else None
            if name:
                self.enable_tool(name)
        else:
            logger.warning(
                "[%s] unknown aux port %r on QwenTextMlxNode; payload ignored",
                self.node_id, port,
            )

    # ────── User-input coercion ───────────────────────────────────────

    @staticmethod
    def _coerce_user_turn(data: Any) -> Optional[str]:
        if isinstance(data, str):
            return data
        if isinstance(data, dict):
            for key in ("text", "content", "message"):
                if key in data and isinstance(data[key], str):
                    return data[key]
            return None
        if _HAS_RUNTIME_DATA and RuntimeData is not None and isinstance(data, RuntimeData):
            try:
                if data.is_text():
                    return data.as_text()
            except Exception:  # noqa: BLE001
                return None
        return None

    # ────── Main processing ───────────────────────────────────────────

    def _build_messages(self, user_text: str) -> List[Dict[str, str]]:
        sys_content = self._system_prompt
        if self._context:
            sys_content = (
                f"{self._system_prompt}\n\n"
                f"Known facts you must use when relevant:\n{self._context}"
            )
        msgs: List[Dict[str, str]] = [{"role": "system", "content": sys_content}]
        msgs.extend(self._history)
        msgs.append({"role": "user", "content": user_text})
        return msgs

    async def process(self, data: Any) -> AsyncGenerator[Any, None]:
        if not _HAS_RUNTIME_DATA or RuntimeData is None:
            logger.error("[%s] RuntimeData unavailable; refusing to process", self.node_id)
            return

        # ── Aux-port control envelope: stop before model work ──
        envelope = self._extract_envelope(data)
        if envelope is not None:
            port, payload = envelope
            logger.info(
                "[%s] aux-port envelope detected: port=%s payload=%r",
                self.node_id, port, payload,
            )
            self._handle_aux_port(port, payload)
            return

        user_text = self._coerce_user_turn(data)
        if not user_text or not user_text.strip():
            logger.debug("[%s] empty user turn; skipping", self.node_id)
            return

        self._interrupt = False
        tools_active = bool(self._active_tool_names)

        # Sampler + prompt cache are shared across passes of one turn.
        sampler = None
        if _mlxlm_make_sampler is not None:
            try:
                sampler = _mlxlm_make_sampler(
                    temp=self.temperature, top_p=self.top_p
                )
            except Exception as exc:  # noqa: BLE001
                logger.debug(
                    "[%s] make_sampler failed (%s); falling back to defaults",
                    self.node_id, exc,
                )
                sampler = None

        if self._prompt_cache is None and _mlxlm_make_prompt_cache is not None:
            try:
                self._prompt_cache = _mlxlm_make_prompt_cache(self._model)
                logger.debug("[%s] prompt cache initialised", self.node_id)
            except Exception as exc:  # noqa: BLE001
                logger.warning(
                    "[%s] make_prompt_cache failed (%s); falling back to uncached prefill",
                    self.node_id, exc,
                )
                self._prompt_cache = None
        prompt_cache_ref = self._prompt_cache

        # Tool-call stream tags (resolved once per turn).
        start_tag = getattr(self._processor, "tool_call_start", None) \
            or _DEFAULT_TOOL_CALL_START
        end_tag = getattr(self._processor, "tool_call_end", None) \
            or _DEFAULT_TOOL_CALL_END

        # History bookkeeping for the whole turn. We record the user turn
        # once, then append each assistant pass (with raw tool_call tags
        # preserved) plus the synthetic tool-result messages, so the next
        # user turn's chat template reflects what actually happened.
        turn_history_additions: List[Dict[str, str]] = [
            {"role": "user", "content": user_text}
        ]

        # Accumulated messages used to build the prompt on every pass.
        # Qwen3.5's chat template rejects a delta-only message list
        # (`No user query found in messages.`), so we pass the full
        # conversation each time. The prompt_cache handles efficiency:
        # mlx-lm prefix-matches the cached tokens and only prefills the
        # suffix that's actually new (the tool-result turn + the new
        # assistant marker). Also keep `tools=` declared on every pass
        # so the template renders the tool wrappers consistently.
        accumulated_messages: List[Dict[str, str]] = self._build_messages(user_text)
        max_passes = self._max_tool_passes if tools_active else 1

        pass_idx = 0
        turn_error: Optional[str] = None
        while pass_idx < max_passes:
            if self._interrupt:
                logger.info(
                    "[%s] barge-in latched before pass %d — halting",
                    self.node_id, pass_idx + 1,
                )
                self._interrupt = False
                break

            # ── Build this pass's prompt (always full conversation) ──
            try:
                tool_schemas = self._active_tool_schemas() if tools_active else []
                template_kwargs: Dict[str, Any] = {
                    "add_generation_prompt": True,
                    "tokenize": False,
                    "enable_thinking": False,
                }
                if tool_schemas:
                    template_kwargs["tools"] = tool_schemas
                try:
                    prompt = self._processor.apply_chat_template(
                        accumulated_messages, **template_kwargs
                    )
                except TypeError:
                    template_kwargs.pop("enable_thinking", None)
                    try:
                        prompt = self._processor.apply_chat_template(
                            accumulated_messages, **template_kwargs
                        )
                    except TypeError:
                        template_kwargs.pop("tools", None)
                        prompt = self._processor.apply_chat_template(
                            accumulated_messages, **template_kwargs
                        )
            except Exception as exc:  # noqa: BLE001
                logger.error(
                    "[%s] chat-template build failed on pass %d: %s",
                    self.node_id, pass_idx + 1, exc,
                )
                turn_error = f"[template-error: {exc}]"
                break

            logger.info(
                "[%s] generating pass %d/%d (history_turns=%d, ctx=%d chars)",
                self.node_id, pass_idx + 1, max_passes,
                len(self._history), len(self._context),
            )

            # ── Set up producer + parser for this pass ────────────
            loop = asyncio.get_running_loop()
            queue: asyncio.Queue = asyncio.Queue()
            _SENTINEL = object()
            _ERROR = object()
            pass_prompt = prompt  # freeze for the closure

            def _produce() -> None:
                try:
                    kwargs: Dict[str, Any] = {
                        "max_tokens": self.max_new_tokens,
                    }
                    if sampler is not None:
                        kwargs["sampler"] = sampler
                    else:
                        kwargs["temp"] = self.temperature
                        kwargs["top_p"] = self.top_p
                    if prompt_cache_ref is not None:
                        kwargs["prompt_cache"] = prompt_cache_ref
                    gen = _mlxlm_stream_generate(
                        self._model, self._processor, pass_prompt, **kwargs
                    )
                    for chunk in gen:
                        # Barge-in checkpoint on the producer side.
                        if self._interrupt:
                            break
                        text = getattr(chunk, "text", None)
                        if text is None and isinstance(chunk, str):
                            text = chunk
                        if not text:
                            continue
                        loop.call_soon_threadsafe(queue.put_nowait, text)
                except Exception as exc:  # noqa: BLE001
                    loop.call_soon_threadsafe(queue.put_nowait, (_ERROR, exc))
                finally:
                    loop.call_soon_threadsafe(queue.put_nowait, _SENTINEL)

            parser: Optional[_ToolCallStreamParser] = None
            if tools_active:
                parser = _ToolCallStreamParser(start_tag, end_tag)

            producer = asyncio.create_task(asyncio.to_thread(_produce))
            raw_parts_this_pass: List[str] = []
            dispatched_names_this_pass: List[str] = []
            pass_error: Optional[str] = None

            try:
                while True:
                    if self._interrupt:
                        logger.info(
                            "[%s] barge-in latched mid-pass — halting",
                            self.node_id,
                        )
                        self._interrupt = False
                        break
                    item = await queue.get()
                    if item is _SENTINEL:
                        break
                    if isinstance(item, tuple) and len(item) == 2 and item[0] is _ERROR:
                        exc = item[1]
                        logger.error("[%s] generation error: %s", self.node_id, exc)
                        pass_error = f"[generation-error: {exc}]"
                        break
                    if not isinstance(item, str):
                        continue
                    raw_parts_this_pass.append(item)

                    if parser is None:
                        yield RuntimeData.text(item)
                        await asyncio.sleep(0)
                        continue

                    display_text, tool_bodies = parser.feed(item)
                    if display_text and self._emit_display_text:
                        yield self._display_envelope(display_text)
                    for body in tool_bodies:
                        call = self._parse_tool_body(body)
                        if call is None:
                            continue
                        name = call.get("name")
                        if isinstance(name, str) and name in self._active_tool_names:
                            dispatched_names_this_pass.append(name)
                        async for out in self._dispatch_parsed_call(call):
                            yield out
                    await asyncio.sleep(0)
            finally:
                if not producer.done():
                    # Blocking generator — wait it out; it'll terminate
                    # on its next tick.
                    try:
                        await producer
                    except Exception:  # noqa: BLE001
                        pass

            # Drain parser tail.
            if parser is not None:
                tail_display, tail_bodies = parser.flush()
                if tail_display and self._emit_display_text:
                    yield self._display_envelope(tail_display)
                for body in tail_bodies:
                    call = self._parse_tool_body(body)
                    if call is None:
                        continue
                    name = call.get("name")
                    if isinstance(name, str) and name in self._active_tool_names:
                        dispatched_names_this_pass.append(name)
                    async for out in self._dispatch_parsed_call(call):
                        yield out

            # Record this pass's raw output in both the accumulated
            # message list (used to build the next pass's prompt) AND the
            # per-turn history additions (committed to persistent history
            # at end-of-turn). Raw content preserves the `<tool_call>`
            # tags verbatim — Qwen's assistant-role template renders
            # `content` as-is, so the tags flow through correctly.
            raw_this_pass = "".join(raw_parts_this_pass)
            if raw_this_pass.strip():
                assistant_msg = {"role": "assistant", "content": raw_this_pass}
                turn_history_additions.append(assistant_msg)
                accumulated_messages.append(assistant_msg)

            if pass_error:
                yield RuntimeData.text(pass_error)
                turn_error = pass_error
                break

            # ── Decide whether to loop ────────────────────────────
            if not dispatched_names_this_pass:
                # Model ended naturally; turn is complete.
                break
            if pass_idx + 1 >= max_passes:
                logger.warning(
                    "[%s] hit max_tool_passes=%d with tool calls still pending",
                    self.node_id, max_passes,
                )
                break
            if self._interrupt:
                self._interrupt = False
                break

            # Synthetic tool-result turn. `say` (and any other side-effect
            # tool in this first cut) has no return value, so content is
            # empty — the model just needs SOMETHING in the tool slot to
            # resume. For future return_value tools, plug real results in
            # here. Qwen's chat template renders these as
            # `<|im_start|>tool\n...<|im_end|>` wrappers.
            tool_msgs = [
                {"role": "tool", "name": n, "content": ""}
                for n in dispatched_names_this_pass
            ]
            turn_history_additions.extend(tool_msgs)
            accumulated_messages.extend(tool_msgs)
            pass_idx += 1

        # Commit turn history (always as a unit — either all or none, so
        # a mid-turn error doesn't leave dangling user turns).
        if turn_error is None and len(turn_history_additions) > 1:
            self._history.extend(turn_history_additions)

        # End-of-turn sentinel on both channels.
        if tools_active and self._emit_display_text:
            yield self._display_envelope("<|text_end|>")
        yield RuntimeData.text("<|text_end|>")

    # ────── Tool-call dispatch + output routing ──────────────────────

    def _display_envelope(self, text: str) -> Any:
        """Emit non-tool-call text on the `ui` channel. Downstream TTS
        skips synthesis for this channel and passes it through, so it
        reaches a display sink without being spoken."""
        return RuntimeData.text(text, channel="ui")

    def _parse_tool_body(self, body: str) -> Optional[Dict[str, Any]]:
        """Parse the JSON-ish text between tool_call_start/end.

        Prefer the tokenizer's own tool parser when available (it handles
        Qwen-family quirks like trailing newlines / leading commentary);
        fall back to plain JSON otherwise.
        """
        stripped = body.strip()
        parser = getattr(self._processor, "tool_parser", None)
        if callable(parser):
            try:
                parsed = parser(stripped)
                if isinstance(parsed, dict):
                    return parsed
            except Exception as exc:  # noqa: BLE001
                logger.debug(
                    "[%s] tokenizer.tool_parser failed (%s); falling back to json.loads",
                    self.node_id, exc,
                )
        try:
            parsed = json.loads(stripped)
            return parsed if isinstance(parsed, dict) else None
        except json.JSONDecodeError as exc:
            logger.warning(
                "[%s] unparseable tool_call body (%s): %r",
                self.node_id, exc, stripped[:200],
            )
            return None

    async def _dispatch_tool_call(self, body: str) -> AsyncGenerator[Any, None]:
        """Entry point: parse a raw tool_call body then dispatch."""
        call = self._parse_tool_body(body)
        if call is None:
            return
        async for out in self._dispatch_parsed_call(call):
            yield out

    async def _dispatch_parsed_call(
        self, call: Dict[str, Any]
    ) -> AsyncGenerator[Any, None]:
        """Dispatch a tool call that's already been JSON-parsed.

        The main loop calls this so it can also record the tool name
        (needed for building the follow-up `{role:"tool"}` turn).
        """
        name = call.get("name")
        raw_args = call.get("arguments")
        if not isinstance(name, str):
            logger.warning("[%s] tool_call missing `name`: %r", self.node_id, call)
            return

        # Normalise `arguments` while preserving the original shape for
        # fallback parsing. Qwen sometimes emits:
        #   arguments: {"text": "..."}        (canonical)
        #   arguments: "..."                  (raw string — treat as the primary arg)
        #   arguments: "{\"text\": \"...\"}"  (stringified JSON)
        #   arguments: null / missing, text at top level
        args: Dict[str, Any] = {}
        if isinstance(raw_args, dict):
            args = raw_args
        elif isinstance(raw_args, str):
            stripped = raw_args.strip()
            if stripped.startswith("{"):
                try:
                    parsed = json.loads(stripped)
                    if isinstance(parsed, dict):
                        args = parsed
                except json.JSONDecodeError:
                    pass
            # If still empty, keep the raw string around for tools that
            # can consume a single positional string.
            if not args:
                args = {"__raw__": raw_args}

        spec = self._available_tools.get(name)
        if spec is None or name not in self._active_tool_names:
            logger.warning(
                "[%s] model called inactive/unknown tool %r (active=%s)",
                self.node_id, name, sorted(self._active_tool_names),
            )
            return
        logger.info(
            "[%s] tool_call dispatched: name=%s args_keys=%s",
            self.node_id, name, list(args.keys()),
        )

        def _extract_string_arg(
            prefer: List[str],
        ) -> Optional[str]:
            """Pull the first non-empty string argument matching `prefer`,
            falling back to top-level call keys and finally to a raw
            `arguments` string. Lets us route even tool calls that the
            model shaped wrong."""
            for key in prefer:
                val = args.get(key)
                if isinstance(val, str) and val:
                    return val
            raw = args.get("__raw__")
            if isinstance(raw, str) and raw:
                return raw
            for key in prefer:
                val = call.get(key)
                if isinstance(val, str) and val:
                    return val
            return None

        if name == "say" and spec.kind == "side_effect":
            spoken = _extract_string_arg(
                ["text", "content", "message", "body", "spoken"]
            )
            if spoken:
                # Plain text on the default (tts) channel → flows into
                # the existing TTS contract unchanged. Each yield reaches
                # the next node (sentencer → TTS) in real time — no
                # end-of-turn buffering.
                yield RuntimeData.text(spoken)
            else:
                logger.warning(
                    "[%s] `say` tool call had no recognisable text arg; "
                    "args=%r call=%r — nothing to synthesise",
                    self.node_id, args, call,
                )
            return

        if name == "show" and spec.kind == "side_effect":
            written = _extract_string_arg(
                ["content", "markdown", "text", "body"]
            )
            if written:
                # Markdown/code → ui channel. Downstream TTS skips
                # synthesis and the UI sink renders it.
                yield RuntimeData.text(written, channel="ui")
            else:
                logger.warning(
                    "[%s] `show` tool call had no recognisable content arg; "
                    "args=%r call=%r",
                    self.node_id, args, call,
                )
            return

        if spec.kind == "side_effect":
            # Generic side-effect tool w/ handler: call it, ignore return.
            if callable(spec.handler):
                # Drop the internal `__raw__` fallback key before
                # forwarding — handlers shouldn't see it.
                handler_args = {k: v for k, v in args.items() if k != "__raw__"}
                try:
                    result = spec.handler(**handler_args)
                    if asyncio.iscoroutine(result):
                        await result
                except Exception as exc:  # noqa: BLE001
                    logger.warning("[%s] tool %s handler raised: %s",
                                   self.node_id, name, exc)
            return

        # return_value tools require a second generation pass to feed
        # results back to the model. Not wired in this first cut —
        # surfaces cleanly rather than silently dropping.
        logger.warning(
            "[%s] return_value tool %r is not yet dispatched (multi-pass "
            "generation not implemented); skipping",
            self.node_id, name,
        )

    # ────── Introspection ─────────────────────────────────────────────

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "QwenTextMlxNode",
            "backend": "mlx-lm",
            "hf_repo": self.hf_repo,
            "system_prompt": self._system_prompt,
            "max_new_tokens": self.max_new_tokens,
            "temperature": self.temperature,
            "top_p": self.top_p,
            "context_len": len(self._context),
            "history_turns": len(self._history),
            "available_tools": sorted(self._available_tools.keys()),
            "active_tools": sorted(self._active_tool_names),
            "emit_display_text": self._emit_display_text,
            "max_tool_passes": self._max_tool_passes,
        }


__all__ = ["QwenTextMlxNode"]
