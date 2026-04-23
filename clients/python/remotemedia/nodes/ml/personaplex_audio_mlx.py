"""
PersonaPlex-7B speech-to-speech node — MLX backend for Apple Silicon.

Wraps the ``personaplex_mlx`` Python package (Kyutai-Moshi fork at
``mu-hashmi/personaplex-mlx``) so pipelines can swap
``PersonaPlexAudioMlxNode`` in alongside :class:`LFM2AudioMlxNode`.
The control-bus surface and output shape match the LFM2 MLX node; only
the underlying model and a few params change.

## Requirements

    pip install "personaplex-mlx @ git+https://github.com/mu-hashmi/personaplex-mlx.git"

No torch build required for the model (mlx only), but note that
``personaplex-mlx`` pulls torch as a transitive dep for weight
conversion and pins Python 3.10–3.12. First run downloads ~9 GB
(8-bit) from ``nvidia/personaplex-7b-v1`` — accept the HuggingFace
license and export ``HF_TOKEN`` before initialization.

## Streaming model — why there's no end-of-turn marker

PersonaPlex is a full-duplex Moshi-family model: every 80 ms frame of
user audio (1920 samples @ 24 kHz) produces exactly one 80 ms frame of
assistant audio. There is no ``end of turn`` signal — the model speaks
(or stays silent) continuously as long as we keep feeding it input.
That's why this node, unlike :class:`LFM2AudioMlxNode`, does **not**
emit ``<|text_end|>``/``<|audio_end|>``.

## Control-bus surface (identical to LFM2 MLX node)

Aux ports accepted:

    audio.in.context        → append knowledge to the persona prompt
    audio.in.system_prompt  → replace persona (restarts streaming state)
    audio.in.reset          → drop cached generator state
    audio.in.barge_in       → reset streaming on the next audio chunk
    audio.in                → continuous audio frames (any chunk size,
                              24 kHz mono float32)

PersonaPlex only exposes one text hook (``LmGen.text_prompt_tokens``),
so knowledge injected via the ``context`` aux port is concatenated onto
the persona prompt rather than repeated per-turn the way the LFM2 chat
API allows. Changing ``context`` or ``system_prompt`` therefore drops
the cached generator — the next frame re-primes the model with the new
combined prompt.

Main output stream: interleaved ``RuntimeData.Text`` (one piece per
text token) and ``RuntimeData.Audio`` (24 kHz mono float32, 80 ms
frames).
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
    import rustymimi  # type: ignore
    import sentencepiece  # type: ignore
    from personaplex_mlx import models as _pp_models
    from personaplex_mlx import utils as _pp_utils
    from personaplex_mlx.persona_utils import (
        get_lm_config,
        get_or_download_mimi,
        get_or_download_model_file,
        get_or_download_tokenizer,
        get_voice_prompt_dir,
        load_lm_weights,
        resolve_voice_prompt,
        seed_all,
        wrap_with_system_tags,
    )
    _ML_DEPS_AVAILABLE = True
except BaseException as _exc:
    _ML_DEPS_AVAILABLE = False
    _ML_IMPORT_ERROR = _exc
    mx = None  # type: ignore
    np = None  # type: ignore
    rustymimi = None  # type: ignore
    sentencepiece = None  # type: ignore
    _pp_models = None  # type: ignore
    _pp_utils = None  # type: ignore
    get_lm_config = None  # type: ignore
    get_or_download_mimi = None  # type: ignore
    get_or_download_model_file = None  # type: ignore
    get_or_download_tokenizer = None  # type: ignore
    get_voice_prompt_dir = None  # type: ignore
    load_lm_weights = None  # type: ignore
    resolve_voice_prompt = None  # type: ignore
    seed_all = None  # type: ignore
    wrap_with_system_tags = None  # type: ignore
    logging.getLogger(__name__).warning(
        "PersonaPlexAudioMlxNode MLX imports failed (%s): %s",
        type(_exc).__name__, _exc,
    )

if TYPE_CHECKING:
    from remotemedia.core.multiprocessing.data import RuntimeData

try:
    # Only ``RuntimeData`` is required. The PyO3-only numpy helpers
    # (``numpy_to_audio`` / ``audio_to_numpy``) are intentionally not
    # imported here — same reasoning as ``lfm2_audio_mlx.py``.
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


DEFAULT_HF_REPO = "nvidia/personaplex-7b-v1"
DEFAULT_SYSTEM_PROMPT = (
    "You are a wise and friendly teacher. Answer questions or provide "
    "advice in a clear and engaging way."
)
DEFAULT_VOICE = "NATF2"
DEFAULT_SEED = 42424242
FRAME_SIZE = 1920  # 80 ms @ 24 kHz — fixed by the Mimi codec.
AUX_PORT_KEY = "__aux_port__"

# Reserved text-token IDs in the Moshi sentencepiece vocab:
# 0=EPAD, 1=BOS, 2=EOS, 3=PAD. Suppress them when decoding pieces.
_RESERVED_TEXT_TOKENS = frozenset({0, 1, 2, 3})


def _reshape_input_tokens(encoded: "np.ndarray", user_codebooks: int) -> "mx.array":
    """Port of ``personaplex_mlx.run_inference._reshape_input_tokens``.

    Mimi returns encoded tokens in (batch, codebooks, time) layout; the
    LM expects the user-codebook slice either as (batch, user_cb, 1) or
    (batch, 1, user_cb) depending on build. Accept both.
    """
    tokens = mx.array(encoded).transpose(0, 2, 1)[:, :, :user_codebooks]
    if tokens.shape[1] == user_codebooks and tokens.shape[2] == 1:
        return tokens
    if tokens.shape[1] == 1 and tokens.shape[2] == user_codebooks:
        return tokens.transpose(0, 2, 1)
    raise ValueError(f"unexpected encoded shape {tokens.shape}")


@dataclass
class ConversationState:
    """Per-session PersonaPlex generator, Mimi codec, and input buffer.

    Mimi's ``Tokenizer`` carries streaming encoder/decoder state, and
    ``LmGen`` holds KV cache + RNG + silence counters, so both must be
    one-per-session. The model weights themselves are shared (see
    ``PersonaPlexAudioMlxNode._model``).
    """

    session_id: str
    gen: Any  # personaplex_mlx.models.LmGen
    audio_tokenizer: Any  # rustymimi.Tokenizer
    pending: Any  # np.ndarray[f32] — leftover samples < FRAME_SIZE
    created_at: datetime = field(default_factory=datetime.now)
    last_accessed: datetime = field(default_factory=datetime.now)
    turn_count: int = 0

    def touch(self) -> None:
        self.last_accessed = datetime.now()


@register_node("PersonaPlexAudioMlxNode")
@python_requires(
    [
        # mu-hashmi's MLX port of PersonaPlex. No PyPI release — pull
        # straight from GitHub via PEP 508 direct reference. pip gets
        # each string as its own argument, so this installs cleanly
        # whether declared here or overridden via a manifest's
        # `python_deps`. Transitive deps (mlx, rustymimi, sphn,
        # sentencepiece, numpy 2.x) come from the repo's pyproject.toml;
        # don't duplicate the numpy pin here — the upstream constraint
        # is narrower than remotemedia-client's and double-pinning
        # turns an already-tight resolution into an impossible one.
        "personaplex-mlx @ git+https://github.com/mu-hashmi/personaplex-mlx.git",
    ]
)
class PersonaPlexAudioMlxNode(MultiprocessNode):
    """
    Streaming speech-to-speech node using the MLX build of PersonaPlex-7B.

    PersonaPlex is a Kyutai-Moshi-family full-duplex model: every 80 ms
    input frame produces an 80 ms output frame. Unlike the LFM2 nodes,
    there's no ``end of turn`` — so this node emits continuous audio
    and text without ``<|text_end|>`` / ``<|audio_end|>`` sentinels.

    The control-bus aux ports (``context``, ``system_prompt``,
    ``reset``, ``barge_in``) are identical to :class:`LFM2AudioMlxNode`,
    so swapping the ``node_type`` in a manifest is the only change
    required to switch backends.
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
        voice: str = DEFAULT_VOICE,
        voice_prompt: Optional[str] = None,
        voice_prompt_dir: Optional[str] = None,
        quantized: Optional[int] = 8,
        text_temperature: float = 0.7,
        audio_temperature: float = 0.8,
        text_top_k: int = 25,
        audio_top_k: int = 250,
        max_steps: int = 4000,
        sample_rate: int = 24000,
        seed: int = DEFAULT_SEED,
        session_timeout_minutes: int = 30,
        warmup_session_id: Optional[str] = "default",
        **kwargs: Any,
    ) -> None:
        if isinstance(config, str):
            raise TypeError(
                "PersonaPlexAudioMlxNode requires NodeConfig or keyword-only "
                "params; bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "personaplex_audio_mlx",
                node_type="PersonaPlexAudioMlxNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "personaplex_audio_mlx"),
                node_type=config.get("node_type", "PersonaPlexAudioMlxNode"),
                params=config.get("params", {}),
            )

        super().__init__(config, **kwargs)

        params = config.params or {}
        self.hf_repo = params.get("hf_repo", hf_repo)
        self._system_prompt = params.get("system_prompt", system_prompt)
        self.voice = params.get("voice", voice)
        self.voice_prompt = params.get("voice_prompt", voice_prompt)
        self.voice_prompt_dir = params.get("voice_prompt_dir", voice_prompt_dir)
        self.quantized: Optional[int] = params.get("quantized", quantized)
        if self.quantized is not None and self.quantized not in (4, 8):
            raise ValueError(
                f"PersonaPlexAudioMlxNode: `quantized` must be 4, 8, or None — "
                f"got {self.quantized!r}"
            )
        self.text_temperature = float(params.get("text_temperature", text_temperature))
        self.audio_temperature = float(params.get("audio_temperature", audio_temperature))
        self.text_top_k = int(params.get("text_top_k", text_top_k))
        self.audio_top_k = int(params.get("audio_top_k", audio_top_k))
        self.max_steps = int(params.get("max_steps", max_steps))
        self.sample_rate = int(params.get("sample_rate", sample_rate))
        self.seed = int(params.get("seed", seed))
        self.session_timeout_minutes = int(
            params.get("session_timeout_minutes", session_timeout_minutes)
        )
        # Session to pre-create inside ``initialize()`` so ``step_system_prompts``
        # (~20-30 s for PersonaPlex-7B at 8-bit on first run) doesn't land on
        # the first ``process()`` call and trip the per-node execution timeout
        # in the Rust streaming scheduler (default 30 s,
        # ``DEFAULT_TIMEOUT_MS`` in ``crates/core/src/executor/streaming_scheduler.rs``).
        # Set to ``None`` to skip warmup (tests that want to measure cold-start
        # latency, or builds where the caller passes a custom session_id on
        # first frame).
        self.warmup_session_id: Optional[str] = params.get(
            "warmup_session_id", warmup_session_id
        )

        # Resolved once during initialize(); shared across sessions.
        self._lm_config: Any = None
        self._model: Any = None
        self._text_tokenizer: Any = None
        self._mimi_file: Optional[str] = None
        self._voice_prompt_path: Optional[str] = None
        self._initialized = False

        self._sessions: Dict[str, ConversationState] = {}
        self._cleanup_task: Optional[asyncio.Task] = None

        # Aux-port state — shape matches the LFM2 MLX node.
        self._context: str = ""
        self._interrupt: bool = False

        self.name = name or config.node_id
        self.is_streaming = True
        logger.info(
            "PersonaPlexAudioMlxNode initialized (repo=%s, voice=%s, q=%s)",
            self.hf_repo, self.voice, self.quantized,
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
                "[%s] dropping %d cached generator(s) (%s)",
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
                f"PersonaPlexAudioMlxNode MLX stack failed to import — {detail}. "
                "Install `personaplex-mlx` on an Apple Silicon Mac and accept "
                "the HuggingFace model license (`HF_TOKEN` env var required)."
            ) from cause
        if self._initialized:
            return

        logger.info(
            "Loading PersonaPlex weights from %s (quantized=%s)",
            self.hf_repo, self.quantized,
        )
        seed_all(self.seed)
        self._lm_config = get_lm_config(None, self.hf_repo)
        model_file, _ = get_or_download_model_file(
            hf_repo=self.hf_repo,
            quantized=self.quantized,
            explicit_model_file=None,
        )
        tokenizer_file = get_or_download_tokenizer(self.hf_repo, None)
        self._mimi_file = get_or_download_mimi(self.hf_repo, None)

        self._text_tokenizer = sentencepiece.SentencePieceProcessor(tokenizer_file)
        self._model = _pp_models.Lm(self._lm_config)
        self._model.set_dtype(mx.bfloat16)
        load_lm_weights(self._model, self._lm_config, model_file, self.quantized)

        vp_dir = get_voice_prompt_dir(self.voice_prompt_dir, self.hf_repo)
        self._voice_prompt_path = resolve_voice_prompt(
            voice=self.voice,
            voice_prompt=self.voice_prompt,
            voice_prompt_dir=vp_dir,
        )

        self._initialized = True
        logger.info(
            "PersonaPlex model loaded (voice_prompt=%s)",
            self._voice_prompt_path,
        )
        self._cleanup_task = asyncio.create_task(self._cleanup_expired_sessions())

        # Pre-warm one session so the *two* expensive first-call paths
        # land inside the 5-minute init budget instead of on the first
        # audio chunk under the 30 s per-node execution timeout:
        #   1. `step_system_prompts` — MLX kernel JIT + KV-cache prime
        #      for the system-prompt shape.
        #   2. `encode_step` + `gen.step` on an audio frame — the
        #      per-frame MLX kernels used during streaming. System
        #      prompts alone don't warm these; empirically the first
        #      audio step still took ~30-40 s on a cold MLX graph.
        # After both run once, steady-state step time is ~80 ms/frame.
        if self.warmup_session_id:
            try:
                import time as _time
                _t0 = _time.time()
                logger.info(
                    "[%s] pre-warming session %r (step_system_prompts)",
                    self.node_id, self.warmup_session_id,
                )
                warm_state = await self._get_or_create_session(
                    self.warmup_session_id
                )
                t_sysprompt = _time.time()
                logger.info(
                    "[%s] system-prompt warm in %.2fs; warming audio step",
                    self.node_id, t_sysprompt - _t0,
                )

                # One dummy audio frame through the streaming path.
                # Silence works — we only need to trigger kernel
                # compilation, not produce anything listenable. The
                # warmup output is discarded.
                dummy = np.zeros(FRAME_SIZE, dtype=np.float32)
                encoded = warm_state.audio_tokenizer.encode_step(
                    dummy[None, None, :]
                )
                if encoded is not None:
                    model_input = _reshape_input_tokens(
                        encoded, warm_state.gen.user_codebooks
                    )
                    _ = warm_state.gen.step(input_tokens=model_input)
                    # Force the MLX graph to materialize now rather
                    # than deferring eval until the first real step.
                    tok = warm_state.gen.last_audio_tokens()
                    if tok is not None:
                        mx.eval(tok)

                logger.info(
                    "[%s] audio-step warm in %.2fs; total warmup %.2fs",
                    self.node_id,
                    _time.time() - t_sysprompt,
                    _time.time() - _t0,
                )
            except Exception as exc:  # noqa: BLE001
                # Don't hard-fail init on warmup — the first real frame
                # will retry session creation and surface the same
                # error at a place callers can see.
                logger.warning(
                    "[%s] pre-warm failed (%s); first frame will retry: %s",
                    self.node_id, type(exc).__name__, exc,
                )

    async def cleanup(self) -> None:
        if self._cleanup_task is not None:
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass
        self._model = None
        self._text_tokenizer = None
        self._initialized = False
        self._sessions.clear()
        logger.info("PersonaPlex model cleaned up")

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

    def _build_combined_prompt(self) -> Optional[str]:
        """Stitch persona + knowledge into a single system prompt.

        PersonaPlex only exposes one text hook (``LmGen.text_prompt_tokens``),
        so knowledge injected via the ``context`` aux port is concatenated
        onto the persona prompt rather than repeated per-turn (the latter
        is how LFM2 handles it, but the Moshi streaming loop has no
        mid-conversation ``add_text``).
        """
        pieces: List[str] = []
        if self._system_prompt:
            pieces.append(self._system_prompt.strip())
        if self._context:
            pieces.append(
                "Relevant knowledge to use when responding:\n"
                f"{self._context.strip()}"
            )
        combined = "\n\n".join(p for p in pieces if p)
        return combined or None

    async def _get_or_create_session(self, session_id: str) -> ConversationState:
        if session_id in self._sessions:
            self._sessions[session_id].touch()
            return self._sessions[session_id]

        if self._model is None or self._text_tokenizer is None:
            raise RuntimeError(
                "PersonaPlexAudioMlxNode: initialize() must be called first"
            )

        logger.info("Creating new PersonaPlex session: %s", session_id)
        audio_tokenizer = rustymimi.Tokenizer(self._mimi_file, num_codebooks=8)
        gen = _pp_models.LmGen(
            model=self._model,
            max_steps=self.max_steps,
            text_sampler=_pp_utils.Sampler(temp=self.text_temperature, top_k=self.text_top_k),
            audio_sampler=_pp_utils.Sampler(temp=self.audio_temperature, top_k=self.audio_top_k),
            check=False,
            # 0.5 s of model silence before the gen flags it (matches
            # the default in personaplex_mlx.local / .offline).
            audio_silence_frame_cnt=int(0.5 * 12.5),
        )
        gen.load_voice_prompt_embeddings(self._voice_prompt_path)

        combined = self._build_combined_prompt()
        if combined:
            gen.text_prompt_tokens = self._text_tokenizer.encode(
                wrap_with_system_tags(combined)
            )
        else:
            gen.text_prompt_tokens = None
        gen.reset_streaming()
        gen.step_system_prompts()

        state = ConversationState(
            session_id=session_id,
            gen=gen,
            audio_tokenizer=audio_tokenizer,
            pending=np.zeros(0, dtype=np.float32),
        )
        self._sessions[session_id] = state
        return state

    # ────── Aux-port envelope handling (copy of LFM2 MLX node) ────────

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
                "[%s] unknown aux port %r on PersonaPlexAudioMlxNode; payload ignored",
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

        async for item in self._process_audio_frames(data):
            yield item

    async def _process_audio_frames(
        self, data: Any
    ) -> AsyncGenerator[Any, None]:
        if not RUNTIME_DATA_AVAILABLE or RuntimeData is None:
            logger.error(
                "[%s] RuntimeData class unavailable; refusing to process",
                self.node_id,
            )
            return

        if not hasattr(data, "is_audio") or not data.is_audio():
            kind = getattr(data, "data_type", lambda: type(data).__name__)()
            yield RuntimeData.text(f"ERROR: expected audio input, got {kind}")
            return

        # Extract PCM — dual-path matching LFM2AudioMlxNode.
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

        if self._interrupt:
            # A barge-in was latched between chunks. Reset this session's
            # streaming state and re-prime the system prompt so the next
            # frame starts cleanly. Matches ``LFM2AudioMlxNode`` semantics
            # where barge_in drops the in-flight assistant turn.
            logger.info(
                "[%s] barge-in latched — resetting streaming state for %s",
                self.node_id, session_id,
            )
            session_state.gen.reset_streaming()
            session_state.gen.step_system_prompts()
            session_state.pending = np.zeros(0, dtype=np.float32)
            self._interrupt = False

        # Concatenate new samples onto the remainder from the previous chunk.
        samples = np.concatenate(
            [session_state.pending, audio_np.astype(np.float32, copy=False)]
        )
        n_frames = samples.shape[0] // FRAME_SIZE
        if n_frames == 0:
            session_state.pending = samples
            return

        session_state.turn_count += 1
        text_pieces_emitted = 0
        audio_frames_emitted = 0

        for frame_idx in range(n_frames):
            pcm = samples[frame_idx * FRAME_SIZE : (frame_idx + 1) * FRAME_SIZE]
            # Mimi expects (batch, channels, samples) = (1, 1, 1920).
            encoded = session_state.audio_tokenizer.encode_step(pcm[None, None, :])
            if encoded is None:
                continue
            model_input = _reshape_input_tokens(
                encoded, session_state.gen.user_codebooks
            )

            text_token = session_state.gen.step(input_tokens=model_input)
            if text_token is not None:
                token_id = int(text_token[0].item())
                if token_id not in _RESERVED_TEXT_TOKENS:
                    try:
                        piece = self._text_tokenizer.id_to_piece(token_id).replace(
                            "▁", " "
                        )
                    except Exception as e:  # noqa: BLE001
                        logger.debug(
                            "[%s] text decode failed on token %d: %s",
                            self.node_id, token_id, e,
                        )
                        piece = ""
                    if piece:
                        yield RuntimeData.text(piece)
                        text_pieces_emitted += 1

            audio_tokens = session_state.gen.last_audio_tokens()
            if audio_tokens is not None:
                decode_tokens = np.array(audio_tokens[:, :, None]).astype(np.uint32)
                try:
                    out_pcm = session_state.audio_tokenizer.decode_step(decode_tokens)
                except Exception as e:  # noqa: BLE001
                    logger.warning(
                        "[%s] mimi decode failed on frame %d: %s",
                        self.node_id, frame_idx, e,
                    )
                    continue
                out_flat = np.asarray(out_pcm, dtype=np.float32).reshape(-1)
                if out_flat.size > 0:
                    yield RuntimeData.audio(out_flat, self.sample_rate, channels=1)
                    audio_frames_emitted += 1

            # Yield to the event loop periodically so the runner can
            # interleave input draining. Frames are ~80 ms apart, so
            # every 4 frames is ~320 ms — responsive without churn.
            if frame_idx % 4 == 0:
                await asyncio.sleep(0)

        # Stash the unprocessed tail for the next chunk.
        session_state.pending = samples[n_frames * FRAME_SIZE :].copy()

        logger.debug(
            "[%s] chunk processed: session=%s frames=%d text=%d audio=%d pending=%d",
            self.node_id, session_id, n_frames,
            text_pieces_emitted, audio_frames_emitted,
            session_state.pending.shape[0],
        )

    # ────── Introspection ─────────────────────────────────────────────

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "PersonaPlexAudioMlxNode",
            "backend": "mlx",
            "hf_repo": self.hf_repo,
            "system_prompt": self._system_prompt,
            "voice": self.voice,
            "voice_prompt": self.voice_prompt,
            "quantized": self.quantized,
            "text_temperature": self.text_temperature,
            "audio_temperature": self.audio_temperature,
            "text_top_k": self.text_top_k,
            "audio_top_k": self.audio_top_k,
            "max_steps": self.max_steps,
            "sample_rate": self.sample_rate,
            "seed": self.seed,
            "session_timeout_minutes": self.session_timeout_minutes,
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
