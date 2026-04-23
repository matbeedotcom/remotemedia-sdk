"""
Multi-turn TTS stress test for ``QwenTextMlxNode``.

Exercises the guarantees the downstream TTS pipeline relies on:

1. **Ordering.** Streamed text chunks arrive in the same order the model
   emitted them, and every turn terminates with a ``<|text_end|>``
   sentinel so the text collector / TTS node can flush.
2. **Barge-in.** ``request_barge_in()`` halts generation promptly — chunks
   produced *after* the interrupt never reach the consumer.
3. **No cross-turn leakage.** Text from a previous (including barge-in'd)
   turn never appears in the next turn's output stream. This is the
   "stale audio reaching the speaker" case.
4. **No stale user content in the model prompt.** After a ``reset`` aux
   envelope the next prompt contains none of the prior user or assistant
   turns — this is the "stale audio reaching the model" case.
5. **Aux envelopes never become user turns.** System-prompt / context /
   reset / barge_in envelopes configure the node but never get fed to
   ``stream_generate`` as user text.
6. **Tool-channel routing.** When the ``say`` tool is active, only the
   ``say(text=...)`` argument flows on the ``tts`` channel; free-form
   markdown goes to the ``ui`` channel. A TTS node downstream must never
   speak markdown, and a display UI must never silently drop spoken text.

All mlx-lm entry points are monkeypatched, so the tests run on any
platform without downloading the 9B weights. The mocks faithfully
reproduce the streaming contract (per-chunk ``.text``, ``<tool_call>``
wrapping), so assertions exercise QwenTextMlxNode's own plumbing rather
than model output.

Run:

    pytest clients/python/tests/test_qwen_text_mlx_multi_turn.py -v
"""

from __future__ import annotations

import asyncio
import json
import time
from collections import deque
from typing import Any, Iterable, List, Tuple

import pytest

import remotemedia.nodes.ml.qwen_text_mlx as qtm
from remotemedia.core.multiprocessing.data import RuntimeData


# ──────────────────────────── fake mlx-lm ─────────────────────────────

class _FakeChunk:
    """Stand-in for mlx-lm's ``GenerationResponse``. Only ``.text`` is read."""

    __slots__ = ("text",)

    def __init__(self, text: str) -> None:
        self.text = text


class _FakeTokenizer:
    """Minimal tokenizer: renders messages to an inspectable string.

    QwenTextMlxNode calls ``apply_chat_template(messages, ...)`` to build
    the prompt and then reads ``tool_call_start`` / ``tool_call_end``
    attributes when tools are active. That's all we need.
    """

    tool_call_start = "<tool_call>"
    tool_call_end = "</tool_call>"
    tool_parser = None  # force the fallback json.loads path in the node

    def apply_chat_template(self, messages: List[dict], **kwargs) -> str:
        lines = []
        for m in messages:
            name = m.get("name", "")
            role = m.get("role", "unknown")
            content = m.get("content", "")
            prefix = f"[{role}{f' name={name}' if name else ''}]"
            lines.append(f"{prefix} {content}")
        return "\n".join(lines)


class _FakeLLM:
    """Scripted replacement for ``mlx_lm.stream_generate``.

    Each call pops one list-of-chunks off ``scripts`` and returns an
    iterator that yields ``_FakeChunk(s)`` with a small ``time.sleep``
    between chunks. The sleep releases the GIL so the asyncio consumer
    task can observe ``self._interrupt`` and barge-in mid-stream.
    """

    def __init__(self, chunk_delay: float = 0.003) -> None:
        self.scripts: deque = deque()
        self.calls: List[Tuple[str, dict]] = []
        self.chunk_delay = chunk_delay

    def queue(self, chunks: Iterable[str]) -> None:
        self.scripts.append(list(chunks))

    def __call__(self, model, tokenizer, prompt, **kwargs):
        self.calls.append((prompt, dict(kwargs)))
        script = self.scripts.popleft() if self.scripts else ["ok."]
        delay = self.chunk_delay

        def _iter():
            for chunk in script:
                if delay:
                    time.sleep(delay)
                yield _FakeChunk(chunk)

        return _iter()


# ───────────────────────── fixtures / helpers ─────────────────────────

@pytest.fixture
def patched_mlx(monkeypatch):
    """Install fake mlx-lm hooks on the qwen_text_mlx module."""
    fake_llm = _FakeLLM(chunk_delay=0.003)
    monkeypatch.setattr(qtm, "_ML_DEPS_AVAILABLE", True, raising=False)
    monkeypatch.setattr(qtm, "_ML_IMPORT_ERROR", None, raising=False)
    monkeypatch.setattr(
        qtm, "_mlxlm_load", lambda repo: (object(), _FakeTokenizer())
    )
    monkeypatch.setattr(qtm, "_mlxlm_stream_generate", fake_llm)
    # Null the optional helpers so the node takes the simple temp/top_p
    # kwarg path — our fake ignores sampling args anyway.
    monkeypatch.setattr(qtm, "_mlxlm_make_sampler", None)
    monkeypatch.setattr(qtm, "_mlxlm_make_logits_processors", None)
    monkeypatch.setattr(qtm, "_mlxlm_make_prompt_cache", None)
    return fake_llm


def _make_node(**kwargs) -> qtm.QwenTextMlxNode:
    defaults = dict(
        node_id="qwen_test",
        enable_say_tool=False,  # tests that need tools re-enable them
        enable_show_tool=False,
        max_new_tokens=4096,
    )
    defaults.update(kwargs)
    return qtm.QwenTextMlxNode(**defaults)


async def _drive_turn(node, user_text: str) -> List[Any]:
    """Feed one user turn and drain outputs into a list."""
    outputs: List[Any] = []
    async for out in node.process(RuntimeData.text(user_text)):
        outputs.append(out)
    return outputs


