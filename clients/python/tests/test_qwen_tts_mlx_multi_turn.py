"""
Multi-turn / barge-in stress test for ``QwenTTSMlxNode``.

The TTS node is the last hop before the listener's ear, so leaks here
mean the user literally hears stale audio. This file exercises the
same guarantees as ``test_qwen_text_mlx_multi_turn.py`` but at the
synthesis layer:

1. **Ordering / passthrough.** Each text frame in yields its text
   verbatim (for the web UI's live transcript) followed by streamed
   audio chunks in the order the model produced them. ``<|text_end|>``
   triggers a single ``<|audio_end|>`` sentinel at the end.
2. **UI-channel text is not spoken.** Text on ``channel="ui"`` (from
   the LLM's ``show(...)`` tool) is forwarded for display but never
   fed to the TTS model.
3. **Pre-synthesis barge-in drops the sentence.** This is the
   pipeline-wiring race fixed in ``qwen_tts_mlx.py`` — a barge-in
   latched by the control bus BEFORE ``_synthesize_streaming`` starts
   iterating must cancel the sentence, not silently clear and proceed.
4. **Mid-synthesis barge-in halts promptly.** Audio chunks produced
   after the interrupt must not reach the output.
5. **Aux-envelope barge-in works via the normal input path.** Same
   contract as the LLM node — ``{"__aux_port__": "barge_in"}`` yields
   nothing and latches the interrupt flag.
6. **Consecutive sentences don't cross-talk.** Barge-in on sentence N
   must not strand state that leaks audio into sentence N+1.

All ``mlx-audio`` hooks are monkeypatched so the tests run on any
platform — no Apple Silicon / model download required.

Run:

    pytest clients/python/tests/test_qwen_tts_mlx_multi_turn.py -v
"""

from __future__ import annotations

import asyncio
import json
import time
from collections import deque
from typing import Any, Iterable, List

import numpy as np
import pytest

import remotemedia.nodes.ml.qwen_tts_mlx as qtts
from remotemedia.core.multiprocessing.data import RuntimeData


# ──────────────────────────── fake mlx-audio ──────────────────────────

class _FakeAudioResult:
    """Stand-in for mlx-audio's per-chunk generator output.

    The node accesses ``result.audio`` only, so that's all we need.
    """

    __slots__ = ("audio",)

    def __init__(self, audio: np.ndarray) -> None:
        self.audio = audio


class _FakeTTSModel:
    """Scripted replacement for the mlx-audio model.

    The node calls ``model.generate(text, voice, speed, stream,
    streaming_interval)`` and iterates the result for ``.audio`` chunks.
    Each call consumes the next scripted list off ``scripts``. If no
    script is queued, a one-chunk "silence" response is returned so
    warmup during ``initialize()`` doesn't blow up.

    ``chunk_delay`` releases the GIL between chunks so the asyncio
    consumer task can observe ``self._interrupt`` and barge-in
    mid-stream.
    """

    def __init__(self, chunk_delay: float = 0.003) -> None:
        self.scripts: deque = deque()
        self.calls: List[dict] = []
        self.chunk_delay = chunk_delay

    def queue(self, chunks: Iterable[np.ndarray]) -> None:
        self.scripts.append(list(chunks))

    def generate(
        self,
        text: str,
        voice: str,
        speed: float,
        stream: bool,
        streaming_interval: float,
    ):
        self.calls.append({
            "text": text,
            "voice": voice,
            "speed": speed,
            "stream": stream,
            "streaming_interval": streaming_interval,
        })
        script = self.scripts.popleft() if self.scripts else [
            np.zeros(8, dtype=np.float32)
        ]
        delay = self.chunk_delay

        for chunk in script:
            if delay:
                time.sleep(delay)
            yield _FakeAudioResult(chunk)


# ───────────────────────── fixtures / helpers ─────────────────────────

