"""
Minimal whisper speech-to-text node — multiprocess-capable.

Accepts ``RuntimeData.Audio`` on the main input, emits one
``RuntimeData.Text`` per input containing the transcript. This is the
pipeline-side counterpart to the client-side whisper used in earlier
examples: the node runs inside the Rust server's multiprocess-Python
worker, so its output is addressable on the Session Control Bus just
like any other node (``subscribe("stt.out")``, intercept, disable, …).

The existing :mod:`remotemedia.nodes.ml.whisper_transcription` module
has a much richer feature set (delta updates, word timings, streaming
accuracy growth) but inherits from the base ``Node`` class, which the
multiprocess runner can't spawn directly. This wrapper is the thin,
batch-shaped sibling suitable for "transcribe one utterance, hand me
the text" use cases like conversation observability.
"""

from __future__ import annotations

import logging
from typing import Any, AsyncGenerator, Dict, Optional, Union

try:
    import numpy as np
    import torch  # noqa: F401  (transformers transitively needs it)
    from transformers import pipeline as _hf_pipeline
    _ML_DEPS_AVAILABLE = True
except ImportError:
    _ML_DEPS_AVAILABLE = False
    np = None  # type: ignore
    _hf_pipeline = None  # type: ignore

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

WHISPER_SAMPLE_RATE = 16000


# Whisper's well-documented "silence hallucinations" — phrases it emits
# on near-silent / ambient clips because they're high-probability
# completions in its training distribution. Passing these through as
# user turns to LFM2 causes the model to reply to nobody, which looks
# like the assistant "going crazy on its own". These are compared
# case-insensitive against the stripped, punctuation-trimmed transcript;
# any match is dropped.
#
# List is drawn from the failure modes reported against whisper-tiny
# on empty audio; extend here if you see others recur in the log.
_SILENCE_HALLUCINATIONS = frozenset(
    s.lower()
    for s in (
        "",
        ".",
        "you",
        "thank you",
        "thanks",
        "thanks for watching",
        "thanks for watching!",
        "thank you for watching",
        "bye",
        "bye.",
        "goodbye",
        "goodbye.",
        "okay",
        "ok",
        "hmm",
        "mhm",
        "uh",
        "um",
        "...",
        ". .",
        ". . .",
    )
)


def _is_likely_hallucination(text: str) -> bool:
    norm = text.strip().strip(".!? ").lower()
    return norm in _SILENCE_HALLUCINATIONS


def _extract_audio_fields(data: Any) -> "tuple[Any, int, int]":
    """
    Pull ``(samples_f32, sample_rate, channels)`` out of a RuntimeData.

    PyO3-backed ``RuntimeData`` exposes ``as_audio()``; the pure-Python
    fallback in :mod:`remotemedia.core.multiprocessing.data` does not
    and uses ``payload`` / ``metadata`` attributes. Handle both.
    """
    # PyO3 fast path.
    if hasattr(data, "as_audio"):
        samples_bytes, sr, channels, _fmt, _n = data.as_audio()
        return np.frombuffer(samples_bytes, dtype=np.float32), int(sr), int(channels)

    # Pure-Python fallback.
    payload = getattr(data, "payload", None)
    meta = getattr(data, "metadata", None)
    sr = int(getattr(meta, "sample_rate", 0) or 0)
    channels = int(getattr(meta, "channels", 1) or 1)

    if isinstance(payload, np.ndarray):
        arr = payload.astype(np.float32, copy=False).reshape(-1)
    elif isinstance(payload, (bytes, bytearray, memoryview)):
        arr = np.frombuffer(bytes(payload), dtype=np.float32)
    else:
        raise TypeError(
            f"WhisperSTTNode: cannot extract audio from RuntimeData payload "
            f"of type {type(payload).__name__}"
        )
    return arr, sr, channels