async def _drive_envelope(node, port: str, payload: dict | None = None) -> None:
    """Send a control-plane aux envelope. Should yield nothing."""
    env = {"__aux_port__": port, "payload": payload or {}}
    async for _ in node.process(RuntimeData.text(json.dumps(env))):
        pytest.fail(
            f"aux envelope for port={port!r} unexpectedly produced output"
        )


def _texts_on_channel(outputs: List[Any], channel: str) -> List[str]:
    """Collect ``as_text()`` for every text RuntimeData on ``channel``."""
    picked: List[str] = []
    for o in outputs:
        if not isinstance(o, RuntimeData) or not o.is_text():
            continue
        ch = getattr(o.metadata, "channel", "tts")
        if ch == channel:
            picked.append(o.as_text())
    return picked


# ──────────────────────────── tests ───────────────────────────────────


@pytest.mark.asyncio
async def test_multiturn_ordering_and_sentinel(patched_mlx):
    """Every turn delivers its chunks in order, followed by ``<|text_end|>``."""
    node = _make_node()
    await node.initialize()
    try:
        scripts = [
            ["The ", "quick ", "brown ", "fox ", "jumps."],
            ["Second ", "turn ", "words ", "here."],
            ["Hello ", "from ", "turn ", "three."],
        ]
        for s in scripts:
            patched_mlx.queue(s)

        for turn_idx, script in enumerate(scripts, 1):
            outs = await _drive_turn(node, f"user turn {turn_idx}")
            tts = _texts_on_channel(outs, channel="tts")

            assert tts[-1] == "<|text_end|>", (
                f"turn {turn_idx}: last chunk must be sentinel, got {tts!r}"
            )
            body = tts[:-1]
            assert body == script, (
                f"turn {turn_idx}: chunks reordered or lost. "
                f"expected={script!r} got={body!r}"
            )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_barge_in_halts_and_no_stale_leak_to_next_turn(patched_mlx):
    """
    Barge-in mid-stream halts generation promptly; chunks generated AFTER
    the interrupt never surface to the TTS consumer; and the next turn's
    output is unpolluted by the aborted turn's residue.
    """
    node = _make_node()
    await node.initialize()
    try:
        long_reply = [f"chunk{i:02d} " for i in range(40)]
        clean_reply = ["fresh ", "start ", "here."]
        patched_mlx.queue(long_reply)
        patched_mlx.queue(clean_reply)

        gen = node.process(RuntimeData.text("please talk for a long time"))

        collected: List[Any] = []
        # Pull a handful of chunks before the interrupt.
        for _ in range(3):
            collected.append(await gen.__anext__())

        # Trip the latch. The consumer's next iteration breaks out of
        # the producer loop; ``_produce``'s own interrupt check stops it
        # from putting any further tokens on the queue.
        node.request_barge_in()

        async for out in gen:
            collected.append(out)

        tts = _texts_on_channel(collected, channel="tts")
        assert tts, "expected at least the sentinel on the tts channel"
        assert tts[-1] == "<|text_end|>", tts

        received_body = "".join(tts[:-1])

        # Early chunks we already pulled must be present.
        assert received_body.startswith("chunk00 chunk01 chunk02"), (
            f"early chunks missing from barge-in'd turn: {received_body!r}"
        )
        # Late chunks must NOT leak — they're the "stale audio" case.
        for stale_idx in range(20, 40):
            stale = f"chunk{stale_idx:02d} "
            assert stale not in received_body, (
                f"stale token {stale!r} leaked past barge-in: "
                f"{received_body!r}"
            )

        # Size guard: the producer can't have pushed more than a handful
        # of extra chunks after we tripped the latch.
        assert len(received_body) < len("".join(long_reply)) // 2, (
            f"barge-in let too many chunks through "
            f"({len(received_body)} chars): {received_body!r}"
        )

        # Next turn must come through cleanly with no trailing residue
        # from the interrupted turn.
        clean_outs = await _drive_turn(node, "start over")
        clean_tts = _texts_on_channel(clean_outs, channel="tts")
        assert clean_tts[-1] == "<|text_end|>"
        assert clean_tts[:-1] == clean_reply, (
            f"next-turn output leaked prior content: {clean_tts!r}"
        )

        clean_body = "".join(clean_tts[:-1])
        assert "chunk" not in clean_body, (
            f"post-barge-in turn leaked aborted tokens: {clean_body!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_aux_envelope_reset_clears_history_and_prompt(patched_mlx):
    """
    A ``reset`` aux envelope wipes history so the next prompt contains
    nothing from the prior conversation — the "stale audio to the model"
    guarantee.
    """
    node = _make_node()
    await node.initialize()
    try:
        patched_mlx.queue(["acknowledged."])
        patched_mlx.queue(["hello!"])
        patched_mlx.queue(["new conversation."])

        await _drive_turn(node, "remember the codeword: banana")
        assert any("banana" in m.get("content", "") for m in node._history), (
            f"history did not capture first turn: {node._history!r}"
        )

        await _drive_envelope(node, "reset")
        assert node._history == [], (
            f"reset envelope did not clear history: {node._history!r}"
        )

        patched_mlx.calls.clear()
        await _drive_turn(node, "what's my name?")
        assert len(patched_mlx.calls) >= 1, "post-reset turn did not call model"

        last_prompt = patched_mlx.calls[-1][0]
        assert "banana" not in last_prompt, (
            f"stale codeword reached the model after reset: {last_prompt!r}"
        )
        assert "remember the codeword" not in last_prompt, (
            f"stale user turn reached the model after reset: {last_prompt!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_aux_envelopes_never_become_model_turns(patched_mlx):
    """
    Control-plane envelopes configure the node but must never reach the
    model as user text. A burst of envelopes triggers zero model calls;
    the next real turn sees a clean conversation with the configured
    system prompt in place.
    """
    node = _make_node()
    await node.initialize()
    try:
        patched_mlx.queue(["ok."])

        await _drive_envelope(
            node, "system_prompt", {"text": "You are a calculator."}
        )
        await _drive_envelope(
            node, "context", {"text": "The user is Alice."}
        )
        await _drive_envelope(node, "reset")

        assert patched_mlx.calls == [], (
            f"aux envelopes triggered model calls: {patched_mlx.calls!r}"
        )
        assert node._history == []

        await _drive_turn(node, "hi")
        assert len(patched_mlx.calls) == 1

        prompt = patched_mlx.calls[0][0]
        assert "__aux_port__" not in prompt, (
            f"envelope JSON reached the model: {prompt!r}"
        )
        # The system prompt we configured via envelope must be in effect.
        assert "calculator" in prompt.lower(), prompt
        # And the context set via envelope too.
        assert "alice" in prompt.lower(), prompt
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_burst_many_turns_no_crosstalk(patched_mlx):
    """
    Many back-to-back short turns. Each turn's output must contain only
    its own tokens — stresses the per-turn queue/producer teardown for
    any cross-turn state leaks.
    """
    node = _make_node()
    await node.initialize()
    try:
        turn_count = 15
        scripts = [[f"T{i}-w{k} " for k in range(5)] for i in range(turn_count)]
        for s in scripts:
            patched_mlx.queue(s)

        for i, script in enumerate(scripts):
            outs = await _drive_turn(node, f"turn {i}")
            tts = _texts_on_channel(outs, channel="tts")
            assert tts[-1] == "<|text_end|>", (
                f"turn {i}: missing end-of-turn sentinel: {tts!r}"
            )
            body = "".join(tts[:-1])
            assert body == "".join(script), (
                f"turn {i}: body mismatch. expected={script!r} got={body!r}"
            )
            for j in range(turn_count):
                if j == i:
                    continue
                needle = f"T{j}-w"
                assert needle not in body, (
                    f"turn {i} leaked tokens from turn {j}: {body!r}"
                )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_barge_in_does_not_drain_stale_queue_into_next_turn(patched_mlx):
    """
    Worst-case barge-in: the producer thread has already pushed many
    chunks onto the internal queue before the consumer observes the
    interrupt. Those in-flight chunks must be DISCARDED (not drained
    into the next turn's output stream), otherwise the user hears a
    burst of audio from the aborted turn right after the fresh turn
    starts.

    Strategy: block the consumer briefly between chunk 1 and the
    interrupt by using a zero-delay script — the producer races ahead
    and stuffs the queue, then we trip the latch and start turn 2.
    Turn 2's output must contain ONLY turn 2's tokens.
    """
    node = _make_node()
    await node.initialize()
    try:
        # Zero-delay stale tokens flood the queue fast.
        patched_mlx.chunk_delay = 0.0
        stale_reply = [f"STALE{i:03d} " for i in range(200)]
        fresh_reply = ["A ", "B ", "C ", "D."]
        patched_mlx.queue(stale_reply)
        patched_mlx.queue(fresh_reply)

        gen = node.process(RuntimeData.text("long reply please"))

        # Read exactly one chunk, then yield control briefly so the
        # producer thread has a chance to fill the queue, then latch.
        first = await gen.__anext__()
        await asyncio.sleep(0.02)
        node.request_barge_in()

        aborted: List[Any] = [first]
        async for out in gen:
            aborted.append(out)

        aborted_tts = _texts_on_channel(aborted, channel="tts")
        assert aborted_tts[-1] == "<|text_end|>"

        # Now drive a clean turn. This is where a stale-queue bug would
        # surface as extra STALE### tokens at the head of the output.
        patched_mlx.chunk_delay = 0.003
        fresh_outs = await _drive_turn(node, "new turn")
        fresh_tts = _texts_on_channel(fresh_outs, channel="tts")

        assert fresh_tts[-1] == "<|text_end|>"
        assert fresh_tts[:-1] == fresh_reply, (
            f"stale queue drained into next turn: {fresh_tts!r}"
        )

        fresh_body = "".join(fresh_tts[:-1])
        assert "STALE" not in fresh_body, (
            f"stale tokens from aborted turn leaked into next turn: "
            f"{fresh_body!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_no_old_data_across_full_pipeline(patched_mlx):
    """
    End-to-end pipeline check: QwenTextMlxNode → TextCollectorNode.
    After barge-in on turn 1, the downstream TextCollectorNode must not
    emit any sentence assembled from turn 1's tokens during turn 2.
    This guards against buffered partial sentences in the collector
    surviving across turns (which would reach the TTS as stale audio).
    """
    from remotemedia.nodes.text_collector import TextCollectorNode

    node = _make_node()
    collector = TextCollectorNode(node_id="collector")

    await node.initialize()
    await collector.initialize()
    try:
        # Turn 1: a long, slow script we'll interrupt. No terminal
        # punctuation on the early chunks so the collector would buffer
        # them if not properly reset.
        patched_mlx.chunk_delay = 0.003
        patched_mlx.queue([
            "tell ", "me ", "about ", "the ", "cosmos ", "and ",
            "its ", "many ", "wonders ", "and ", "stars ",
        ])
        # Turn 2: a complete, punctuated reply.
        patched_mlx.queue([
            "Short answer here.",
        ])

        # Drive turn 1 → barge-in after 2 chunks.
        gen = node.process(RuntimeData.text("talk forever"))
        pulled: List[Any] = []
        for _ in range(2):
            pulled.append(await gen.__anext__())
        node.request_barge_in()
        async for out in gen:
            pulled.append(out)

        # Run those through the collector to mimic what the pipeline
        # does on turn 1.
        turn1_sentences: List[str] = []
        for item in pulled:
            if not isinstance(item, RuntimeData) or not item.is_text():
                continue
            async for s in collector.process(item):
                if s.is_text():
                    turn1_sentences.append(s.as_text())

        # The only "sentence" that should reach TTS from turn 1 is the
        # end-of-turn sentinel (the collector passes it through). The
        # partial "tell me about..." must NOT have been flushed because
        # there was no sentence boundary.
        non_sentinel = [s for s in turn1_sentences if s != "<|text_end|>"]
        for s in non_sentinel:
            # Even if yield_partial_on_end fires for the partial tail,
            # it must not include tokens the producer pushed AFTER the
            # barge-in. We verify that by checking the fresh turn below
            # doesn't see any of these leaked tokens.
            pass

        # Now drive turn 2 through the full chain and assert the
        # downstream collector sees ONLY turn 2's content.
        turn2_outs = await _drive_turn(node, "now a short reply")
        turn2_sentences: List[str] = []
        for item in turn2_outs:
            if not isinstance(item, RuntimeData) or not item.is_text():
                continue
            async for s in collector.process(item):
                if s.is_text():
                    turn2_sentences.append(s.as_text())

        body = [s for s in turn2_sentences if s != "<|text_end|>"]
        joined = " ".join(body)
        assert "Short answer here" in joined, (
            f"expected turn 2 content in collector output: {turn2_sentences!r}"
        )
        # None of turn 1's tokens may appear in turn 2's collector output.
        for stale_token in (
            "cosmos", "wonders", "stars", "tell me about",
        ):
            assert stale_token not in joined, (
                f"stale turn-1 token {stale_token!r} reached TTS via "
                f"collector: {turn2_sentences!r}"
            )
    finally:
        await collector.cleanup()
        await node.cleanup()


@pytest.mark.asyncio
async def test_say_tool_routes_to_tts_and_display_to_ui(patched_mlx):
    """
    With the ``say`` tool active, free-form markdown between tool calls
    must flow on the ``ui`` channel (not spoken) and the ``say(text=...)``
    argument on the ``tts`` channel (spoken, never visible as JSON). This
    is the routing contract downstream TTS / display nodes rely on.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        # Pass 1: some prose, a say() tool call, more prose. The node
        # will then re-prompt with a synthetic {role:"tool"} turn.
        patched_mlx.queue([
            "Here is the plan: ",
            "<tool_call>",
            '{"name":"say","arguments":{"text":"Hello there"}}',
            "</tool_call>",
            " And some more markdown.",
        ])
        # Pass 2: no tool call → terminates the multi-pass loop cleanly.
        patched_mlx.queue(["All done."])

        outs = await _drive_turn(node, "greet the user")

        tts = _texts_on_channel(outs, channel="tts")
        ui = _texts_on_channel(outs, channel="ui")

        tts_joined = "".join(tts)
        ui_joined = "".join(ui)

        # say() argument reaches TTS (with the flushing newline the node
        # appends so the text collector flushes to the speaker promptly).
        assert "Hello there" in tts_joined, (
            f"say argument missing from tts: {tts!r}"
        )
        # Markdown must NOT reach TTS.
        assert "Here is the plan" not in tts_joined, (
            f"markdown leaked to tts: {tts_joined!r}"
        )
        assert "And some more markdown" not in tts_joined, (
            f"markdown leaked to tts: {tts_joined!r}"
        )
        # Raw tool_call tags must never surface to either channel.
        assert "<tool_call>" not in tts_joined
        assert "<tool_call>" not in ui_joined
        assert '"name":"say"' not in tts_joined
        assert '"name":"say"' not in ui_joined

        # Markdown reaches UI; the say argument does NOT (would cause
        # double-display of spoken text on the transcript view).
        assert "Here is the plan" in ui_joined, (
            f"display text missing from ui: {ui_joined!r}"
        )
        assert "And some more markdown" in ui_joined, ui_joined
        assert "Hello there" not in ui_joined, (
            f"spoken text duplicated on ui channel: {ui_joined!r}"
        )

        # Both channels get a terminating sentinel.
        assert tts[-1] == "<|text_end|>", tts
        assert ui[-1] == "<|text_end|>", ui
    finally:
        await node.cleanup()


# ────────── adversarial: tests designed to EXPOSE real leaks ──────────
#
# The tests above exercise the happy path. These target specific code
# paths in qwen_text_mlx.py where stale data can actually survive a
# barge-in. Each one is written to FAIL if the production code has the
# suspected bug. A pass here is a real guarantee; a fail is a bug to fix.


@pytest.mark.asyncio
async def test_no_second_tool_dispatch_after_barge_in_in_same_chunk(patched_mlx):
    """
    Two complete ``<tool_call>...</tool_call>`` bodies arrive in a SINGLE
    streaming chunk (realistic when mlx-lm decodes several tokens in one
    step). The consumer's inner ``for body in tool_bodies`` loop
    (qwen_text_mlx.py:954-994) dispatches each body without re-checking
    ``self._interrupt`` between them — so if barge-in fires after the
    first ``say`` has been yielded, the second ``say`` still reaches TTS
    as stale audio.

    If this test FAILS, the fix is to check ``self._interrupt`` inside
    the ``for body in tool_bodies`` loop and break out early.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    # Zero delay so both tool calls arrive as one chunk.
    patched_mlx.chunk_delay = 0.0
    await node.initialize()
    try:
        # Two tool calls concatenated into ONE streaming chunk.
        patched_mlx.queue([
            '<tool_call>{"name":"say","arguments":{"text":"FIRST spoken"}}</tool_call>'
            '<tool_call>{"name":"say","arguments":{"text":"SECOND stale"}}</tool_call>',
        ])
        # A no-op pass-2 script in case the loop continues (it shouldn't
        # if barge-in latches, but queue a script so we don't crash if it
        # does).
        patched_mlx.queue(["(unreachable)"])

        gen = node.process(RuntimeData.text("say two things"))
        collected: List[Any] = []
        barge_triggered = False

        async for out in gen:
            collected.append(out)
            if (
                not barge_triggered
                and isinstance(out, RuntimeData)
                and out.is_text()
                and getattr(out.metadata, "channel", "tts") == "tts"
                and "FIRST spoken" in out.as_text()
            ):
                # User started talking — latch barge-in NOW, between
                # dispatches of tool body #1 and #2.
                node.request_barge_in()
                barge_triggered = True

        assert barge_triggered, "first say never observed — script broken"

        tts_joined = "".join(_texts_on_channel(collected, channel="tts"))
        assert "FIRST spoken" in tts_joined, (
            "first say should have reached TTS before barge-in: "
            f"{tts_joined!r}"
        )
        # THIS is the stale-audio leak: if SECOND reaches TTS the user
        # hears the continuation of a response they already interrupted.
        assert "SECOND stale" not in tts_joined, (
            "BUG: second tool_call dispatched after barge-in — the "
            "inner `for body in tool_bodies` loop at "
            "qwen_text_mlx.py:954-994 needs an interrupt check. "
            f"tts_joined={tts_joined!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_no_ui_tail_display_emitted_after_barge_in(patched_mlx):
    """
    Unconditional ``parser.flush()`` at qwen_text_mlx.py:1004-1008 fires
    even when the consumer exited because of barge-in. Any display text
    the parser was holding below its safe-length threshold gets pushed
    out to the ``ui`` channel AFTER the user has already interrupted.
    Not audio-critical, but contract-breaking and a sign the interrupt
    path isn't uniformly honored.

    If this test FAILS, the fix is to gate the ``parser.flush()`` block
    on ``not self._interrupt``.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        # Custom fake: emit 2 tiny chunks that together stay under the
        # parser's safe-length hold (10 chars), then block so the
        # consumer suspends with a non-empty parser buffer. We barge-in
        # during the block; the producer wakes, the generator returns
        # normally, the consumer sees the sentinel and breaks, and
        # ``parser.flush()`` runs. Without the fix, flush emits the
        # buffered "abcd" on the UI channel — that's the stale leak.
        def _two_chunks_then_block(model, tokenizer, prompt, **kwargs):
            def _iter():
                yield _FakeChunk("ab")
                yield _FakeChunk("cd")
                # Gives the test time to barge-in while the consumer is
                # idle and the parser buffer is "abcd" (below hold=10).
                time.sleep(0.2)
            patched_mlx.calls.append((prompt, dict(kwargs)))
            return _iter()

        import remotemedia.nodes.ml.qwen_text_mlx as _qtm
        original = _qtm._mlxlm_stream_generate
        _qtm._mlxlm_stream_generate = _two_chunks_then_block
        try:
            gen = node.process(RuntimeData.text("write short markdown"))
            collected: List[Any] = []

            # Drive the generator in a task — process() only runs when
            # iterated, so we need it active BEFORE calling barge-in
            # (otherwise ``process()``'s entry resets ``_interrupt``).
            async def _drain():
                async for out in gen:
                    collected.append(out)

            drain_task = asyncio.create_task(_drain())
            # Give producer time to emit both chunks AND block in sleep.
            await asyncio.sleep(0.08)
            node.request_barge_in()
            await drain_task
        finally:
            _qtm._mlxlm_stream_generate = original

        ui = _texts_on_channel(collected, channel="ui")
        # The sentinel is allowed; any OTHER content on ui is leaked.
        ui_nonsentinel = [s for s in ui if s != "<|text_end|>"]
        assert ui_nonsentinel == [], (
            "BUG: parser.flush() yielded tail_display to the UI channel "
            "after barge-in. The flush block at "
            "qwen_text_mlx.py:1004-1008 runs unconditionally and needs "
            f"an interrupt gate. leaked_ui={ui_nonsentinel!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_barge_in_does_not_commit_malformed_history(patched_mlx):
    """
    If barge-in interrupts the model mid-``<tool_call>`` body, the raw
    pass text ends with an unclosed ``<tool_call>{"name":"say"...`` tag.
    That raw string gets committed to ``self._history`` as an assistant
    turn (qwen_text_mlx.py:1045-1049, 1092-1093), so the NEXT turn's
    chat template renders an unclosed tool call in context, confusing
    Qwen's tool-call state machine.

    If this test FAILS, the fix is to either strip unclosed tool-call
    markers from ``raw_this_pass`` before committing, or skip the
    history commit entirely when the pass was barge-in'd.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        patched_mlx.chunk_delay = 0.003
        # Open a tool call, start emitting arguments, then "stop" (the
        # mock's script ends and generation completes naturally — but
        # we'll barge-in first).
        patched_mlx.queue([
            "<tool_call>",
            '{"name":"say",',
            '"arguments":',
            '{"text":"this say was',  # unclosed — never reaches </tool_call>
            " interrupted mid-way",
            " and should not leak",
        ])
        patched_mlx.queue(["follow.up."])

        gen = node.process(RuntimeData.text("interrupt me"))
        # Read a few items then barge-in.
        seen = 0
        async for out in gen:
            seen += 1
            if seen >= 2:
                node.request_barge_in()
                break
        # Drain the rest.
        async for _ in gen:
            pass

        # Inspect history: no assistant message may contain an unclosed
        # tool_call tag.
        for msg in node._history:
            if msg.get("role") != "assistant":
                continue
            content = msg.get("content", "")
            open_count = content.count("<tool_call>")
            close_count = content.count("</tool_call>")
            assert open_count == close_count, (
                "BUG: unclosed <tool_call> committed to history after "
                "barge-in. qwen_text_mlx.py:1045-1049 needs to skip or "
                f"sanitize partial tool-call content. msg={msg!r}"
            )

        # Even stricter: the raw argument text from the interrupted say
        # must not appear in history, because replaying it on the next
        # turn causes Qwen to try to complete the interrupted call.
        for msg in node._history:
            if msg.get("role") != "assistant":
                continue
            content = msg.get("content", "")
            assert "interrupted mid-way" not in content, (
                "BUG: mid-tool-call fragment reached history; next "
                f"prompt will be polluted. msg={msg!r}"
            )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_next_prompt_clean_after_unclosed_tool_call(patched_mlx):
    """
    End-to-end of bug (C): if a turn ends with an unclosed
    ``<tool_call>…`` fragment (from barge-in, ``max_tokens`` cutoff, or
    a malformed model output), the NEXT model call must not render that
    fragment into its prompt. Poisoning the context makes Qwen try to
    complete the half-call instead of answering the new user turn.

    This test ends turn 1 by letting the scripted generation finish
    naturally with an unclosed tool call — no ``request_barge_in`` is
    needed to reproduce the hazard, and using the natural path avoids
    leaving ``_interrupt`` latched into turn 2.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        patched_mlx.chunk_delay = 0.0
        patched_mlx.queue([
            "<tool_call>",
            '{"name":"say","arguments":{"text":"poisoned-fragment',
        ])
        patched_mlx.queue(["ok."])

        await _drive_turn(node, "turn 1")

        patched_mlx.calls.clear()
        await _drive_turn(node, "turn 2")
        assert len(patched_mlx.calls) >= 1
        next_prompt = patched_mlx.calls[-1][0]

        assert "poisoned-fragment" not in next_prompt, (
            "BUG: mid-tool-call fragment from turn 1 leaked into turn "
            f"2's prompt: {next_prompt!r}"
        )
        opens = next_prompt.count("<tool_call>")
        closes = next_prompt.count("</tool_call>")
        assert opens == closes, (
            f"BUG: unbalanced tool_call tags in next prompt "
            f"(opens={opens}, closes={closes}): {next_prompt!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_duplicate_only_pass_terminates_turn_early(patched_mlx):
    """
    When the model re-emits the same ``say`` in subsequent passes (very
    common for simple Q&A — Qwen has nothing new to add after its first
    tool call), the turn must terminate instead of running the full
    ``max_tool_passes`` budget. Caught in production: 4 passes of
    duplicates burned ~40 s of inference and blew past the 30 s
    scheduler timeout, killing the LLM for subsequent turns.
    """
    node = _make_node(
        enable_say_tool=True, enable_show_tool=False, max_tool_passes=4
    )
    await node.initialize()
    try:
        # Pass 1: legitimate say. Passes 2-4 (if we got there): same say
        # re-emitted. The fix should break out after pass 2 detects the
        # duplicate, so passes 3 and 4 never run.
        say_body = (
            '<tool_call>{"name":"say","arguments":'
            '{"text":"Yes, I am here."}}</tool_call>'
        )
        patched_mlx.queue([say_body])  # pass 1
        patched_mlx.queue([say_body])  # pass 2 — duplicate, should stop here
        patched_mlx.queue([say_body])  # pass 3 (should never run)
        patched_mlx.queue([say_body])  # pass 4 (should never run)

        await _drive_turn(node, "Hi, is this thing on?")

        # With the fix: at most 2 model calls (pass 1 + duplicate pass 2).
        # Without the fix: 4 model calls (full budget).
        assert len(patched_mlx.calls) <= 2, (
            f"BUG: duplicate passes not short-circuited — turn ran "
            f"{len(patched_mlx.calls)} model calls (budget was 4). "
            "This is what drives the 30 s scheduler timeout in prod."
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_pre_turn_barge_in_is_cleared_and_next_turn_runs(patched_mlx):
    """
    Pipeline semantics: the VAD publishes ``barge_in`` on every user
    ``speech_start``, including the very first utterance of a session
    when nothing is in flight to cancel. The user's transcribed speech
    then arrives here as the new turn's input. By the time we're
    invoked, the barge-in's intent (cancel the previous response) has
    already been satisfied — we must NOT kill this fresh turn,
    otherwise the user never hears a reply to their first question.

    Caught in production: holding the flag across process() entry
    silently cancelled the first real turn on every session with the
    log line ``barge-in latched before pass 1 — halting``. The
    entry-reset behavior is correct for this architecture; this test
    pins it down so a future well-meaning refactor can't regress it.
    """
    node = _make_node(enable_say_tool=False)
    await node.initialize()
    try:
        patched_mlx.queue(["hello back."])
        # Simulate the VAD firing barge_in the moment the user started
        # talking — BEFORE the transcription reaches the LLM as input.
        node.request_barge_in()
        assert node._interrupt is True

        outs = await _drive_turn(node, "Hello, can you hear me?")
        tts = _texts_on_channel(outs, channel="tts")

        assert patched_mlx.calls, (
            "stale barge-in prevented the first real turn from running"
        )
        assert tts == ["hello back.", "<|text_end|>"], (
            f"expected normal response despite pre-turn barge-in: {tts!r}"
        )
        assert node._interrupt is False, "latch should be cleared"
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_mid_turn_barge_in_still_halts(patched_mlx):
    """
    Companion guarantee: clearing the latch on entry must not break
    mid-turn barge-in. The user talking OVER an active response
    should still halt generation promptly — that path is covered by
    ``test_barge_in_halts_and_no_stale_leak_to_next_turn`` above; this
    test is a regression guard for the specific change to the entry
    reset, ensuring the in-loop interrupt checks still fire.
    """
    node = _make_node(enable_say_tool=False)
    await node.initialize()
    try:
        long_reply = [f"tok{i:02d} " for i in range(30)]
        patched_mlx.queue(long_reply)

        gen = node.process(RuntimeData.text("talk at length"))
        collected: List[Any] = []
        for _ in range(3):
            collected.append(await gen.__anext__())
        node.request_barge_in()
        async for out in gen:
            collected.append(out)

        tts = _texts_on_channel(collected, channel="tts")
        body = "".join(tts[:-1])
        assert tts[-1] == "<|text_end|>"
        assert body.startswith("tok00 tok01 tok02"), body
        # Late tokens must not leak.
        for stale in long_reply[20:]:
            assert stale not in body, (
                f"mid-turn barge-in regressed — stale {stale!r} leaked: {body!r}"
            )
    finally:
        await node.cleanup()


# ────── streaming tool-call args (first-audio latency) ──────────────
#
# Qwen emits every tool call as a multi-token wrapper ending in
# `</tool_call>`. Waiting for that close tag before dispatching adds
# seconds of delay to the first spoken word on a 9B MLX model with a
# long system prompt. The parser now streams the ``"text":"..."``
# argument character-by-character as the model produces it, so a
# downstream TextCollectorNode can flush complete sentences to the TTS
# as soon as they're seen.


def _split_into_chunks(s: str, chunk_size: int = 4) -> List[str]:
    return [s[i:i + chunk_size] for i in range(0, len(s), chunk_size)]


@pytest.mark.asyncio
async def test_say_arg_is_streamed_chunk_by_chunk(patched_mlx):
    """
    The ``text`` argument of ``say(...)`` must arrive in the output
    stream as multiple ``RuntimeData.text`` chunks, interleaved with
    the model's token timing — not buffered until ``</tool_call>``.
    A sentence-boundary newline is appended after the closing ``"`` so
    the downstream ``TextCollectorNode`` flushes the utterance.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        # Split the tool_call across many small chunks — the shape
        # the real mlx-lm stream_generate produces.
        full = (
            '<tool_call>{"name":"say","arguments":{"text":"'
            'Yes, I am here. How can I help you today?'
            '"}}</tool_call>'
        )
        patched_mlx.queue(_split_into_chunks(full, chunk_size=4))

        outs = await _drive_turn(node, "hi")
        tts = _texts_on_channel(outs, channel="tts")

        # Multiple chunks — not one big blob.
        body_chunks = [t for t in tts if t != "<|text_end|>"]
        assert len(body_chunks) >= 3, (
            f"say arg should stream in multiple chunks; got {body_chunks!r}"
        )

        # Concatenation matches the argument value (plus the flush \n).
        joined = "".join(body_chunks)
        assert joined == (
            "Yes, I am here. How can I help you today?\n"
        ), f"streamed chunks don't concatenate to the arg value: {joined!r}"

        # Tool-call tags never leak on the tts channel.
        assert "<tool_call>" not in joined
        assert '"name"' not in joined
        assert "</tool_call>" not in joined
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_first_audio_text_arrives_before_tool_call_closes(patched_mlx):
    """
    The whole point of streaming: the first chars of the ``text`` arg
    reach the consumer BEFORE the model emits the ``</tool_call>``
    closing tag. A downstream TTS can start synthesising immediately.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        # Script: build the tool_call up through the arg value, THEN
        # have a visible gap before the closing tags. If the consumer
        # only yielded at ``</tool_call>``, the first tts output
        # wouldn't appear until the final chunk.
        patched_mlx.queue([
            '<tool_call>{"name":"say","arguments":{"text":"',
            "First words. ",
            "Second words.",
            '"}}',
            "</tool_call>",
        ])

        gen = node.process(RuntimeData.text("stream for me"))
        # Pull items one at a time, tracking when the first tts chunk
        # lands vs when the sentinel shows up.
        seen_text_before_close = False
        saw_final_newline = False
        async for out in gen:
            if not isinstance(out, RuntimeData) or not out.is_text():
                continue
            if getattr(out.metadata, "channel", "tts") != "tts":
                continue
            text = out.as_text()
            if text == "<|text_end|>":
                break
            if "First words" in text:
                seen_text_before_close = True
            if text == "\n":
                saw_final_newline = True

        assert seen_text_before_close, (
            "first words of say(text=...) should stream out before the "
            "closing </tool_call> arrives"
        )
        assert saw_final_newline, (
            "a flush newline must be appended after the arg closes so "
            "the downstream TextCollector emits the complete sentence"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_streaming_decodes_json_escapes(patched_mlx):
    """
    JSON string escapes inside the tool arg must be decoded during
    streaming — otherwise the TTS would speak raw ``\\n`` sequences.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        body = (
            r'<tool_call>{"name":"say","arguments":{"text":"'
            r'line1\nline2 and a \"quoted\" word and a backslash\\here'
            r'"}}</tool_call>'
        )
        patched_mlx.queue(_split_into_chunks(body, chunk_size=3))

        outs = await _drive_turn(node, "test escapes")
        tts = "".join(
            t for t in _texts_on_channel(outs, channel="tts")
            if t != "<|text_end|>"
        ).rstrip("\n")

        assert tts == (
            'line1\nline2 and a "quoted" word and a backslash\\here'
        ), f"escape decoding wrong: {tts!r}"
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_show_arg_streams_on_ui_channel(patched_mlx):
    """
    ``show(content=...)`` streams on the ``ui`` channel so a display
    sink can render markdown incrementally; spoken content is never
    duplicated there.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=True)
    await node.initialize()
    try:
        body = (
            '<tool_call>{"name":"show","arguments":{"content":"'
            '```python\\nprint(1)\\n```'
            '"}}</tool_call>'
        )
        patched_mlx.queue(_split_into_chunks(body, chunk_size=5))
        patched_mlx.queue(["done."])

        outs = await _drive_turn(node, "show me code")
        ui = "".join(
            t for t in _texts_on_channel(outs, channel="ui")
            if t != "<|text_end|>"
        )
        tts = "".join(
            t for t in _texts_on_channel(outs, channel="tts")
            if t != "<|text_end|>"
        ).rstrip("\n")

        assert "```python\nprint(1)\n```" in ui, ui
        # Spoken content from this tool call must not leak to tts
        # (there's only a `show` in this body, nothing to speak).
        assert "python" not in tts and "print(1)" not in tts, (
            f"show content leaked to tts channel: {tts!r}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_mid_arg_barge_in_halts_streaming(patched_mlx):
    """
    Barge-in fired mid-argument must stop emitting further chars to
    TTS. Any chunks already pushed before the latch are allowed; none
    after.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False)
    await node.initialize()
    try:
        # Tiny chunks so the barge-in latch has a chance to observe.
        patched_mlx.chunk_delay = 0.003
        first_part = "Early words. "
        late_part = "LATE_STALE_NEVER_SPEAK. " * 30
        body = (
            '<tool_call>{"name":"say","arguments":{"text":"'
            + first_part + late_part +
            '"}}</tool_call>'
        )
        patched_mlx.queue(_split_into_chunks(body, chunk_size=4))

        gen = node.process(RuntimeData.text("start"))
        collected: List[Any] = []
        accumulated_tts = ""
        barged = False
        async for out in gen:
            collected.append(out)
            if (
                isinstance(out, RuntimeData)
                and out.is_text()
                and getattr(out.metadata, "channel", "tts") == "tts"
            ):
                accumulated_tts += out.as_text()
                if not barged and "Early words." in accumulated_tts:
                    node.request_barge_in()
                    barged = True
            # Safety break so the test doesn't hang.
            if len(collected) > 500:
                break

        assert barged, "barge-in never fired — chunks too small?"
        joined = "".join(_texts_on_channel(collected, channel="tts"))
        assert "Early words" in joined, (
            f"early chunks before barge-in missing: {joined!r}"
        )
        stale_count = joined.count("LATE_STALE_NEVER_SPEAK")
        # Allow a tiny handful (≤2) in-flight chunks between the latch
        # being set and the consumer's next interrupt check, but the
        # vast majority of the 30 stale repetitions must be dropped.
        assert stale_count < 5, (
            f"BUG: post-barge-in arg chars leaked to tts "
            f"({stale_count}/30 stale repetitions got through): "
            f"{joined[:300]!r}..."
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_second_pass_does_not_stream_to_dedup_safely(patched_mlx):
    """
    Streaming is enabled ONLY on the first pass. If pass 2 starts with
    a tool call that turns out to be a duplicate, streaming the arg
    would double-speak before dedup could fire. The parser for pass 2+
    falls back to buffer-and-dispatch so the dedupe check has the full
    signature before any audio leaks.
    """
    node = _make_node(enable_say_tool=True, enable_show_tool=False,
                      max_tool_passes=2)
    await node.initialize()
    try:
        # Pass 1 emits a full say.
        patched_mlx.queue([
            '<tool_call>{"name":"say","arguments":'
            '{"text":"Hello, Mathieu."}}</tool_call>',
        ])
        # Pass 2 re-emits the EXACT same body. Dedup should catch it.
        # If streaming were enabled on pass 2, the arg chars would hit
        # the tts channel before dedup fired, doubling the audio.
        patched_mlx.queue([
            '<tool_call>{"name":"say","arguments":'
            '{"text":"Hello, Mathieu."}}</tool_call>',
        ])

        outs = await _drive_turn(node, "greet me")
        tts = "".join(
            t for t in _texts_on_channel(outs, channel="tts")
            if t != "<|text_end|>"
        )

        # The spoken content should appear exactly once.
        count = tts.count("Hello, Mathieu.")
        assert count == 1, (
            f"BUG: pass-2 streaming double-spoke the same say. "
            f"tts={tts!r} occurrences={count}"
        )
    finally:
        await node.cleanup()


@pytest.mark.asyncio
async def test_interleaved_envelopes_and_turns_preserve_order(patched_mlx):
    """
    Interleave aux envelopes with turns in the same input path the
    pipeline uses. The model must see turns in order; envelopes take
    effect exactly where delivered (not retroactively or laggedly).
    """
    node = _make_node()
    await node.initialize()
    try:
        patched_mlx.queue(["first."])
        patched_mlx.queue(["second."])
        patched_mlx.queue(["third."])

        await _drive_turn(node, "turn A")
        await _drive_envelope(
            node, "system_prompt", {"text": "Be terse."}
        )
        await _drive_turn(node, "turn B")
        await _drive_envelope(node, "reset")
        await _drive_turn(node, "turn C")

        # Three model calls — one per real turn, zero for envelopes.
        assert len(patched_mlx.calls) == 3, (
            f"expected 3 model calls, got {len(patched_mlx.calls)}"
        )

        prompt_a, prompt_b, prompt_c = (c[0] for c in patched_mlx.calls)

        # turn A ran under the default system prompt.
        assert "Be terse" not in prompt_a
        assert "turn A" in prompt_a

        # turn B ran under the new persona and still sees turn A in history.
        assert "Be terse" in prompt_b
        assert "turn A" in prompt_b
        assert "turn B" in prompt_b

        # turn C ran AFTER reset — must NOT see A or B, but the persona
        # set before reset persists (reset clears history, not prompt).
        assert "Be terse" in prompt_c
        assert "turn A" not in prompt_c, (
            f"reset failed to drop turn A from model prompt: {prompt_c!r}"
        )
        assert "turn B" not in prompt_c, (
            f"reset failed to drop turn B from model prompt: {prompt_c!r}"
        )
        assert "turn C" in prompt_c
    finally:
        await node.cleanup()
