"""
LFM2-Audio speech-to-speech node — multiprocess-capable, control-bus-aware.

Speech-to-speech conversational AI built on Liquid AI's LFM2-Audio-1.5B.
Accepts audio on the main input, generates interleaved text and audio on
the output. The same aux-port control surface as :mod:`lfm2_text` is
wired up here so a gRPC/WebRTC client can steer a live voice agent
between turns without tearing the session down.

## Control-bus surface

Publishes to these aux ports arrive here as
``RuntimeData.Json({"__aux_port__": <port>, "payload": {...}})``:

    audio.in.context        → store RAG / retrieval text used on the
                              next turn; invalidates cached chat state so
                              the new system turn includes the context.
    audio.in.system_prompt  → replace the persona / behaviour prompt;
                              cached chat states are dropped.
    audio.in.reset          → drop conversation history for all sessions.
    audio.in.barge_in       → request immediate cancellation of the
                              currently-generating turn (if any). The
                              generator loop checks the flag on every
                              token batch and bails out cleanly.
    audio.in                → main channel: one audio utterance, treated
                              as the user turn.

The main audio channel is still validated as ``RuntimeData.Audio`` — the
aux-port envelope always arrives as text-wrapped JSON, so it's peeled
off BEFORE audio validation runs.
"""

from __future__ import annotations

import asyncio
import json
import logging
import os
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, AsyncGenerator, Dict, List, Optional, TYPE_CHECKING, Union

# Heavy ML deps are optional so the module can be imported for node
# registration even when liquid_audio / torch aren't installed. Capture
# the actual failure reason — the previous blanket `except ImportError`
# silently masked "liquid_audio is installed but import ChatState
# failed" cases, producing misleading "please pip install liquid-audio"
# errors even when the venv had the package.
_ML_IMPORT_ERROR: Optional[BaseException] = None
try:
    import numpy as np
    import torch
    import torchaudio  # noqa: F401
    from liquid_audio import ChatState, LFMModality
    from liquid_audio import LFM2AudioModel, LFM2AudioProcessor
    _ML_DEPS_AVAILABLE = True
except BaseException as _exc:
    _ML_DEPS_AVAILABLE = False
    _ML_IMPORT_ERROR = _exc
    np = None  # type: ignore
    torch = None  # type: ignore
    ChatState = None  # type: ignore
    LFMModality = None  # type: ignore
    LFM2AudioModel = None  # type: ignore
    LFM2AudioProcessor = None  # type: ignore
    logging.getLogger(__name__).warning(
        "LFM2AudioNode ML imports failed (%s): %s",
        type(_exc).__name__, _exc,
    )

if _ML_DEPS_AVAILABLE:
    try:  # torch dynamo is opportunistic — don't let it break inference
        import torch._dynamo
        torch._dynamo.config.suppress_errors = True
    except (ImportError, AttributeError):
        pass

if TYPE_CHECKING:
    from remotemedia.core.multiprocessing.data import RuntimeData

try:
    from remotemedia.core.multiprocessing.data import (
        RuntimeData,
        numpy_to_audio,
        audio_to_numpy,  # noqa: F401  (exported for downstream users)
    )
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    RuntimeData = None  # type: ignore
    numpy_to_audio = None  # type: ignore
    audio_to_numpy = None  # type: ignore
    logging.warning(
        "[LFM2AudioNode] RuntimeData bindings not available. "
        "Using fallback implementation."
    )

from remotemedia.core.multiprocessing import (
    MultiprocessNode,
    NodeConfig,
    python_requires,
    register_node,
)

logger = logging.getLogger(__name__)


DEFAULT_SYSTEM_PROMPT = "Respond with interleaved text and audio."
AUX_PORT_KEY = "__aux_port__"


@dataclass
class ConversationState:
    """One live ``liquid_audio.ChatState`` plus session bookkeeping."""

    session_id: str
    chat_state: Any  # liquid_audio.ChatState
    created_at: datetime = field(default_factory=datetime.now)
    last_accessed: datetime = field(default_factory=datetime.now)
    turn_count: int = 0

    def touch(self) -> None:
        self.last_accessed = datetime.now()


