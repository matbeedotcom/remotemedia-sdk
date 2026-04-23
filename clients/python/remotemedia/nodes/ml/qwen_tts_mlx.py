"""
Qwen3-TTS speech-synthesis node — MLX backend for Apple Silicon.

Consumes streamed ``RuntimeData.Text`` chunks from upstream
(typically ``QwenTextMlxNode`` → ``TextCollectorNode`` so what we
receive per-frame is a complete sentence) and **emits audio chunks as
fast as the model produces them**, using ``mlx_audio``'s
``generate(stream=True)`` generator. Each waveform chunk yielded from
``process()`` is forwarded by the multiprocess runner to the session
router on the same tick, which writes it straight into the WebRTC
audio ring buffer — so the listener hears the start of a sentence
while the end of it is still being synthesised. No per-reply
buffering; no wait-for-``<|text_end|>``.

    incoming: RuntimeData.text("<sentence>."), ..., RuntimeData.text("<|text_end|>")
    outgoing: RuntimeData.text("<sentence>."),           ← text passthrough
              RuntimeData.audio(<chunk>), ...,           ← streamed waveform
              (next sentence interleaves its own text+audio)
              ...
              RuntimeData.text("<|audio_end|>")          ← end-of-reply marker

The passthrough behaviour is deliberate: the web client subscribes to
``audio.out`` for live transcript AND for playback-duration tracking
(used for half-duplex mic-gating). Keeping tokens on the same stream
as the synthesised audio means the same contract as
:class:`LFM2AudioMlxNode` — so the existing WebRTC UI works unchanged
against either backend.

## Requirements

    pip install mlx-audio numpy

See https://github.com/Blaizzy/mlx-audio/blob/main/mlx_audio/tts/models/qwen3_tts/README.md
for voice names and streaming params.

## Control-bus aux ports accepted

    <node_id>.in.barge_in  → halt current synthesis
    <node_id>.in.voice     → change voice for subsequent turns
"""

from __future__ import annotations

import asyncio
import json
import logging
import re
from concurrent.futures import ThreadPoolExecutor
from typing import Any, AsyncGenerator, Dict, List, Optional, Union

_ML_IMPORT_ERROR: Optional[BaseException] = None
try:
    import numpy as np
    from mlx_audio.tts.utils import load_model as _mlx_load_tts_model
    _ML_DEPS_AVAILABLE = True
