"""
LFM2 text-generation node.

A pure-Python multi-turn LLM node built on Liquid AI's LFM2 family
(default: `LiquidAI/LFM2-350M`). This is the text-only sibling of
`lfm2_audio.py` — no audio, no speech synthesis, just chat-style
text generation with a persistent conversation history per session.

## Control-bus interop

The node exposes two methods that are the in-process analog of the
Session Control Bus operations (see `docs/SESSION_CONTROL.md`):

    node.set_context(docs)    ≡ ctrl.publish("llm.in.context", Data.text(...))
    node.clear_context()      ≡ ctrl.publish("llm.in.context", Data.text(""))
    node.set_system_prompt()  ≡ ctrl.publish("llm.in.system_prompt", Data.text(...))

Once the bus's gRPC transport lands, these same semantics will be
reachable from a remote client without any change to the node. Today
they are called directly in-process.
"""

from __future__ import annotations

import logging
from typing import Any, Dict, List, Optional

try:
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer
    _ML_DEPS_AVAILABLE = True
except ImportError:
    _ML_DEPS_AVAILABLE = False
    torch = None  # type: ignore
    AutoModelForCausalLM = None  # type: ignore
    AutoTokenizer = None  # type: ignore

from remotemedia.core.node import Node
from remotemedia.core.multiprocessing import python_requires, register_node

logger = logging.getLogger(__name__)


DEFAULT_SYSTEM_PROMPT = (
    "You are a helpful assistant. Answer briefly. "
    "If the user references information you have not been told, say so clearly."
)


@register_node("LFM2TextNode")
@python_requires([
    # Per Liquid AI deployment docs:
    # https://docs.liquid.ai/deployment/gpu-inference/transformers
    # LFM2 architecture is recognized in transformers >= 4.54.
    "transformers>=4.54.0",
    "torch>=2.1",
    "accelerate>=0.33",
])
class LFM2TextNode(Node):
    """
    Multi-turn text chat node backed by an LFM2 causal-LM checkpoint.

    Input: a user turn as `str` (or a dict with key `text`).
    Output: the assistant's reply as `str`.

    State carried across calls:
      - ``system_prompt``: the current system message.
      - ``context``: auxiliary context block injected via ``set_context``.
        Rendered as an additional system-turn prefix ahead of each user
        message (RAG-style).
      - ``history``: ordered list of ``{role, content}`` turns.

    Both ``context`` and ``system_prompt`` can be overwritten at any time.
    Overwriting does NOT replay the conversation — the new value only
    affects the next turn.
    """

    def __init__(
        self,
        name: Optional[str] = None,
        hf_repo: str = "LiquidAI/LFM2-350M",
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        device: Optional[str] = None,
        max_new_tokens: int = 120,
        temperature: float = 0.2,
        do_sample: bool = False,
        trust_remote_code: bool = True,
        **kwargs,
    ) -> None:
        super().__init__(name=name, **kwargs)
        self.hf_repo = hf_repo
        self._system_prompt = system_prompt
        self._context: str = ""
        self._history: List[Dict[str, str]] = []
        self._device = device
        self.max_new_tokens = max_new_tokens
        self.temperature = temperature
        self.do_sample = do_sample
        self.trust_remote_code = trust_remote_code
        self._tokenizer = None
        self._model = None

    # ────── Control-bus-analog API ─────────────────────────────────────

    def set_context(self, docs: str) -> None:
        """Replace the auxiliary context block. Applied on the next turn."""
        self._context = docs or ""
        logger.debug("[%s] context set (%d chars)", self.name, len(self._context))

    def clear_context(self) -> None:
        """Remove the auxiliary context block."""
        self._context = ""
        logger.debug("[%s] context cleared", self.name)

    def set_system_prompt(self, prompt: str) -> None:
        """Overwrite the system prompt. Applied on the next turn."""
        self._system_prompt = prompt
        logger.debug("[%s] system prompt updated", self.name)

    def reset_history(self) -> None:
        """Drop the conversation history (keeps system prompt + context)."""
        self._history.clear()
        logger.debug("[%s] history reset", self.name)

    # ────── Lifecycle ─────────────────────────────────────────────────

    def initialize(self) -> None:
        if self._is_initialized:
            return
        if not _ML_DEPS_AVAILABLE:
            raise RuntimeError(
                "LFM2TextNode requires `transformers` and `torch`. "
                "Install with: pip install 'transformers>=4.54' torch"
            )
        device = self._device or ("cuda" if torch.cuda.is_available() else "cpu")
        self._device = device
        logger.info("[%s] loading %s on %s", self.name, self.hf_repo, device)
        self._tokenizer = AutoTokenizer.from_pretrained(
            self.hf_repo, trust_remote_code=self.trust_remote_code
        )
        self._model = AutoModelForCausalLM.from_pretrained(
            self.hf_repo,
            trust_remote_code=self.trust_remote_code,
            torch_dtype=torch.float32 if device == "cpu" else torch.float16,
        ).to(device)
        self._model.eval()
        super().initialize()

    def cleanup(self) -> None:
        self._tokenizer = None
        self._model = None
        self._history.clear()
        super().cleanup()

    # ────── Core processing ───────────────────────────────────────────

    def process(self, data: Any) -> str:
        """Consume one user turn, append to history, return the reply."""
        if not self._is_initialized:
            self.initialize()

        user_text = self._coerce_input(data)
        self._history.append({"role": "user", "content": user_text})

        messages = self._build_messages()
        reply = self._generate(messages)

        self._history.append({"role": "assistant", "content": reply})
        return reply

    # ────── Internals ─────────────────────────────────────────────────

    @staticmethod
    def _coerce_input(data: Any) -> str:
        if isinstance(data, str):
            return data
        if isinstance(data, dict):
            for key in ("text", "content", "message"):
                if key in data and isinstance(data[key], str):
                    return data[key]
        raise TypeError(
            f"LFM2TextNode expects str or dict with 'text' key; got {type(data).__name__}"
        )

    def _build_messages(self) -> List[Dict[str, str]]:
        """Construct the message list fed to the chat template."""
        system_content = self._system_prompt
        if self._context:
            # Context is its own labeled section so the model knows it's
            # retrieved info, not prior conversation.
            system_content = (
                f"{self._system_prompt}\n\n"
                f"Known facts you must use when relevant:\n{self._context}"
            )
        return [{"role": "system", "content": system_content}, *self._history]

    def _generate(self, messages: List[Dict[str, str]]) -> str:
        assert self._tokenizer is not None and self._model is not None
        # transformers >= 5 returns a BatchEncoding (dict of tensors); older
        # releases returned a bare tensor. Normalize to a dict of tensors so
        # we can pass the attention mask through to `generate`.
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
            # Legacy path: plain tensor of input_ids.
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
        text = self._tokenizer.decode(new_tokens, skip_special_tokens=True).strip()
        return text