@pytest.fixture
def fake_tts_model(monkeypatch):
    """Install a fake mlx-audio load hook that returns a scripted model."""
    model = _FakeTTSModel(chunk_delay=0.003)
    monkeypatch.setattr(qtts, "_ML_DEPS_AVAILABLE", True, raising=False)
    monkeypatch.setattr(qtts, "_ML_IMPORT_ERROR", None, raising=False)
    monkeypatch.setattr(qtts, "_mlx_load_tts_model", lambda _repo: model)
    # The node nulls out `np` if mlx-audio fails to import (same try
    # block). Put a real numpy back so _to_float32_mono / upsampler
    # work even when mlx-audio isn't installed in the test env.
    monkeypatch.setattr(qtts, "np", np, raising=False)
    return model


def _make_node(**kwargs) -> qtts.QwenTTSMlxNode:
    defaults = dict(
        node_id="qwen_tts_test",
        # Keep output_sample_rate == sample_rate so the upsampler is a
        # no-op and tests can assert on sample counts directly.
        sample_rate=24000,
        output_sample_rate=24000,
        streaming_interval=0.1,
        passthrough_text=True,
    )
    defaults.update(kwargs)
    return qtts.QwenTTSMlxNode(**defaults)


async def _drive_sentence(node, text: str) -> List[Any]:
    outputs: List[Any] = []
    async for out in node.process(RuntimeData.text(text)):
        outputs.append(out)
    return outputs


async def _drive_envelope(node, port: str, payload: dict | None = None) -> None:
    env = {"__aux_port__": port, "payload": payload or {}}
    async for _ in node.process(RuntimeData.text(json.dumps(env))):
        pytest.fail(
            f"aux envelope for port={port!r} unexpectedly produced output"
        )


def _audio_chunks(outputs: List[Any]) -> List[np.ndarray]:
    return [
        o.as_numpy() for o in outputs
        if isinstance(o, RuntimeData) and o.is_audio()
    ]


def _text_chunks(outputs: List[Any]) -> List[str]:
    return [
        o.as_text() for o in outputs
        if isinstance(o, RuntimeData) and o.is_text()
    ]


def _chunk(n: int, value: float = 0.1) -> np.ndarray:
    return np.full(n, value, dtype=np.float32)


# ──────────────────────────── tests ───────────────────────────────────


