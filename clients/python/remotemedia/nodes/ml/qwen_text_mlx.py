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
from typing import Any, AsyncGenerator, Dict, List, Optional, Union

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
    _ML_DEPS_AVAILABLE = True
except BaseException as _exc:  # noqa: BLE001 — broken installs raise everything
    _ML_DEPS_AVAILABLE = False
    _ML_IMPORT_ERROR = _exc
    mx = None  # type: ignore
    _mlxlm_load = None  # type: ignore
    _mlxlm_stream_generate = None  # type: ignore
    _mlxlm_make_sampler = None  # type: ignore
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

        self.name = name or config.node_id
        self.is_streaming = True
        logger.info("QwenTextMlxNode configured (repo=%s)", self.hf_repo)

    # ────── In-process control-plane-analog API ───────────────────────

    def set_context(self, docs: str) -> None:
        self._context = docs or ""
        logger.debug("[%s] context set (%d chars)", self.node_id, len(self._context))

    def clear_context(self) -> None:
        self._context = ""

    def set_system_prompt(self, prompt: str) -> None:
        self._system_prompt = prompt or DEFAULT_SYSTEM_PROMPT

    def reset_history(self) -> None:
        self._history.clear()
        logger.info("[%s] history reset", self.node_id)

    def request_barge_in(self) -> None:
        self._interrupt = True
        logger.info("[%s] barge-in requested", self.node_id)

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
        messages = self._build_messages(user_text)

        try:
            # mlx-lm's tokenizer wrapper mirrors HuggingFace's
            # `apply_chat_template`: give it a list of role/content
            # dicts and get back a ready-to-tokenize prompt string.
            #
            # `enable_thinking=False` suppresses Qwen3.5's
            # chain-of-thought preamble (the "Thinking Process:" /
            # "**Analyze the Request:**" blocks that otherwise leak
            # into the text stream and get fed to the TTS as spoken
            # scratchpad). Older tokenizers don't understand the
            # kwarg — catch the TypeError and retry without it so we
            # stay compatible across Qwen releases.
            try:
                prompt = self._processor.apply_chat_template(
                    messages,
                    add_generation_prompt=True,
                    tokenize=False,
                    enable_thinking=False,
                )
            except TypeError:
                prompt = self._processor.apply_chat_template(
                    messages, add_generation_prompt=True, tokenize=False
                )
        except Exception as exc:  # noqa: BLE001
            logger.error("[%s] chat-template build failed: %s", self.node_id, exc)
            yield RuntimeData.text(f"[template-error: {exc}]")
            yield RuntimeData.text("<|text_end|>")
            return

        logger.info(
            "[%s] generating reply (history_turns=%d, ctx=%d chars)",
            self.node_id, len(self._history), len(self._context),
        )

        # Run the blocking streamer on a worker thread and bridge
        # into async via an asyncio.Queue. mlx-lm's stream_generate
        # is a plain Python generator and would starve the event
        # loop if iterated directly here.
        loop = asyncio.get_running_loop()
        queue: asyncio.Queue = asyncio.Queue()
        _SENTINEL = object()
        _ERROR = object()

        # mlx-lm 0.19+ removed the `temp=` / `top_p=` kwargs on
        # `stream_generate` in favour of an externally-built
        # `sampler` callable. Keep a fallback path for older builds.
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

        def _produce() -> None:
            try:
                kwargs: Dict[str, Any] = {
                    "max_tokens": self.max_new_tokens,
                }
                if sampler is not None:
                    kwargs["sampler"] = sampler
                else:
                    # Legacy kwargs for older mlx-lm releases.
                    kwargs["temp"] = self.temperature
                    kwargs["top_p"] = self.top_p
                gen = _mlxlm_stream_generate(
                    self._model, self._processor, prompt, **kwargs
                )
                for chunk in gen:
                    # Barge-in checkpoint. `mlx_lm.stream_generate` is a
                    # blocking Python generator; we can't cancel its
                    # decoder mid-token, but we can break the loop at
                    # each yielded chunk so no further tokens reach the
                    # TTS. Without this check the LLM keeps emitting
                    # tokens after barge-in — the consumer stops
                    # reading, but the producer runs to completion.
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

        producer = asyncio.create_task(asyncio.to_thread(_produce))
        reply_parts: List[str] = []
        try:
            while True:
                if self._interrupt:
                    logger.info("[%s] barge-in latched — halting reply stream", self.node_id)
                    self._interrupt = False
                    break
                item = await queue.get()
                if item is _SENTINEL:
                    break
                if isinstance(item, tuple) and len(item) == 2 and item[0] is _ERROR:
                    exc = item[1]
                    logger.error("[%s] generation error: %s", self.node_id, exc)
                    yield RuntimeData.text(f"[generation-error: {exc}]")
                    break
                if not isinstance(item, str):
                    continue
                reply_parts.append(item)
                yield RuntimeData.text(item)
                await asyncio.sleep(0)
        finally:
            if not producer.done():
                # Producer cannot be cancelled from outside (blocking
                # generator) — wait it out; it'll terminate on its
                # next tick when the generator completes.
                try:
                    await producer
                except Exception:  # noqa: BLE001
                    pass

        full_reply = "".join(reply_parts).strip()
        if full_reply:
            self._history.append({"role": "user", "content": user_text})
            self._history.append({"role": "assistant", "content": full_reply})

        # Sentinel tells downstream TTS to flush any buffered text.
        # Matches the contract used by LFM2AudioMlxNode.
        yield RuntimeData.text("<|text_end|>")

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
        }


__all__ = ["QwenTextMlxNode"]