@register_node("LFM2AudioNode")
@python_requires(
    [
        # LFM2-Audio pulls ChatState/LFM2AudioModel/LFM2AudioProcessor from
        # the liquid_audio SDK, which itself has torch/torchaudio/transformers
        # as transitive deps. We pin transformers to the LFM2-compatible
        # release line used by the text sibling.
        "liquid-audio>=0.1",
        # See control_bus_test_server.rs for the full reasoning. In short:
        # liquid_audio 1.1.0 imports a transformers-4.54-era private
        # symbol (`Lfm2HybridConvCache`) that 5.x removed.
        "transformers>=4.54.0,<5.0",
        "torch>=2.1",
        "torchaudio>=2.1",
        "accelerate>=0.33",
    ]
)
class LFM2AudioNode(MultiprocessNode):
    """
    Multi-turn speech-to-speech node with control-bus aux ports.

    Main channel consumes ``RuntimeData.Audio`` (24 kHz mono float32)
    and yields interleaved ``RuntimeData.Text`` and ``RuntimeData.Audio``.
    Aux ports (context / system_prompt / reset / barge_in) are handled
    out-of-band — aux messages produce no outputs of their own.
    """

    # ────── Construction (dual-mode: in-process kwargs OR NodeConfig) ──

    def __init__(
        self,
        config: Union[NodeConfig, Dict[str, Any], None] = None,
        *,
        node_id: Optional[str] = None,
        name: Optional[str] = None,
        hf_repo: str = "LiquidAI/LFM2-Audio-1.5B",
        system_prompt: str = DEFAULT_SYSTEM_PROMPT,
        device: Optional[str] = None,
        audio_temperature: float = 1.0,
        audio_top_k: int = 4,
        max_new_tokens: int = 4096,
        sample_rate: int = 24000,
        session_timeout_minutes: int = 30,
        text_only: bool = False,
        **kwargs: Any,
    ) -> None:
        # Multiprocess runner's 3-attempt construction: str → TypeError → config=...
        # Reject the bare string form so we land on the config path where
        # manifest params actually reach us.
        if isinstance(config, str):
            raise TypeError(
                "LFM2AudioNode requires NodeConfig or keyword-only params; "
                "bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "lfm2_audio",
                node_type="LFM2AudioNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "lfm2_audio"),
                node_type=config.get("node_type", "LFM2AudioNode"),
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

        req_device = params.get("device", device)
        if req_device is None:
            self.device = (
                "cuda" if _ML_DEPS_AVAILABLE and torch.cuda.is_available() else "cpu"
            )
        else:
            self.device = req_device

        self._processor: Any = None
        self._model: Any = None
        self._initialized = False

        # Per-session chat state. Keyed by logical session id (which is
        # the IPC session id in the multiprocess runner).
        self._sessions: Dict[str, ConversationState] = {}
        self._cleanup_task: Optional[asyncio.Task] = None

        # Aux-port state.
        self._context: str = ""          # retrieval/RAG block
        self._interrupt: bool = False    # barge-in latch (consumed by token loop)

        # Friendly name used by Pipeline.get_node(name) for in-process use.
        self.name = name or config.node_id

        self.is_streaming = True
        logger.info(
            "LFM2AudioNode initialized: device=%s text_only=%s",
            self.device, self.text_only,
        )

    # ────── In-process control-plane-analog API ───────────────────────
    #
    # These setters are used by in-process tests (`pl.get_node("lfm").set_context(...)`)
    # and by `_handle_aux_port` when the same node runs in a multiprocess
    # worker driven by the Session Control Bus. Keeping a single surface
    # ensures both paths behave identically.

    def set_context(self, docs: str) -> None:
        """Replace the retrieval/RAG context applied to the system turn."""
        self._context = docs or ""
        self._invalidate_sessions("context")

    def clear_context(self) -> None:
        self._context = ""
        self._invalidate_sessions("context-clear")

    def set_system_prompt(self, prompt: str) -> None:
        """Replace the persona/system prompt. Drops cached chat states."""
        self._system_prompt = prompt or DEFAULT_SYSTEM_PROMPT
        self._invalidate_sessions("system-prompt")

    def reset_history(self) -> None:
        """Drop conversation history. Next turn starts from the system prompt."""
        self._invalidate_sessions("reset")

    def request_barge_in(self) -> None:
        """Signal the active generation loop to stop ASAP. Cleared next turn."""
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
            # Surface the actual import failure instead of the
            # misleading "please pip install" error. Common cases:
            # - `from liquid_audio import ChatState` fails because the
            #   API moved between versions.
            # - torchaudio is imported but the native ffmpeg backend
            #   isn't available on the host (OSError from dlopen).
            cause = _ML_IMPORT_ERROR
            detail = (
                f"{type(cause).__name__}: {cause}" if cause is not None
                else "unknown import failure"
            )
            raise RuntimeError(
                f"LFM2AudioNode ML stack failed to import — {detail}. "
                "Required packages: liquid_audio, torch, torchaudio."
            ) from cause
        if self._initialized:
            return

        logger.info(
            "Initializing LFM2-Audio from %r on %s", self.hf_repo, self.device
        )
        if self.device == "cpu" and not torch.cuda.is_available():
            os.environ["CUDA_VISIBLE_DEVICES"] = ""

        try:
            self._processor = LFM2AudioProcessor.from_pretrained(
                self.hf_repo, device=self.device
            )
        except TypeError:
            self._processor = LFM2AudioProcessor.from_pretrained(self.hf_repo)
            if self.device == "cpu":
                self._processor = self._processor.to("cpu")
        self._processor = self._processor.eval()

        # liquid_audio 1.1.0 changed `LFM2AudioModel.from_pretrained`:
        #   - dropped the `attn_implementation=` kwarg.
        #   - defaults to device="cuda", dtype=torch.bfloat16 regardless
        #     of host capability; we must pass our own device down so
        #     the inner model isn't constructed on CUDA on a CPU-only
        #     box (which aborts with "Torch not compiled with CUDA
        #     enabled" the moment any tensor is allocated).
        #
        # Pass `device=` / `dtype=` explicitly when the API accepts them
        # and fall back to post-construct `.to(device)` otherwise — old
        # 0.x liquid_audio didn't accept device/dtype on from_pretrained.
        model_kwargs: Dict[str, Any] = {}
        if self.device == "cuda":
            model_kwargs["device"] = "cuda"
            model_kwargs["dtype"] = torch.bfloat16
        else:
            model_kwargs["device"] = "cpu"
            model_kwargs["dtype"] = torch.float32

        try:
            self._model = LFM2AudioModel.from_pretrained(self.hf_repo, **model_kwargs)
        except TypeError:
            # Older liquid_audio that doesn't accept device / dtype —
            # load on CPU and move afterwards.
            self._model = LFM2AudioModel.from_pretrained(self.hf_repo)
            self._model = (
                self._model.cuda() if self.device == "cuda" else self._model.to("cpu")
            )
        self._model = self._model.eval()

        self._initialized = True
        logger.info("LFM2-Audio model loaded")
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
        logger.info("LFM2-Audio model cleaned up")

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
            except Exception as exc:  # noqa: BLE001 — keep the cleaner alive
                logger.error("Session cleanup error: %s", exc)

    def _build_system_turn_text(self) -> str:
        if not self._context:
            return self._system_prompt
        return (
            f"{self._system_prompt}\n\n"
            f"Known facts you must use when relevant:\n{self._context}"
        )

    async def _get_or_create_session(self, session_id: str) -> ConversationState:
        if session_id in self._sessions:
            self._sessions[session_id].touch()
            return self._sessions[session_id]

        if self._processor is None:
            raise RuntimeError("LFM2AudioNode: initialize() must be called first")

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
        """
        Return ``(port_name, payload_dict)`` if ``data`` is an aux-port
        envelope, else ``None``.

        Accepts dict / JSON string / RuntimeData.Text-of-JSON.
        """
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
                "[%s] unknown aux port %r on LFM2AudioNode; payload ignored",
                self.node_id, port,
            )

    # ────── Main processing ───────────────────────────────────────────

    async def process(
        self, data: Any
    ) -> AsyncGenerator[Any, None]:
        """
        Route between aux-port control messages and audio user turns.

        Aux-port messages produce no outputs (they only update state).
        Audio user turns stream interleaved text + audio back.
        """
        # ── Unwrap the aux-port envelope (control-bus publishes) ──
        envelope = self._extract_envelope(data)
        if envelope is not None:
            port, payload = envelope
            logger.info(
                "[%s] aux-port envelope detected: port=%s payload=%r",
                self.node_id, port, payload,
            )
            self._handle_aux_port(port, payload)
            return  # no yielded output for control-plane frames

        # ── Otherwise, treat the input as an audio user turn ──
        async for item in self._process_audio_turn(data):
            yield item

    async def _process_audio_turn(
        self, data: Any
    ) -> AsyncGenerator[Any, None]:
        if not RUNTIME_DATA_AVAILABLE or RuntimeData is None:
            yield RuntimeData.text("ERROR: RuntimeData bindings unavailable")  # type: ignore[attr-defined]
            return

        if not hasattr(data, "is_audio") or not data.is_audio():
            kind = getattr(data, "data_type", lambda: type(data).__name__)()
            logger.error("Expected audio input, got %s", kind)
            yield RuntimeData.text(f"ERROR: expected audio input, got {kind}")
            return

        # Pull audio out of RuntimeData up front — the PyO3 handle is
        # not safe to touch after an async suspension point. `as_audio()`
        # is PyO3-only; fall back to the pure-Python `payload` / metadata
        # shape when the Rust bindings aren't loaded (see the
        # `RuntimeData bindings not available` warning on import).
        if hasattr(data, "as_audio"):
            samples_bytes, input_sample_rate, _channels, _fmt, _n = data.as_audio()
            audio_array = np.frombuffer(samples_bytes, dtype=np.float32)
        else:
            payload = getattr(data, "payload", None)
            meta = getattr(data, "metadata", None)
            input_sample_rate = int(getattr(meta, "sample_rate", 0) or 0)
            if isinstance(payload, np.ndarray):
                audio_array = payload.astype(np.float32, copy=False).reshape(-1)
            elif isinstance(payload, (bytes, bytearray, memoryview)):
                audio_array = np.frombuffer(bytes(payload), dtype=np.float32)
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

        # Reset barge-in at the start of every new turn — it's a latch,
        # not a continuous flag.
        self._interrupt = False

        # Add the user audio turn.
        chat.new_turn("user")
        wav = torch.from_numpy(audio_array).float()
        if wav.dim() == 1:
            wav = wav.unsqueeze(0)
        chat.add_audio(wav, self.sample_rate)
        chat.end_turn()

        chat.new_turn("assistant")
        session_state.turn_count += 1
        logger.info(
            "[%s] starting generation for session=%s turn=%d",
            self.node_id, session_id, session_state.turn_count,
        )

        text_tokens_for_history: List[Any] = []
        audio_tokens_for_history: List[Any] = []
        modality_flags_for_history: List[Any] = []

        if self.text_only:
            token_generator = self._model.generate_sequential(
                **chat,
                max_new_tokens=self.max_new_tokens,
                text_temperature=None,
                text_top_k=None,
            )
        else:
            token_generator = self._model.generate_interleaved(
                **chat,
                max_new_tokens=self.max_new_tokens,
                audio_temperature=self.audio_temperature,
                audio_top_k=self.audio_top_k,
            )

        audio_batch: List[Any] = []
        audio_batch_size = 5
        token_idx = 0

        def _decode_and_emit(batch: List[Any]) -> Optional[Any]:
            if not batch:
                return None
            try:
                cloned = [t.clone().detach() for t in batch]
                stacked = torch.stack(cloned, dim=0).T.unsqueeze(0)
                with torch.no_grad():
                    waveform = self._processor.mimi.decode(stacked)[0]
                arr = waveform.cpu().numpy()
                if arr.ndim == 2:
                    arr = arr[0]
                return numpy_to_audio(arr, self.sample_rate, channels=1)
            except RuntimeError as e:
                if "CUDA error" in str(e) or "CUBLAS" in str(e):
                    logger.warning("CUDA error decoding audio batch, skipping: %s", str(e)[:100])
                    return None
                raise

        while True:
            if self._interrupt:
                logger.info("[%s] barge-in latched — halting generation", self.node_id)
                self._interrupt = False
                break

            try:
                token = next(token_generator)
            except StopIteration:
                break

            token_idx += 1
            if token_idx % 10 == 0:
                await asyncio.sleep(0)

            if token.numel() == 1:
                # Text token. Flush any pending audio batch first so
                # audio stays contiguous in the output stream.
                if audio_batch:
                    audio_rd = _decode_and_emit(audio_batch)
                    audio_batch.clear()
                    if audio_rd is not None:
                        yield audio_rd

                text_tokens_for_history.append(token)
                modality_flags_for_history.append(LFMModality.TEXT)

                decoded = self._processor.text.decode(token)
                if decoded:
                    yield RuntimeData.text(decoded)
                    await asyncio.sleep(0)
            else:
                audio_tokens_for_history.append(token)
                modality_flags_for_history.append(LFMModality.AUDIO_OUT)
                audio_batch.append(token)

                if len(audio_batch) >= audio_batch_size:
                    audio_rd = _decode_and_emit(audio_batch)
                    audio_batch.clear()
                    if audio_rd is not None:
                        yield audio_rd

        # Drain remaining audio tokens (drop the final end-of-audio marker).
        if len(audio_batch) > 1:
            audio_rd = _decode_and_emit(audio_batch[:-1])
            audio_batch.clear()
            if audio_rd is not None:
                yield audio_rd

        # Terminal markers so the client can cut over between turns.
        yield RuntimeData.text("<|text_end|>")
        yield RuntimeData.text("<|audio_end|>")

        # Append this turn's tokens to chat history so the next turn has context.
        try:
            if text_tokens_for_history or audio_tokens_for_history:
                text_stack = (
                    torch.stack(text_tokens_for_history, 1)
                    if text_tokens_for_history else None
                )
                audio_stack = (
                    torch.stack(audio_tokens_for_history, 1)
                    if audio_tokens_for_history else None
                )
                modality_tensor = None
                if modality_flags_for_history:
                    modality_values = [int(f.value) for f in modality_flags_for_history]
                    modality_tensor = torch.tensor(modality_values, dtype=torch.long).unsqueeze(0)

                if self.text_only:
                    codebooks = (
                        getattr(self._processor, "codebooks", None)
                        or getattr(getattr(self._processor, "mimi", None), "codebooks", None)
                        or (int(audio_stack.size(0)) if isinstance(audio_stack, torch.Tensor) else 8)
                    )
                    chat.append(
                        text=text_stack,
                        audio_out=torch.empty((codebooks, 0), dtype=torch.long),
                        modality_flag=modality_tensor,
                    )
                else:
                    chat.append(
                        text=text_stack,
                        audio_out=audio_stack,
                        modality_flag=modality_tensor,
                    )
            chat.end_turn()
        except Exception as e:  # noqa: BLE001
            logger.error("Failed to append to chat history: %s", e)
            try:
                chat.end_turn()
            except Exception:  # noqa: BLE001
                pass

    # ────── Introspection ─────────────────────────────────────────────

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "LFM2AudioNode",
            "hf_repo": self.hf_repo,
            "system_prompt": self._system_prompt,
            "device": self.device,
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
