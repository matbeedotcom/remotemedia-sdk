"""
LFM2 text-generation node — multiprocess-capable.

A multi-turn LLM node built on Liquid AI's LFM2 family
(default: ``LiquidAI/LFM2-350M``). This is the text-only sibling of
``lfm2_audio.py`` — no audio, no speech synthesis, just chat-style
text generation with a per-session conversation history.

## Dual-mode usage

- **In-process** — instantiate ``LFM2TextNode`` directly and call
  ``.process(data)`` from a Python pipeline. See
  ``clients/python/tests/test_lfm2_multi_turn_control.py``.

- **Via Rust multiprocess runner** — reference this class by
  ``node_type: LFM2TextNode`` in a pipeline manifest. The Rust server
  spawns a Python subprocess that runs ``remotemedia.core.multiprocessing.runner``,
  which imports this class out of the Python node registry. Control-
  bus ``publish("lfm.in.context", Data)`` frames from a remote client
  arrive here as aux-port envelopes and update the node's context
  between turns.

## Control-bus interop

Auxiliary-port publishes from a remote client arrive as a
``RuntimeData::Json`` (on the Rust side) / text-of-JSON (on the IPC)
payload shaped like::

    { "__aux_port__": "context", "payload": { "text": "docs..." } }

This node inspects every incoming payload for ``__aux_port__``:

- ``"context"``     → update ``self._context`` (no output yielded).
- ``"system_prompt"`` → update ``self._system_prompt`` (no output).
- missing           → treat the payload as a user turn.

That way the same node works identically whether context is fed via
the direct ``node.set_context()`` API (in-process tests) or via
``ctrl.publish("lfm.in.context", Data.from_text(...))`` (over gRPC).
"""

from __future__ import annotations

import json
import logging
from typing import Any, Dict, List, Optional, Union

try:
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer
    _ML_DEPS_AVAILABLE = True
except ImportError:
    _ML_DEPS_AVAILABLE = False
    torch = None  # type: ignore
    AutoModelForCausalLM = None  # type: ignore
    AutoTokenizer = None  # type: ignore

from remotemedia.core.multiprocessing import (
    MultiprocessNode,
    NodeConfig,
    python_requires,
    register_node,
)

try:
    from remotemedia.core.multiprocessing.data import RuntimeData
    _HAS_RUNTIME_DATA = True
except ImportError:
    _HAS_RUNTIME_DATA = False
    RuntimeData = None  # type: ignore

logger = logging.getLogger(__name__)


DEFAULT_SYSTEM_PROMPT = (
    "You are a helpful assistant. Answer briefly. "
    "If the user references information you have not been told, say so clearly."
)


AUX_PORT_KEY = "__aux_port__"


