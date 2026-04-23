"""
LFM2-Audio speech-to-speech node — MLX backend for Apple Silicon.

Sibling of :mod:`remotemedia.nodes.ml.lfm2_audio` (torch/liquid-audio).
This variant uses the MLX-quantised model published at
``mlx-community/LFM2.5-Audio-1.5B-4bit`` and Apple's ``mlx-audio`` Python
package, so it runs natively on M-series Macs without any CUDA/torch
install. Public surface (aux ports, setters, streaming output shape) is
kept identical to the torch node so pipelines can swap ``LFM2AudioNode``
→ ``LFM2AudioMlxNode`` with no client changes.

## Requirements

    pip install mlx-audio soundfile numpy

No torch, no liquid-audio. Model weights download on first use
(~1.95 GB for the 5-bit quantisation).

## Control-bus surface

Identical to the torch node — see ``lfm2_audio.py`` for the long
explanation. Aux ports accepted:

    audio.in.context        → store RAG context applied on next turn
    audio.in.system_prompt  → replace persona
    audio.in.reset          → drop conversation history
    audio.in.barge_in       → interrupt current generation
    audio.in                → one audio utterance (user turn)

Main output stream: interleaved ``RuntimeData.Text`` and
``RuntimeData.Audio`` (24 kHz mono float32) terminated by the markers
``<|text_end|>`` and ``<|audio_end|>``.
"""

from __future__ import annotations

import asyncio
import json
import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, AsyncGenerator, Dict, List, Optional, TYPE_CHECKING, Union

_ML_IMPORT_ERROR: Optional[BaseException] = None
try:
    import mlx.core as mx
    import numpy as np
    from mlx_audio.sts.models.lfm_audio import (
        ChatState,
        LFMModality,
        LFM2AudioModel,
        LFM2AudioProcessor,
    )
    _ML_DEPS_AVAILABLE = True
except BaseException as _exc:
    _ML_DEPS_AVAILABLE = False
    _ML_IMPORT_ERROR = _exc
    mx = None  # type: ignore
    np = None  # type: ignore
    ChatState = None  # type: ignore
    LFMModality = None  # type: ignore
    LFM2AudioModel = None  # type: ignore
    LFM2AudioProcessor = None  # type: ignore
    logging.getLogger(__name__).warning(
        "LFM2AudioMlxNode MLX imports failed (%s): %s",
        type(_exc).__name__, _exc,
    )

if TYPE_CHECKING:
    from remotemedia.core.multiprocessing.data import RuntimeData

try:
    # Only `RuntimeData` is required. `numpy_to_audio` /
    # `audio_to_numpy` are PyO3-only helpers not present in the
    # pure-Python fallback — asking for them turned a "PyO3 bindings
    # missing" warning into a hard ImportError.
    from remotemedia.core.multiprocessing.data import RuntimeData
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    RuntimeData = None  # type: ignore

from remotemedia.core.multiprocessing import (
    MultiprocessNode,
    NodeConfig,
    python_requires,
    register_node,
)

logger = logging.getLogger(__name__)


DEFAULT_HF_REPO = "mlx-community/LFM2.5-Audio-1.5B-4bit"
DEFAULT_SYSTEM_PROMPT = "Respond with interleaved text and audio."
AUX_PORT_KEY = "__aux_port__"


@dataclass
class ConversationState:
    """One live ``mlx_audio`` ``ChatState`` plus session bookkeeping."""

    session_id: str
    chat_state: Any  # mlx_audio ChatState
    created_at: datetime = field(default_factory=datetime.now)
    last_accessed: datetime = field(default_factory=datetime.now)
    turn_count: int = 0

    def touch(self) -> None:
        self.last_accessed = datetime.now()