@pytest.mark.asyncio
async def test_passthrough_text_then_audio_in_order(fake_tts_model):
    """
    Sentence in → text passthrough + audio chunks in model order.
    """
    node = _make_node()
    await node.initialize()
    try:
        script = [_chunk(16, 0.1), _chunk(24, 0.2), _chunk(8, 0.3)]
        fake_tts_model.queue(script)

        outs = await _drive_sentence(node, "Hello there.")
        texts = _text_chunks(outs)
        audio = _audio_chunks(outs)

        # Text comes first (UI transcript), then audio chunks.
        assert texts == ["Hello there."], texts
        assert len(audio) == len(script), (
            f"expected {len(script)} audio chunks, got {len(audio)}"
        )
        for got, want in zip(audio, script):
            assert got.shape == want.shape
            assert np.allclose(got, want), "audio chunk reordered or corrupted"
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_text_end_emits_audio_end_sentinel(fake_tts_model):
    """``<|text_end|>`` in input triggers a single ``<|audio_end|>`` output."""
    node = _make_node()
    await node.initialize()
    try:
        fake_tts_model.queue([_chunk(4)])

        outs = await _drive_sentence(node, "Goodbye.<|text_end|>")
        texts = _text_chunks(outs)
        # The `<|text_end|>` arrives embedded; passthrough forwards it
        # verbatim for the UI, but the TTS stripped it before feeding
        # the model, and an `<|audio_end|>` is appended at the tail.
        assert texts[0] == "Goodbye.<|text_end|>"
        assert texts[-1] == "<|audio_end|>", texts
        # The model was called with cleaned text (no sentinel).
        assert fake_tts_model.calls
        assert fake_tts_model.calls[-1]["text"] == "Goodbye.", (
            f"sentinel leaked into TTS input: {fake_tts_model.calls[-1]!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_ui_channel_text_is_not_spoken(fake_tts_model):
    """
    UI-channel text must be forwarded for display but never fed to the
    synthesis model — otherwise code blocks / markdown get spoken aloud.
    """
    node = _make_node()
    await node.initialize()
    try:
        fake_tts_model.calls.clear()
        # Warmup's call is already in `calls`; clear and send UI text.
        outs: List[Any] = []
        async for out in node.process(
            RuntimeData.text("```python\nprint(1)\n```", channel="ui")
        ):
            outs.append(out)

        texts = _text_chunks(outs)
        audio = _audio_chunks(outs)
        assert audio == [], "UI text must not be synthesised"
        assert len(texts) == 1
        assert texts[0] == "```python\nprint(1)\n```"
        # Channel preserved for downstream display sink.
        assert outs[0].metadata.channel == "ui"
        # Model was never called for this input.
        assert fake_tts_model.calls == [], (
            f"UI text reached the TTS model: {fake_tts_model.calls!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_pre_synthesis_barge_in_is_cleared_before_next_sentence(
    fake_tts_model,
):
    """
    Pipeline semantics: this pipeline's VAD fires ``barge_in`` on EVERY
    ``speech_start`` — including the very first utterance, when no
    response is in flight. The user's transcribed speech then arrives
    as the next turn's input, and the LLM's response eventually
    reaches the TTS as sentences. By the time a sentence lands here,
    any barge-in latched earlier was for a response that already
    finished naturally — it must NOT cancel the upcoming sentence,
    otherwise the user never hears the reply to their first question.

    This was caught in production: my initial "fix" held the flag
    across the entry, which silently killed the first response on
    every session. The entry-reset behavior is correct for this
    architecture; this test pins it down.
    """
    node = _make_node()
    await node.initialize()
    try:
        fake_tts_model.calls.clear()
        # Simulate: VAD fired barge_in before this sentence arrived.
        node.request_barge_in()
        assert node._interrupt is True

        script = [_chunk(8, 0.5), _chunk(8, 0.6)]
        fake_tts_model.queue(script)

        outs = await _drive_sentence(node, "This must be spoken.")
        audio = _audio_chunks(outs)

        assert fake_tts_model.calls, (
            "stale barge-in from a prior turn should not prevent "
            "synthesis of the next sentence"
        )
        assert fake_tts_model.calls[-1]["text"] == "This must be spoken."
        assert len(audio) == len(script), (
            f"expected {len(script)} audio chunks, got {len(audio)}"
        )
        for got, want in zip(audio, script):
            assert np.allclose(got, want)
        # Latch is cleared.
        assert node._interrupt is False
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_mid_synthesis_barge_in_halts_no_stale_audio(fake_tts_model):
    """
    Barge-in during an active synthesis halts the chunk stream promptly.
    Audio chunks produced AFTER the interrupt latches must never reach
    the output — that's the stale-audio-to-speaker case.
    """
    node = _make_node()
    await node.initialize()
    try:
        # Long script: barge-in after a few chunks. Chunks past index 10
        # must not be emitted.
        script = [_chunk(8, 0.01 * i) for i in range(30)]
        fake_tts_model.queue(script)

        gen = node.process(RuntimeData.text("a very long sentence"))
        collected: List[Any] = []
        drain_task = asyncio.create_task(_collect_into(gen, collected))
        # Give the producer time to push a handful of chunks, then latch.
        await asyncio.sleep(0.015)
        node.request_barge_in()
        await drain_task

        audio = _audio_chunks(collected)
        # At least one chunk should have made it before the latch; way
        # fewer than the full script should have surfaced.
        assert 0 < len(audio) < len(script), (
            f"mid-synth barge-in either no-op or full-flush: "
            f"{len(audio)}/{len(script)} chunks"
        )
        # Lax bound: if more than half the script leaks, the latch
        # didn't work. In practice we expect single-digit chunks.
        assert len(audio) < len(script) // 2, (
            f"too many chunks leaked past barge-in: {len(audio)}/{len(script)}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_aux_envelope_barge_in_latches_flag(fake_tts_model):
    """
    The ``{"__aux_port__": "barge_in"}`` envelope, delivered via the
    normal input queue, flips the interrupt latch identically to a
    direct ``request_barge_in()`` call — and (per this pipeline's
    semantics) the latch is cleared at the start of the next sentence.
    """
    node = _make_node()
    await node.initialize()
    try:
        fake_tts_model.calls.clear()
        await _drive_envelope(node, "barge_in")
        assert node._interrupt is True

        # Next sentence runs normally; flag is cleared on entry.
        fake_tts_model.queue([_chunk(4)])
        outs = await _drive_sentence(node, "fresh")
        assert len(_audio_chunks(outs)) == 1
        assert node._interrupt is False
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_consecutive_sentences_no_audio_crosstalk(fake_tts_model):
    """
    Back-to-back sentences: each sentence's audio must match its own
    script — no state from the prior sentence's producer can leak.
    """
    node = _make_node()
    await node.initialize()
    try:
        scripts = [
            [_chunk(8, 0.1 + 0.1 * i) for i in range(3)]
            for _ in range(6)
        ]
        # Make every sentence's samples distinct so we can tell them apart.
        distinctive = []
        for s_idx, s in enumerate(scripts):
            per_sentence = [
                np.full(c.shape, 0.1 * (s_idx + 1) + 0.01 * j, dtype=np.float32)
                for j, c in enumerate(s)
            ]
            distinctive.append(per_sentence)
            fake_tts_model.queue(per_sentence)

        for idx, script in enumerate(distinctive):
            outs = await _drive_sentence(node, f"sentence {idx}")
            audio = _audio_chunks(outs)
            assert len(audio) == len(script), (
                f"sentence {idx}: chunk count mismatch"
            )
            for got, want in zip(audio, script):
                assert np.allclose(got, want), (
                    f"sentence {idx}: audio chunk mismatches its own script "
                    f"— cross-talk from another sentence"
                )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_mid_synthesis_barge_in_does_not_leak_into_next_sentence(
    fake_tts_model,
):
    """
    Barge-in cuts sentence 1 short; sentence 2 follows immediately.
    Sentence 2's audio must match ITS OWN script, not carry forward any
    of sentence 1's leftover chunks from the producer's pipeline.
    """
    node = _make_node()
    await node.initialize()
    try:
        # Sentence 1: long, we'll interrupt it. Use a distinctive
        # signal so any leakage into sentence 2 is obvious.
        s1 = [np.full(8, 0.77, dtype=np.float32) for _ in range(40)]
        s2 = [np.full(8, 0.11, dtype=np.float32) for _ in range(3)]
        fake_tts_model.queue(s1)
        fake_tts_model.queue(s2)

        gen = node.process(RuntimeData.text("interrupt me"))
        collected: List[Any] = []
        drain_task = asyncio.create_task(_collect_into(gen, collected))
        await asyncio.sleep(0.01)
        node.request_barge_in()
        await drain_task

        outs2 = await _drive_sentence(node, "fresh sentence")
        audio2 = _audio_chunks(outs2)
        assert len(audio2) == len(s2), (
            f"sentence 2: chunk count mismatch. "
            f"got {len(audio2)}, expected {len(s2)}"
        )
        for got, want in zip(audio2, s2):
            assert np.allclose(got, want), (
                "sentence 2 audio carries values from sentence 1 — "
                "stale audio leaked into the next sentence"
            )
            assert not np.allclose(got, 0.77), (
                f"sentence 2 leaked sentence 1's distinctive 0.77 signal"
            )
    finally:
        await node.cleanup()


# ──────────────────────────── helpers ─────────────────────────────────


async def _collect_into(gen, collected: List[Any]) -> None:
    async for out in gen:
        collected.append(out)