except BaseException as _exc:  # noqa: BLE001
    _ML_DEPS_AVAILABLE = False
    _ML_IMPORT_ERROR = _exc
    np = None  # type: ignore
    _mlx_load_tts_model = None  # type: ignore
    logging.getLogger(__name__).warning(
        "QwenTTSMlxNode mlx-audio imports failed (%s): %s",
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


DEFAULT_HF_REPO = "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-6bit"
DEFAULT_VOICE = "serena"
AUX_PORT_KEY = "__aux_port__"

# Markers produced by upstream LLM nodes. `<|text_end|>` triggers a
# flush-and-synthesize; anything matching `<|...|>` is stripped from
# the text before it reaches the TTS model.
_SENTINEL_RE = re.compile(r"<\|[^|]*\|>")


@register_node("QwenTTSMlxNode")
@python_requires(
    [
        "mlx-audio>=0.1",
        "numpy>=1.24",
    ]
)
class QwenTTSMlxNode(MultiprocessNode):
    """Streaming TTS wrapper around ``mlx_audio.tts``."""

    def __init__(
        self,
        config: Union[NodeConfig, Dict[str, Any], None] = None,
        *,
        node_id: Optional[str] = None,
        name: Optional[str] = None,
        hf_repo: str = DEFAULT_HF_REPO,
        voice: str = DEFAULT_VOICE,
        sample_rate: int = 24000,
        output_sample_rate: int = 48000,
        streaming_interval: float = 0.32,
        speed: float = 1.0,
        passthrough_text: bool = True,
        **kwargs: Any,
    ) -> None:
        if isinstance(config, str):
            raise TypeError(
                "QwenTTSMlxNode requires NodeConfig or keyword-only params; "
                "bare positional node_id not supported"
            )
        if config is None:
            config = NodeConfig(
                node_id=node_id or name or "qwen_tts_mlx",
                node_type="QwenTTSMlxNode",
                params={},
            )
        elif isinstance(config, dict):
            config = NodeConfig(
                node_id=config.get("node_id", node_id or "qwen_tts_mlx"),
                node_type=config.get("node_type", "QwenTTSMlxNode"),
                params=config.get("params", {}),
            )

        super().__init__(config, **kwargs)

        params = config.params or {}
        self.hf_repo = params.get("hf_repo", hf_repo)
        self.voice = params.get("voice", voice)
        self.sample_rate = int(params.get("sample_rate", sample_rate))
        # Upsample to the WebRTC Opus track's negotiated rate inside this
        # node so the pipeline can be `audio → client` with zero
        # intermediate-node batching. Without this we'd need a downstream
        # FastResampleNode and the session router would collect the
        # TTS's per-chunk yields into a Vec before the resampler could
        # run, destroying real-time streaming.
        self.output_sample_rate = int(
            params.get("output_sample_rate", output_sample_rate)
        )
        self.streaming_interval = float(
            params.get("streaming_interval", streaming_interval)
        )
        self.speed = float(params.get("speed", speed))
        self.passthrough_text = bool(params.get("passthrough_text", passthrough_text))

        self._model: Any = None
        self._initialized = False

        # Dedicated single-thread executor for every MLX call on this
        # node. Metal command buffers submitted by `model.generate()`
        # are bound to the OS thread that submitted them; if a
        # subsequent call lands on a different worker thread (which
        # asyncio's default executor does freely) Metal raises
        # `kIOGPUCommandBufferCallbackErrorInvalidResource` and the
        # whole process aborts. Pinning to one worker keeps the
        # GPU context consistent across warmup + every synthesis.
        self._mlx_executor: Optional[ThreadPoolExecutor] = None

        # No per-reply text buffering — we now stream per-sentence
        # straight through `_synthesize_streaming`. Kept only the
        # barge-in latch.
        self._interrupt: bool = False

        self.name = name or config.node_id
        self.is_streaming = True
        logger.info(
            "QwenTTSMlxNode configured (repo=%s, voice=%s, sr=%d)",
            self.hf_repo, self.voice, self.sample_rate,
        )

    # ────── Control-plane helpers ─────────────────────────────────────

    def request_barge_in(self) -> None:
        self._interrupt = True
        logger.info("[%s] barge-in requested", self.node_id)

    def set_voice(self, voice: str) -> None:
        if voice:
            self.voice = voice

    # ────── MultiprocessNode contract ─────────────────────────────────

    async def initialize(self) -> None:
        if not _ML_DEPS_AVAILABLE:
            cause = _ML_IMPORT_ERROR
            detail = (
                f"{type(cause).__name__}: {cause}" if cause is not None
                else "unknown import failure"
            )
            raise RuntimeError(
                f"QwenTTSMlxNode mlx-audio stack failed to import — {detail}. "
                "Install `mlx-audio` on an Apple Silicon Mac."
            ) from cause
        if self._initialized:
            return

        # Create the dedicated MLX worker before touching mlx-audio so
        # model load and all subsequent generate() calls run on the
        # same OS thread.
        self._mlx_executor = ThreadPoolExecutor(
            max_workers=1, thread_name_prefix=f"mlx-tts-{self.node_id}"
        )
        loop = asyncio.get_event_loop()

        logger.info("[%s] loading %s via mlx-audio", self.node_id, self.hf_repo)
        self._model = await loop.run_in_executor(
            self._mlx_executor, _mlx_load_tts_model, self.hf_repo
        )

        # Pre-warm the MLX graph. The first real `generate()` call on
        # Qwen3-TTS takes 10–13 s on an M-series Mac because MLX
        # JIT-compiles the full decoder pipeline and materialises the
        # voice embeddings on the first inference. Absorbing that
        # warmup here means the first real sentence of the first
        # real reply starts synthesising at the model's steady-state
        # rate (~3 s per short sentence) instead of eating a full
        # warmup cycle.
        def _warmup() -> None:
            try:
                gen = self._model.generate(
                    text="Hello.",
                    voice=self.voice,
                    speed=self.speed,
                    stream=True,
                    streaming_interval=self.streaming_interval,
                )
                for _ in gen:
                    pass
            except Exception as exc:  # noqa: BLE001
                logger.warning("[%s] TTS warmup failed: %s", self.node_id, exc)

        logger.info("[%s] warming up Qwen TTS (one dummy synthesis)", self.node_id)
        t0 = loop.time()
        # Run warmup on the SAME dedicated MLX worker as every later
        # generate() call.
        await loop.run_in_executor(self._mlx_executor, _warmup)
        logger.info(
            "[%s] Qwen TTS model ready (warmup=%.1fs)",
            self.node_id, loop.time() - t0,
        )

        self._initialized = True

    async def cleanup(self) -> None:
        self._model = None
        self._initialized = False
        if self._mlx_executor is not None:
            self._mlx_executor.shutdown(wait=False, cancel_futures=True)
            self._mlx_executor = None

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
        if port == "barge_in":
            self.request_barge_in()
        elif port == "voice":
            text = payload.get("text")
            if isinstance(text, str):
                self.set_voice(text)
        elif port in ("context", "system_prompt", "reset"):
            # The web UI dual-publishes LLM-side aux ports to both
            # `audio.in.*` (LFM2's single node) and `llm.in.*` (the
            # Qwen S2S split pipeline). When this TTS node is in a
            # split pipeline it receives the `audio.in.*` copy and
            # should silently drop it — the `llm` node picks up its
            # own copy. Debug-level only; no warning.
            logger.debug(
                "[%s] ignoring LLM-only aux port %r on TTS node", self.node_id, port,
            )
        else:
            logger.warning(
                "[%s] unknown aux port %r on QwenTTSMlxNode; payload ignored",
                self.node_id, port,
            )

    # ────── Input coercion ────────────────────────────────────────────

    @staticmethod
    def _extract_text(data: Any) -> Optional[str]:
        if isinstance(data, str):
            return data
        if _HAS_RUNTIME_DATA and RuntimeData is not None and isinstance(data, RuntimeData):
            try:
                if data.is_text():
                    return data.as_text()
            except Exception:  # noqa: BLE001
                return None
        return None

    # ────── Streaming synthesis ───────────────────────────────────────

    async def _synthesize_streaming(
        self, text: str
    ) -> "AsyncGenerator[Any, None]":
        """
        Fire one `model.generate(stream=True)` call for *this* text
        fragment (typically a sentence produced by ``TextCollectorNode``
        upstream) and yield each decoded waveform chunk to the caller
        the instant the Python generator hands it back.

        Each yield propagates out of the node's ``process()`` generator,
        through the Rust multiprocess streaming path, and into the
        WebRTC audio sink — so the listener hears the first words of
        a reply while the later words are still being generated.
        """
        text = _SENTINEL_RE.sub("", text).strip()
        if not text:
            return

        # Clear any stale barge-in latch on each new sentence. If the
        # user speaks over this sentence, `_handle_aux_port` will set
        # the flag again and we'll drop the remaining chunks on the
        # next iteration.
        self._interrupt = False

        logger.info(
            "[%s] synthesising %d chars (voice=%s)",
            self.node_id, len(text), self.voice,
        )

        loop = asyncio.get_running_loop()
        t_start = loop.time()
        queue: asyncio.Queue = asyncio.Queue()
        _SENTINEL = object()
        _ERROR = object()

        def _produce() -> None:
            try:
                gen = self._model.generate(
                    text=text,
                    voice=self.voice,
                    speed=self.speed,
                    stream=True,
                    streaming_interval=self.streaming_interval,
                )
                chunk_idx = 0
                for result in gen:
                    if self._interrupt:
                        break
                    audio = getattr(result, "audio", None)
                    if audio is None:
                        continue
                    chunk_idx += 1
                    # Log the instant each chunk returns from mlx-audio.
                    # Delta from t_start tells us whether the model
                    # actually streams (monotonic spread across
                    # synthesis) or bulk-yields (all deltas within a
                    # few ms of each other, at the end).
                    dt = loop.time() - t_start
                    logger.info(
                        "[%s] mlx-audio chunk %d at +%.3fs",
                        self.node_id, chunk_idx, dt,
                    )
                    loop.call_soon_threadsafe(queue.put_nowait, audio)
            except Exception as exc:  # noqa: BLE001
                loop.call_soon_threadsafe(queue.put_nowait, (_ERROR, exc))
            finally:
                loop.call_soon_threadsafe(queue.put_nowait, _SENTINEL)

        # IMPORTANT: run the producer on the node's dedicated MLX
        # worker thread (the same one that loaded the model and ran
        # warmup). Using `asyncio.to_thread` would hand the work to
        # whatever ThreadPoolExecutor worker is free, and Metal
        # command buffers from a stale thread context abort the
        # process with `kIOGPUCommandBufferCallbackErrorInvalidResource`.
        # `.submit()` starts the work immediately; `wrap_future`
        # makes the concurrent.futures.Future awaitable without
        # re-wrapping in `create_task` (which expects a coroutine).
        if self._mlx_executor is None:
            logger.error("[%s] MLX executor is None; refusing to synthesise", self.node_id)
            return
        producer = asyncio.wrap_future(self._mlx_executor.submit(_produce))
        barged = False
        yielded = 0
        try:
            while True:
                if self._interrupt:
                    logger.info(
                        "[%s] barge-in latched — halting synthesis", self.node_id
                    )
                    self._interrupt = False
                    barged = True
                    break
                item = await queue.get()
                if item is _SENTINEL:
                    break
                if isinstance(item, tuple) and len(item) == 2 and item[0] is _ERROR:
                    exc = item[1]
                    logger.error("[%s] synthesis error: %s", self.node_id, exc)
                    break
                arr = self._to_float32_mono(item)
                if arr is None or arr.size == 0:
                    continue
                # Yield the chunk immediately — do NOT await before
                # this. The caller (process_streaming in the Rust
                # runner) forwards each yielded RuntimeData.Audio to
                # the session router on the same tick, which writes
                # it into the WebRTC audio ring buffer.
                yielded += 1
                # Upsample to the Opus track's negotiated rate BEFORE
                # yielding. This lets the downstream pipeline be a
                # single hop (audio → client) — with a separate
                # resample node in the graph, the session router
                # batches this node's yields until process() returns,
                # which kills real-time playback.
                arr_out = self._upsample_to_output_rate(arr)
                dt_out = loop.time() - t_start
                logger.info(
                    "[%s] yielding chunk %d at +%.3fs (%d→%d samples)",
                    self.node_id, yielded, dt_out, arr.size, arr_out.size,
                )
                yield RuntimeData.audio(
                    arr_out, self.output_sample_rate, channels=1
                )
                await asyncio.sleep(0)
        finally:
            # Producer cleanup. `self._model.generate()` is a blocking
            # C-extension call inside `asyncio.to_thread` — we can't
            # cancel it cleanly, but on barge-in we must not await it
            # (would stall the node for the full synthesis duration and
            # trip the 30 s router timeout). Cancel the asyncio wrapper
            # and let the OS thread drain into the no-op queue at its
            # own pace; the next sentence spins up a fresh producer.
            if not producer.done():
                if barged:
                    producer.cancel()
                else:
                    try:
                        await producer
                    except Exception:  # noqa: BLE001
                        pass

    def _upsample_to_output_rate(self, samples: "np.ndarray") -> "np.ndarray":
        """Cheap linear 24 kHz → 48 kHz upsampler (or any integer ratio).

        Speech is heavily band-limited below 8 kHz, so linear interpolation
        is perceptually indistinguishable from a polyphase filter here and
        avoids pulling in scipy as a runtime dep. If `sample_rate ==
        output_sample_rate` (someone set the defaults equal) we pass
        through.
        """
        if self.sample_rate == self.output_sample_rate or samples.size == 0:
            return samples
        ratio = self.output_sample_rate / float(self.sample_rate)
        new_len = int(round(samples.size * ratio))
        if new_len <= 0:
            return samples
        x_old = np.arange(samples.size, dtype=np.float64)
        x_new = np.linspace(0.0, samples.size - 1.0, new_len, dtype=np.float64)
        return np.interp(x_new, x_old, samples).astype(np.float32, copy=False)

    @staticmethod
    def _to_float32_mono(audio: Any) -> Optional["np.ndarray"]:
        """Coerce assorted mlx-audio return types into 1-D float32."""
        if audio is None:
            return None
        arr = audio
        # mx.array → numpy
        if hasattr(arr, "tolist") and not isinstance(arr, (bytes, bytearray, list)):
            try:
                arr = np.asarray(arr)
            except Exception:  # noqa: BLE001
                return None
        if isinstance(arr, list):
            arr = np.asarray(arr, dtype=np.float32)
        if not isinstance(arr, np.ndarray):
            try:
                arr = np.asarray(arr)
            except Exception:  # noqa: BLE001
                return None
        while arr.ndim > 1:
            arr = arr[0]
        return arr.astype(np.float32, copy=False)

    # ────── Main processing ───────────────────────────────────────────

    async def process(self, data: Any) -> AsyncGenerator[Any, None]:
        if not _HAS_RUNTIME_DATA or RuntimeData is None:
            logger.error("[%s] RuntimeData unavailable; refusing to process", self.node_id)
            return

        # Aux-port envelopes (barge_in, voice switch) arrive as text
        # frames; consume them and yield nothing.
        envelope = self._extract_envelope(data)
        if envelope is not None:
            port, payload = envelope
            logger.info(
                "[%s] aux-port envelope detected: port=%s payload=%r",
                self.node_id, port, payload,
            )
            self._handle_aux_port(port, payload)
            return

        text = self._extract_text(data)
        if text is None:
            return

        # Channel-aware routing. `channel == "ui"` means the producer
        # wants this chunk shown visually but NOT spoken. We forward it
        # on our output so a downstream display sink can see it, but we
        # skip synthesis and all pressure bookkeeping for it.
        channel = "tts"
        if _HAS_RUNTIME_DATA and RuntimeData is not None and isinstance(data, RuntimeData):
            md = getattr(data, "metadata", None)
            channel = getattr(md, "channel", "tts") if md is not None else "tts"
        if channel == "ui":
            logger.debug(
                "[%s] ui-channel text (%d chars) — passthrough, no synth",
                self.node_id, len(text),
            )
            yield RuntimeData.text(text, channel="ui")
            return

        # End-of-reply marker. Detected before passthrough so we can
        # strip it from the transcript and emit `<|audio_end|>` exactly
        # once at the tail of the reply.
        is_end = "<|text_end|>" in text

        if self.passthrough_text:
            # Forward the original frame verbatim so the web UI's live
            # transcript stays continuous. Frontends already strip
            # `<|...|>` markers before rendering.
            yield RuntimeData.text(text)

        # Synthesise every fragment immediately and predictably. Because
        # the MultiprocessNode dispatch loop processes one message at a
        # time, sentences naturally serialise: while this synth runs, the
        # next sentence queues on the input side and starts as soon as we
        # return. Pressure-aware coalescing sat on top of this serial
        # ordering to try to group sentences, but added perceptible jitter
        # (defer-or-flush timing depended on a rolling runway estimate) —
        # the reliable behaviour is "one sentence in, one synth out".
        clean = _SENTINEL_RE.sub("", text).strip()
        if clean:
            async for audio_frame in self._synthesize_streaming(clean):
                yield audio_frame

        if is_end:
            # End-of-reply sentinel: tells the client playback is done
            # on our side (server) and half-duplex mic-gating can
            # re-arm after local playback drains.
            yield RuntimeData.text("<|audio_end|>")

    # ────── Introspection ─────────────────────────────────────────────

    def get_config(self) -> dict:
        return {
            "node_id": self.node_id,
            "node_type": "QwenTTSMlxNode",
            "backend": "mlx-audio",
            "hf_repo": self.hf_repo,
            "voice": self.voice,
            "sample_rate": self.sample_rate,
            "streaming_interval": self.streaming_interval,
            "speed": self.speed,
            "passthrough_text": self.passthrough_text,
        }


__all__ = ["QwenTTSMlxNode"]