@register_node("LFM2AudioMlxNode")
@python_requires(
    [
        # Apple's MLX port of LFM2-Audio. Pulls mlx, mlx-lm, and the
        # mimi/detokenizer assets needed to turn audio codebook tokens
        # back into waveforms.
        "mlx-audio>=0.1",
        "numpy>=1.24",
    ]
)
class LFM2AudioMlxNode(MultiprocessNode):
    """
    Multi-turn speech-to-speech node using the MLX build of LFM2-Audio.

    API-compatible with :class:`LFM2AudioNode` (torch variant) on the
    control-bus surface: same aux ports, same reply stream shape, same
    session-keyed conversation history. Swap ``node_type`` in the
    manifest to choose between the two backends.
    """

    # ────── Construction (dual-mode: in-process kwargs OR NodeConfig) ──

    def __init__(
        self,
        config: Union[NodeConfig, Dict[str, Any], None] = None,
        *,
        node_id: Optional[str] = None,
        name: Optional[str] = None,
        hf_repo: str = DEFAULT_HF_REPO,
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        audio_temperature: float = 1.0,
        audio_top_k: int = 4,
        max_new_tokens: int = 2048,
        sample_rate: int = 24000,
        session_timeout_minutes: int = 30,
        text_only: bool = False,
        audio_decode_interval: int = 25,
        **kwargs: Any,
    ) -> None:
        if isinstance(config, str):
            raise TypeError(
                "LFM2AudioMlxNode requires NodeConfig or keyword-only params; "
                "bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "lfm2_audio_mlx",
                node_type="LFM2AudioMlxNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "lfm2_audio_mlx"),
                node_type=config.get("node_type", "LFM2AudioMlxNode"),
                params=config.get("params", {}),
            )

        super().__init__(config, **kwargs)

        params = config.params or {}
        self.hf_repo = params.get("hf_repo", hf_repo)
        self._system_prompt = params.get("system_prompt", system_prompt)
        self.audio_temperature = float(params.get("audio_temperature", audio_temperature))
        self.audio_top_k = int(params.get("audio_top_k", audio_top_k))
        self.max_new_tokens = int(params.get("max_new_tokens", max_new_tokens))
        self.sample_rate = int(params.get("sample_rate", sample_rate))
        self.session_timeout_minutes = int(
            params.get("session_timeout_minutes", session_timeout_minutes)
        )
        self.text_only = bool(params.get("text_only", text_only))
        # How often (in audio tokens) we call `decode_with_detokenizer`
        # while streaming. Smaller → lower perceived latency, more
        # boundary-artifact risk; larger → smoother audio, more latency.
        # The MLX detokenizer is tuned for end-of-turn calls, so we pick
        # a chunky default that favours quality.
        self.audio_decode_interval = max(
            1, int(params.get("audio_decode_interval", audio_decode_interval))
        )

        self._processor: Any = None
        self._model: Any = None
        self._initialized = False

        self._sessions: Dict[str, ConversationState] = {}
        self._cleanup_task: Optional[asyncio.Task] = None

        # Aux-port state — shared shape with the torch node.
        self._context: str = ""
        self._interrupt: bool = False

        self.name = name or config.node_id
        self.is_streaming = True
        logger.info(
            "LFM2AudioMlxNode initialized (text_only=%s, repo=%s)",
            self.text_only, self.hf_repo,
        )

    # ────── In-process control-plane-analog API ───────────────────────

    def set_context(self, docs: str) -> None:
        self._context = docs or ""
        self._invalidate_sessions("context")

    def clear_context(self) -> None:
        self._context = ""
        self._invalidate_sessions("context-clear")

    def set_system_prompt(self, prompt: str) -> None:
        self._system_prompt = prompt or DEFAULT_SYSTEM_PROMPT
        self._invalidate_sessions("system-prompt")

    def reset_history(self) -> None:
        self._invalidate_sessions("reset")

    def request_barge_in(self) -> None:
        self._interrupt = True
        logger.info("[%s] barge-in requested", self.node_id)

    def _invalidate_sessions(self, reason: str) -> None:
        if self._sessions:
            logger.info(
                "[%s] dropping %d cached chat state(s) (%s)",
                self.node_id, len(self._sessions), reason,
            )
            self._sessions.clear()

    # ────── MultiprocessNode contract ─────────────────────────────────

    async def initialize(self) -> None:
        if not _ML_DEPS_AVAILABLE:
            cause = _ML_IMPORT_ERROR
            detail = (
                f"{type(cause).__name__}: {cause}" if cause is not None
                else "unknown import failure"
            )
            raise RuntimeError(
                f"LFM2AudioMlxNode MLX stack failed to import — {detail}. "
                "Install `mlx-audio` on an Apple Silicon Mac."
            ) from cause
        if self._initialized:
            return

        logger.info("Loading %s via mlx-audio", self.hf_repo)
        # mlx-audio's `from_pretrained` returns MLX-native objects; no
        # device/dtype kwargs needed — MLX always targets the current
        # Metal device on Apple Silicon.
        self._processor = LFM2AudioProcessor.from_pretrained(self.hf_repo)
        self._model = LFM2AudioModel.from_pretrained(self.hf_repo)

        self._initialized = True
        logger.info("LFM2-Audio (MLX) model loaded")
        self._cleanup_task = asyncio.create_task(self._cleanup_expired_sessions())

    async def cleanup(self) -> None:
        if self._cleanup_task is not None:
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass
        self._model = None
        self._processor = None
        self._initialized = False
        self._sessions.clear()
        logger.info("LFM2-Audio (MLX) model cleaned up")

    async def _cleanup_expired_sessions(self) -> None:
        while True:
            try:
                await asyncio.sleep(60)
                now = datetime.now()
                expired = [
                    sid for sid, s in self._sessions.items()
                    if (now - s.last_accessed).total_seconds() / 60 > self.session_timeout_minutes
                ]
                for sid in expired:
                    logger.info("Removing expired session: %s", sid)
                    self._sessions.pop(sid, None)
            except asyncio.CancelledError:
                break
            except Exception as exc:  # noqa: BLE001
                logger.error("Session cleanup error: %s", exc)

    def _build_system_turn_text(self) -> str:
        # Persona / style only. Injected knowledge ("context") is NOT
        # stitched into the system prompt anymore — LFM2-Audio tends
        # to under-attend to facts buried in the system message when
        # the prompt is spoken. The context is re-injected as a text
        # span inside each user turn alongside the audio; see
        # `add_audio`/`add_text` in `process()`. This mirrors the
        # model-card pattern:
        #     chat.new_turn("user")
        #     chat.add_audio(audio, sample_rate=sr)
        #     chat.add_text("Transcribe the audio.")
        #     chat.end_turn()
        return self._system_prompt

    async def _get_or_create_session(self, session_id: str) -> ConversationState:
        if session_id in self._sessions:
            self._sessions[session_id].touch()
            return self._sessions[session_id]

        if self._processor is None:
            raise RuntimeError("LFM2AudioMlxNode: initialize() must be called first")

        logger.info("Creating new conversation session: %s", session_id)
        chat = ChatState(self._processor)
        chat.new_turn("system")
        chat.add_text(self._build_system_turn_text())
        chat.end_turn()

        self._sessions[session_id] = ConversationState(
            session_id=session_id, chat_state=chat
        )
        return self._sessions[session_id]

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
        if RUNTIME_DATA_AVAILABLE and RuntimeData is not None and isinstance(data, RuntimeData):
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
                "[%s] unknown aux port %r on LFM2AudioMlxNode; payload ignored",
                self.node_id, port,
            )

    # ────── Main processing ───────────────────────────────────────────

    async def process(self, data: Any) -> AsyncGenerator[Any, None]:
        envelope = self._extract_envelope(data)
        if envelope is not None:
            port, payload = envelope
            logger.info(
                "[%s] aux-port envelope detected: port=%s payload=%r",
                self.node_id, port, payload,
            )
            self._handle_aux_port(port, payload)
            return

        async for item in self._process_audio_turn(data):
            yield item

    async def _process_audio_turn(
        self, data: Any
    ) -> AsyncGenerator[Any, None]:
        if not RUNTIME_DATA_AVAILABLE or RuntimeData is None:
            # Can't yield a RuntimeData.text() here because RuntimeData
            # itself is None — bail out and let the runner log the
            # missing-dep warning. Nothing we yield would be parseable
            # client-side anyway.
            logger.error(
                "[%s] RuntimeData class unavailable; refusing to process",
                self.node_id,
            )
            return

        if not hasattr(data, "is_audio") or not data.is_audio():
            kind = getattr(data, "data_type", lambda: type(data).__name__)()
            yield RuntimeData.text(f"ERROR: expected audio input, got {kind}")
            return

        # Pull samples out of RuntimeData up front — dual-path (PyO3 vs
        # pure-Python) matching the torch node.
        if hasattr(data, "as_audio"):
            samples_bytes, input_sample_rate, _channels, _fmt, _n = data.as_audio()
            audio_np = np.frombuffer(samples_bytes, dtype=np.float32)
        else:
            payload = getattr(data, "payload", None)
            meta = getattr(data, "metadata", None)
            input_sample_rate = int(getattr(meta, "sample_rate", 0) or 0)
            if isinstance(payload, np.ndarray):
                audio_np = payload.astype(np.float32, copy=False).reshape(-1)
            elif isinstance(payload, (bytes, bytearray, memoryview)):
                audio_np = np.frombuffer(bytes(payload), dtype=np.float32)
            else:
                yield RuntimeData.text(
                    f"ERROR: unsupported RuntimeData payload type "
                    f"{type(payload).__name__}"
                )
                return

        if input_sample_rate != self.sample_rate:
            yield RuntimeData.text(
                f"ERROR: input sample rate {input_sample_rate}Hz "
                f"does not match model rate {self.sample_rate}Hz"
            )
            return

        session_id = (
            data.session_id if hasattr(data, "session_id") and data.session_id else "default"
        )
        session_state = await self._get_or_create_session(session_id)
        chat = session_state.chat_state

        # Reset barge-in at the start of every new turn. In this
        # pipeline the VAD publishes barge_in on every speech_start,
        # including the first turn where nothing is in flight; by the
        # time this audio reaches us, the barge-in's intent has
        # already been satisfied.
        self._interrupt = False

        # Feed the user turn. MLX-audio expects mx.array input at the
        # native sample rate. If knowledge has been injected via the
        # `context` aux port, append it as a text span inside the
        # same user turn — the model attends to this span alongside
        # the audio (per the model card's `add_audio` + `add_text`
        # pattern). System-prompt-only placement wasn't reliably
        # used by the model for factual questions.
        chat.new_turn("user")
        chat.add_audio(mx.array(audio_np, dtype=mx.float32), sample_rate=self.sample_rate)
        if self._context:
            chat.add_text(
                f"\n\nRelevant knowledge you must use when answering the "
                f"audio above:\n{self._context}"
            )
        chat.end_turn()

        chat.new_turn("assistant")
        session_state.turn_count += 1
        logger.info(
            "[%s] starting MLX generation for session=%s turn=%d",
            self.node_id, session_id, session_state.turn_count,
        )

        # mlx-audio's ChatState.append takes `(token, modality)` per
        # token — not a batched `(text_stack, audio_stack, flags)` call
        # like the torch liquid-audio API. So we append as we generate
        # and don't accumulate lists for a second append at the end.
        # The lists below are only kept for logging / stats.
        text_token_count = 0
        audio_token_count = 0

        # mlx-audio's generate_interleaved yields `(token, modality)`
        # tuples. `generate_sequential` is text-only; same tuple shape.
        if self.text_only:
            token_generator = self._model.generate_sequential(
                **dict(chat),
                max_new_tokens=self.max_new_tokens,
                temperature=self.audio_temperature,
            )
        else:
            token_generator = self._model.generate_interleaved(
                **dict(chat),
                max_new_tokens=self.max_new_tokens,
            )

        audio_pending: List[Any] = []
        token_idx = 0

        def _decode_audio_chunk(
            tokens: List[Any], *, drop_last: bool = False
        ) -> Optional[Any]:
            """Decode a batch of codebook tokens (shape (8,) each) → audio.

            The mlx-audio model-card pattern:

                codes = mx.stack(tokens[:-1], axis=1)[None, :]  # (1, 8, T)
                waveform = processor.decode_with_detokenizer(codes)
                sf.write("response.wav", waveform[0].tolist(), 24000)

            ``decode_with_detokenizer`` returns a (1, T_audio) mx.array.
            During streaming we feed every batch through; at end-of-turn
            the final ``audio_out[-1]`` is an end-of-audio marker and
            gets dropped via ``drop_last=True``.

            Falls back to ``decode_audio`` (the alternative Mimi-codec
            path) for mlx-audio builds that don't expose the detokeniser
            entrypoint.
            """
            if not tokens:
                return None
            usable = tokens[:-1] if drop_last else tokens
            if not usable:
                return None
            try:
                codes = mx.stack(usable, axis=1)[None, :]  # (1, 8, T)
                decode = getattr(self._processor, "decode_with_detokenizer", None)
                if decode is None:
                    decode = self._processor.decode_audio
                waveform = decode(codes)
                mx.eval(waveform)
                arr = np.array(waveform, copy=False)
                # Normalise to 1-D f32. Depending on the entrypoint the
                # shape is (1, T) (detokenizer) or (1, 1, T) (decode_audio).
                while arr.ndim > 1:
                    arr = arr[0]
                if arr.size == 0:
                    return None
                return RuntimeData.audio(
                    arr.astype(np.float32, copy=False),
                    self.sample_rate,
                    channels=1,
                )
            except Exception as e:  # noqa: BLE001
                logger.warning(
                    "[%s] MLX audio decode failed on %d tokens: %s",
                    self.node_id, len(usable), e,
                )
                return None

        while True:
            if self._interrupt:
                logger.info("[%s] barge-in latched — halting generation", self.node_id)
                self._interrupt = False
                break

            try:
                token, modality = next(token_generator)
            except StopIteration:
                break

            token_idx += 1
            if token_idx % 10 == 0:
                await asyncio.sleep(0)
            # Force eager eval so we don't hold a giant compute graph.
            mx.eval(token)

            # Append this token to the assistant's in-progress turn so
            # multi-turn history is preserved. mlx-audio's ChatState
            # expects a per-token call.
            try:
                chat.append(token, modality)
            except Exception as e:  # noqa: BLE001
                logger.debug(
                    "[%s] chat.append failed on token %d: %s",
                    self.node_id, token_idx, e,
                )

            if modality == LFMModality.TEXT:
                # Flush pending audio first so text doesn't land
                # between two audio chunks of the same utterance.
                if audio_pending:
                    rd = _decode_audio_chunk(audio_pending)
                    audio_pending = []
                    if rd is not None:
                        yield rd

                text_token_count += 1
                try:
                    decoded = self._processor.decode_text(token[None])
                except Exception as e:  # noqa: BLE001
                    logger.warning(
                        "[%s] text decode failed on token %d: %s",
                        self.node_id, token_idx, e,
                    )
                    decoded = ""
                if decoded:
                    yield RuntimeData.text(decoded)
                    await asyncio.sleep(0)
            else:
                # Audio codebook token (shape (8,))
                audio_token_count += 1
                audio_pending.append(token)
                if len(audio_pending) >= self.audio_decode_interval:
                    rd = _decode_audio_chunk(audio_pending)
                    audio_pending = []
                    if rd is not None:
                        yield rd

        # Drain any remaining audio tokens. mlx-audio's end-of-audio
        # marker is the last codebook token — drop it before decoding
        # (matches the model-card snippet: `audio_out[:-1]`).
        if audio_pending:
            rd = _decode_audio_chunk(audio_pending, drop_last=True)
            audio_pending = []
            if rd is not None:
                yield rd

        yield RuntimeData.text("<|text_end|>")
        yield RuntimeData.text("<|audio_end|>")

        # Per-token `chat.append` already ran inside the loop, so the
        # assistant turn is fully populated. Close it out for multi-turn
        # context. Losing this call would cause the next turn to fail
        # mid-generation because ChatState still thinks we're inside an
        # unfinished assistant turn.
        try:
            chat.end_turn()
        except Exception as e:  # noqa: BLE001
            logger.warning(
                "[%s] chat.end_turn() failed: %s "
                "(next turn may not see this turn's history)",
                self.node_id, e,
            )

        logger.info(
            "[%s] turn %d complete: %d text tokens, %d audio tokens",
            self.node_id,
            session_state.turn_count,
            text_token_count,
            audio_token_count,
        )

    # ────── Introspection ─────────────────────────────────────────────

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "LFM2AudioMlxNode",
            "backend": "mlx",
            "hf_repo": self.hf_repo,
            "system_prompt": self._system_prompt,
            "audio_temperature": self.audio_temperature,
            "audio_top_k": self.audio_top_k,
            "max_new_tokens": self.max_new_tokens,
            "sample_rate": self.sample_rate,
            "session_timeout_minutes": self.session_timeout_minutes,
            "text_only": self.text_only,
            "context_len": len(self._context),
            "active_sessions": len(self._sessions),
        }

    def get_session_info(self, session_id: str) -> Optional[Dict[str, Any]]:
        if session_id not in self._sessions:
            return None
        s = self._sessions[session_id]
        return {
            "session_id": s.session_id,
            "turn_count": s.turn_count,
            "created_at": s.created_at.isoformat(),
            "last_accessed": s.last_accessed.isoformat(),
        }

    def list_sessions(self) -> List[Dict[str, Any]]:
        return [self.get_session_info(sid) for sid in self._sessions.keys()]