@register_node("WhisperSTTNode")
@python_requires(
    [
        "transformers>=4.40.0",
        "torch>=2.1",
        "accelerate>=0.33",
    ]
)
class WhisperSTTNode(MultiprocessNode):
    """
    One-shot speech-to-text via HuggingFace Whisper.

    One ``RuntimeData.Audio`` in → one ``RuntimeData.Text`` out. The
    node resamples to 16 kHz mono on the fly so upstream producers
    can emit at any rate / channel layout.
    """

    def __init__(
        self,
        config: Union[NodeConfig, Dict[str, Any], None] = None,
        *,
        node_id: Optional[str] = None,
        name: Optional[str] = None,
        model_id: str = "openai/whisper-tiny.en",
        language: Optional[str] = "en",
        device: Optional[str] = None,
        torch_dtype: str = "float32",
        chunk_length_s: int = 30,
        **kwargs: Any,
    ) -> None:
        if isinstance(config, str):
            raise TypeError(
                "WhisperSTTNode requires NodeConfig or keyword-only params; "
                "bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "whisper_stt",
                node_type="WhisperSTTNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "whisper_stt"),
                node_type=config.get("node_type", "WhisperSTTNode"),
                params=config.get("params", {}),
            )

        super().__init__(config, **kwargs)

        params = config.params or {}
        self.model_id = params.get("model_id", model_id)
        self.language = params.get("language", language)
        self._requested_device = params.get("device", device)
        self._requested_dtype = params.get("torch_dtype", torch_dtype)
        self.chunk_length_s = int(params.get("chunk_length_s", chunk_length_s))

        self._pipeline: Any = None
        self.name = name or config.node_id

    async def initialize(self) -> None:
        if not _ML_DEPS_AVAILABLE:
            raise RuntimeError(
                "WhisperSTTNode requires `transformers` and `torch`. "
                "Install with: pip install 'transformers>=4.40' torch"
            )

        device = self._requested_device
        if device is None:
            device = "cuda:0" if torch.cuda.is_available() else "cpu"

        dtype = getattr(torch, self._requested_dtype, torch.float32)
        # On CPU, float16 silently becomes float32 anyway and produces
        # noisy warnings — normalise to float32.
        if device == "cpu":
            dtype = torch.float32

        logger.info(
            "[%s] loading whisper '%s' on %s (dtype=%s)",
            self.node_id, self.model_id, device, dtype,
        )
        self._pipeline = _hf_pipeline(
            "automatic-speech-recognition",
            model=self.model_id,
            torch_dtype=dtype,
            device=device,
            chunk_length_s=self.chunk_length_s,
        )
        logger.info("[%s] whisper ready", self.node_id)

    async def cleanup(self) -> None:
        self._pipeline = None

    async def process(self, data: Any) -> AsyncGenerator[Any, None]:
        if not _HAS_RUNTIME_DATA or RuntimeData is None:
            return
        if not hasattr(data, "is_audio") or not data.is_audio():
            # LFM2-Audio fans out text tokens + `<|text_end|>` /
            # `<|audio_end|>` markers to its downstream `stt_out` via
            # the manifest connection. Those aren't transcribable —
            # drop silently at DEBUG so real misroutes (e.g. an
            # unexpected JSON frame) can still be noticed without
            # spamming WARN on every token.
            kind = getattr(data, "data_type", lambda: type(data).__name__)()
            logger.debug("[%s] dropping non-audio input (%s)", self.node_id, kind)
            return
        if self._pipeline is None:
            logger.error("[%s] pipeline not initialized", self.node_id)
            return

        audio, sr, channels = _extract_audio_fields(data)
        if audio.dtype != np.float32:
            audio = audio.astype(np.float32, copy=False)

        # Downmix to mono (average channels) if needed.
        if channels and channels > 1 and audio.size % channels == 0:
            audio = audio.reshape(-1, channels).mean(axis=1)

        # Resample to 16 kHz — whisper's native rate. Linear interp is
        # fine for transcription quality; if `resampy` is installed we
        # use that for better-sounding resamples.
        if sr != WHISPER_SAMPLE_RATE and audio.size > 0:
            try:
                import resampy
                audio = resampy.resample(
                    audio.astype(np.float32), int(sr), WHISPER_SAMPLE_RATE
                ).astype(np.float32)
            except ImportError:
                ratio = WHISPER_SAMPLE_RATE / float(sr)
                n_out = max(1, int(round(audio.size * ratio)))
                audio = np.interp(
                    np.linspace(0.0, audio.size - 1.0, n_out, dtype=np.float64),
                    np.arange(audio.size, dtype=np.float64),
                    audio.astype(np.float64),
                ).astype(np.float32)

        if audio.size == 0:
            return

        # HuggingFace pipeline accepts a raw float32 np.ndarray at
        # whisper's native rate. `return_timestamps=False` keeps the
        # payload a single text string.
        try:
            # English-only checkpoints (repo name ends in ".en") refuse
            # the `language=` / `task=` kwargs — their generation_config
            # isn't multilingual, and passing them raises
            # ValueError("Cannot specify `task` or `language` for an
            # English-only model..."). Only set language when the model
            # is multilingual.
            is_english_only = str(self.model_id).lower().endswith(".en")
            pipeline_kwargs: Dict[str, Any] = {"return_timestamps": False}
            # The transformers pipeline iterates `generate_kwargs` when
            # it's passed, so passing `None` blows up with
            # "'NoneType' object is not iterable". Only set the key
            # when we have an actual dict.
            if self.language and not is_english_only:
                pipeline_kwargs["generate_kwargs"] = {"language": self.language}

            result = self._pipeline(
                {"array": audio, "sampling_rate": WHISPER_SAMPLE_RATE},
                **pipeline_kwargs,
            )
        except Exception as e:  # noqa: BLE001
            logger.error("[%s] transcription failed: %s", self.node_id, e)
            return

        text = (result.get("text") or "").strip() if isinstance(result, dict) else str(result).strip()
        logger.info(
            "[%s] transcript (%d samples @ %dHz): %r",
            self.node_id, audio.size, WHISPER_SAMPLE_RATE, text[:120],
        )
        if not text:
            return
        if _is_likely_hallucination(text):
            logger.info(
                "[%s] dropping likely silence-hallucination: %r",
                self.node_id, text[:120],
            )
            return
        yield RuntimeData.text(text)
