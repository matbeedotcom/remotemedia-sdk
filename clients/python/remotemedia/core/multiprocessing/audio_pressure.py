"""
Pressure-aware coalescing buffer for audio-producing nodes.

Any multiprocess node that turns a text (or other token) stream into a stream
of audio frames can mix in :class:`AudioPressureMixin` to automatically:

- buffer incoming fragments when there is still un-played audio runway
  downstream (so the next synthesis call sees a larger, more natural span),
- drain the buffer immediately when the runway is about to starve (so first-
  audio latency stays bounded under slow upstream producers),
- always drain on an explicit end-of-reply signal.

The mixin is designed for the current multiprocess node contract, in which
:py:meth:`MultiprocessNode._process_single_message` awaits one ``process()``
generator at a time: we can't suspend inside a call waiting for siblings,
but we can defer work until the *next* incoming fragment. Because the
MultiprocessNode input queue buffers messages while ``process()`` is running,
the next call naturally arrives "carrying" everything that accumulated
during the previous synthesis — which is exactly the coalescing opportunity
we want.

Typical usage (see ``qwen_tts_mlx.py`` for the reference wiring)::

    class MyTTS(MultiprocessNode, AudioPressureMixin):
        def __init__(self, ...):
            super().__init__(...)
            self.init_audio_pressure(
                low_pressure_margin_ms=500.0,
            )

        async def process(self, data):
            text = self._extract_text(data)
            is_end = "<|text_end|>" in text

            batch = self.pressure_decide(text, force_drain=is_end)
            if batch is None:
                # Pressure is low — defer. Nothing to yield for this frame.
                return

            async for frame in self._synthesize_streaming(batch):
                self.record_audio_frame(frame)
                yield frame

            if is_end:
                yield RuntimeData.text("<|audio_end|>")
"""

from __future__ import annotations

import asyncio
import logging
from typing import Any, List, Optional

logger = logging.getLogger(__name__)


class AudioPressureMixin:
    """
    Coalesce text-to-audio workloads based on downstream audio runway.

    Subclasses must:
      1. Call :py:meth:`init_audio_pressure` from ``__init__``.
      2. Call :py:meth:`pressure_decide` in ``process()`` with each incoming
         fragment; if it returns ``None`` the fragment has been buffered and
         no audio should be emitted this call.
      3. Call :py:meth:`record_audio_frame` for every audio frame yielded
         back upstream, so the runway estimate stays in sync with reality.

    The runway estimate is purely local — we know (a) when our last synth
    call finished and (b) how many seconds of audio it produced. Provided
    the downstream sink delivers frames in ~real time (true for our WebRTC
    track), ``max(0, plays_until - now)`` is a good proxy for how much
    audio the listener still has buffered.
    """

    # ────── lifecycle ────────────────────────────────────────────────

    def init_audio_pressure(
        self,
        *,
        low_pressure_margin_ms: float = 500.0,
        coalesce_window_ms: float = 800.0,
    ) -> None:
        """
        Initialise mixin state. Must be called from the subclass ``__init__``.

        Parameters
        ----------
        low_pressure_margin_ms :
            If more than this much audio remains queued, incoming fragments
            are deferred so the next synth call gets a larger batch.
        coalesce_window_ms :
            Soft upper bound on how long to hold a deferred batch before
            flushing regardless of runway. Prevents the node from sitting
            on text indefinitely if upstream stalls without sending an
            end-marker.
        """
        self._ap_pending_text: List[str] = []
        self._ap_audio_plays_until: float = 0.0
        self._ap_last_buffer_started: float = 0.0
        self.low_pressure_margin_ms: float = float(low_pressure_margin_ms)
        self.coalesce_window_ms: float = float(coalesce_window_ms)

    def reset_audio_pressure(self) -> None:
        """Drop any buffered text and reset runway. Call on barge-in / reset."""
        self._ap_pending_text = []
        self._ap_audio_plays_until = 0.0
        self._ap_last_buffer_started = 0.0

    # ────── runway bookkeeping ───────────────────────────────────────

    def _ap_now(self) -> float:
        loop = asyncio.get_event_loop()
        return loop.time()

    def audio_runway_s(self) -> float:
        """Seconds of audio estimated to still be queued downstream."""
        return max(0.0, self._ap_audio_plays_until - self._ap_now())

    def record_audio_frame(self, frame: Any) -> None:
        """
        Extend the runway by the duration of one emitted audio frame.

        Uses :py:meth:`_audio_frame_duration_s` so subclasses with exotic
        frame types can override just that hook.
        """
        duration_s = self._audio_frame_duration_s(frame)
        if duration_s <= 0.0:
            return
        now = self._ap_now()
        # If audio was still queued past `now`, extend from that tail;
        # otherwise we just finished playing — start fresh at `now`.
        self._ap_audio_plays_until = max(self._ap_audio_plays_until, now) + duration_s

    def _audio_frame_duration_s(self, frame: Any) -> float:
        """
        Compute the duration of one audio frame in seconds.

        Default implementation handles :class:`RuntimeData.Audio` frames.
        Subclasses can override for custom frame types.
        """
        # Prefer duck-typing so we don't hard-depend on RuntimeData here.
        try:
            is_audio = getattr(frame, "is_audio", None)
            if callable(is_audio) and is_audio():
                audio = frame.as_audio()
                samples = getattr(audio, "samples", None)
                sample_rate = getattr(audio, "sample_rate", None)
                channels = getattr(audio, "channels", 1) or 1
                if samples is None or not sample_rate:
                    return 0.0
                n = len(samples)
                return n / float(sample_rate) / float(channels)
        except Exception:  # noqa: BLE001
            return 0.0
        return 0.0

    # ────── decision API ─────────────────────────────────────────────

    def pressure_decide(
        self,
        incoming_text: Optional[str],
        *,
        force_drain: bool = False,
    ) -> Optional[str]:
        """
        Feed an incoming text fragment; return the batch to synthesise now,
        or ``None`` if the fragment has been buffered for a later call.

        Decision rules (in order):
          * ``force_drain=True`` (end-of-reply, barge-in recovery, etc.) →
            return the whole buffer.
          * Runway below ``low_pressure_margin_ms`` → high pressure, return
            the whole buffer.
          * Buffer has been sitting for more than ``coalesce_window_ms`` →
            flush to avoid indefinite hold.
          * Otherwise → buffer and return ``None``.

        Empty/whitespace-only input is still appended (it's a legitimate
        boundary fragment from the collector) but will not by itself
        force a drain.
        """
        now = self._ap_now()
        if incoming_text:
            if not self._ap_pending_text:
                self._ap_last_buffer_started = now
            self._ap_pending_text.append(incoming_text)

        if not self._ap_pending_text:
            return None

        if force_drain:
            return self._drain_buffer()

        runway_s = self.audio_runway_s()
        margin_s = self.low_pressure_margin_ms / 1000.0
        if runway_s <= margin_s:
            return self._drain_buffer()

        # Low pressure — but don't hold forever.
        window_s = self.coalesce_window_ms / 1000.0
        held_for = now - self._ap_last_buffer_started
        if held_for >= window_s:
            return self._drain_buffer()

        return None

    def _drain_buffer(self) -> Optional[str]:
        if not self._ap_pending_text:
            return None
        batch = "".join(self._ap_pending_text)
        self._ap_pending_text = []
        self._ap_last_buffer_started = 0.0
        return batch

    def has_buffered_text(self) -> bool:
        return bool(self._ap_pending_text)


__all__ = ["AudioPressureMixin"]