@register_node("LFM2TextNode")
@python_requires(
    [
        # Per Liquid AI deployment docs:
        # https://docs.liquid.ai/deployment/gpu-inference/transformers
        # LFM2 architecture requires transformers >= 4.54.
        "transformers>=4.54.0",
        "torch>=2.1",
        "accelerate>=0.33",
    ]
)
class LFM2TextNode(MultiprocessNode):
    """
    Multi-turn text chat node backed by an LFM2 causal-LM checkpoint.

    Subclassing ``MultiprocessNode`` makes this node runnable inside the
    Rust multiprocess Python executor. The same class also supports
    in-process use — construct with ``node_id=...`` and call ``.process()``.
    """

    # ────── Construction (dual-mode: in-process kwargs OR NodeConfig) ──

    def __init__(
        self,
        config: Union[NodeConfig, Dict[str, Any], None] = None,
        *,
        node_id: Optional[str] = None,
        name: Optional[str] = None,
        hf_repo: str = "LiquidAI/LFM2-350M",
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        device: Optional[str] = None,
        max_new_tokens: int = 120,
        temperature: float = 0.2,
        do_sample: bool = False,
        trust_remote_code: bool = True,
        **kwargs: Any,
    ) -> None:
        # The multiprocess runner first calls `node_class(node_id_string)`
        # (runner.py line 142), catches TypeError, then tries
        # `node_class(config=full_config)` (line 149). We deliberately
        # reject the string form with TypeError so the runner's fallback
        # lands us on the config-only path where manifest params reach us.
        if isinstance(config, str):
            raise TypeError(
                "LFM2TextNode requires NodeConfig or keyword-only params; "
                "bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "lfm2_text",
                node_type="LFM2TextNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "lfm2_text"),
                node_type=config.get("node_type", "LFM2TextNode"),
                params=config.get("params", {}),
            )

        super().__init__(config, **kwargs)

        # Manifest params override constructor defaults.
        params = config.params or {}
        self.hf_repo = params.get("hf_repo", hf_repo)
        self._system_prompt = params.get("system_prompt", system_prompt)
        self._device = params.get("device", device)
        self.max_new_tokens = int(params.get("max_new_tokens", max_new_tokens))
        self.temperature = float(params.get("temperature", temperature))
        self.do_sample = bool(params.get("do_sample", do_sample))
        self.trust_remote_code = bool(params.get("trust_remote_code", trust_remote_code))

        self._context: str = ""
        self._history: List[Dict[str, str]] = []
        self._tokenizer = None
        self._model = None

        # Friendly name used by Pipeline.get_node(name) for in-process tests.
        # Default to the config node_id; override with explicit `name=` kwarg.
        self.name = name or config.node_id

    # ────── In-process control-plane-analog API ───────────────────────

    def set_context(self, docs: str) -> None:
        """Replace the auxiliary context block. Applied on the next turn."""
        self._context = docs or ""
        logger.debug("[%s] context set (%d chars)", self.node_id, len(self._context))

    def clear_context(self) -> None:
        """Remove the auxiliary context block."""
        self._context = ""
        logger.debug("[%s] context cleared", self.node_id)

    def set_system_prompt(self, prompt: str) -> None:
        """Overwrite the system prompt. Applied on the next turn."""
        self._system_prompt = prompt
        logger.debug("[%s] system prompt updated", self.node_id)

    def reset_history(self) -> None:
        """Drop the conversation history (keeps system prompt + context)."""
        self._history.clear()
        logger.debug("[%s] history reset", self.node_id)

    # ────── MultiprocessNode contract ─────────────────────────────────

    async def initialize(self) -> None:
        if not _ML_DEPS_AVAILABLE:
            raise RuntimeError(
                "LFM2TextNode requires `transformers` and `torch`. "
                "Install with: pip install 'transformers>=4.54' torch"
            )
        device = self._device or ("cuda" if torch.cuda.is_available() else "cpu")
        self._device = device
        logger.info("[%s] loading %s on %s", self.node_id, self.hf_repo, device)
        self._tokenizer = AutoTokenizer.from_pretrained(
            self.hf_repo, trust_remote_code=self.trust_remote_code
        )
        self._model = AutoModelForCausalLM.from_pretrained(
            self.hf_repo,
            trust_remote_code=self.trust_remote_code,
            torch_dtype=torch.float32 if device == "cpu" else torch.float16,
        ).to(device)
        self._model.eval()
        logger.info("[%s] LFM2 model loaded", self.node_id)

    async def cleanup(self) -> None:
        self._tokenizer = None
        self._model = None
        self._history.clear()

    async def process(self, data: Any) -> Any:
        """Consume one user turn (or a control-plane envelope) and yield reply."""
        logger.info("[%s] process() received data: type=%s body=%r",
                    self.node_id, type(data).__name__, str(data)[:200])
        # ── Unwrap the aux-port envelope (control-bus publishes) ──
        envelope = self._extract_envelope(data)
        if envelope is not None:
            port, payload = envelope
            logger.info("[%s] aux-port envelope detected: port=%s payload=%r",
                        self.node_id, port, payload)
            self._handle_aux_port(port, payload)
            return None  # no user-visible output for control-plane frames

        # ── Otherwise, treat as a user turn ──
        user_text = self._coerce_user_turn(data)
        self._history.append({"role": "user", "content": user_text})
        reply = self._generate(self._build_messages())
        self._history.append({"role": "assistant", "content": reply})

        # Return as RuntimeData when available (the multiprocess runner needs
        # a RuntimeData-shaped result to ship back over IPC); in-process tests
        # keep the plain-string return path.
        if _HAS_RUNTIME_DATA and RuntimeData is not None:
            return RuntimeData.text(reply)
        return reply

    # ────── Envelope & input handling ─────────────────────────────────

    def _extract_envelope(self, data: Any) -> Optional[tuple]:
        """
        Inspect `data` for an aux-port envelope.

        Returns ``(port_name, payload_dict)`` if the payload is an aux-port
        envelope, else ``None``. Accepts:

        - a dict directly
        - a JSON string (from RuntimeData.Text transported over IPC)
        - a RuntimeData whose text / json body is one of the above
        """
        blob = self._to_dict(data)
        if blob is None:
            return None
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
        if _HAS_RUNTIME_DATA and isinstance(data, RuntimeData):
            try:
                if data.is_text():
                    return self._to_dict(data.as_text())
            except Exception:
                return None
        return None

    def _handle_aux_port(self, port: str, payload: Dict[str, Any]) -> None:
        if port == "context":
            text = payload.get("text")
            if isinstance(text, str):
                self.set_context(text)
            elif "context" in payload and isinstance(payload["context"], str):
                self.set_context(payload["context"])
        elif port == "system_prompt":
            text = payload.get("text")
            if isinstance(text, str):
                self.set_system_prompt(text)
        elif port == "reset":
            self.reset_history()
        else:
            logger.warning(
                "[%s] unknown aux port %r on LFM2TextNode; payload ignored",
                self.node_id, port,
            )

    @staticmethod
    def _coerce_user_turn(data: Any) -> str:
        if isinstance(data, str):
            return data
        if isinstance(data, dict):
            for key in ("text", "content", "message"):
                if key in data and isinstance(data[key], str):
                    return data[key]
        if _HAS_RUNTIME_DATA and isinstance(data, RuntimeData):
            try:
                if data.is_text():
                    return data.as_text()
            except Exception:
                pass
        raise TypeError(
            f"LFM2TextNode expected str/dict/RuntimeData.Text, got {type(data).__name__}"
        )

    # ────── Core generation ───────────────────────────────────────────

    def _build_messages(self) -> List[Dict[str, str]]:
        system_content = self._system_prompt
        if self._context:
            system_content = (
                f"{self._system_prompt}\n\n"
                f"Known facts you must use when relevant:\n{self._context}"
            )
        logger.info(
            "[%s] building messages: context=%r history_turns=%d",
            self.node_id, self._context[:60], len(self._history),
        )
        return [{"role": "system", "content": system_content}, *self._history]

    def _generate(self, messages: List[Dict[str, str]]) -> str:
        assert self._tokenizer is not None and self._model is not None
        encoded = self._tokenizer.apply_chat_template(
            messages,
            add_generation_prompt=True,
            return_tensors="pt",
            tokenize=True,
        )
        if hasattr(encoded, "data") and isinstance(getattr(encoded, "data", None), dict):
            inputs = {k: v.to(self._device) for k, v in encoded.items()}
        elif isinstance(encoded, dict):
            inputs = {k: v.to(self._device) for k, v in encoded.items()}
        else:
            inputs = {"input_ids": encoded.to(self._device)}

        input_len = inputs["input_ids"].shape[-1]

        with torch.inference_mode():
            output_ids = self._model.generate(
                **inputs,
                max_new_tokens=self.max_new_tokens,
                do_sample=self.do_sample,
                temperature=self.temperature if self.do_sample else None,
                pad_token_id=self._tokenizer.eos_token_id,
            )

        new_tokens = output_ids[0, input_len:]
        return self._tokenizer.decode(new_tokens, skip_special_tokens=True).strip()
