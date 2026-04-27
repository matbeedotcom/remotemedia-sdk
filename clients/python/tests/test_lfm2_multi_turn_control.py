"""
Multi-turn LFM2 conversation with dynamic context injection — production path.

Exercises the **full user-facing API surface**, not direct node method calls:

  - ``Pipeline(nodes=[...])``  /  ``Pipeline.from_definition(...)``
  - ``pipeline.managed_execution()`` context manager (init + cleanup)
  - ``pipeline.process(stream)`` async streaming entry point
  - ``pipeline.get_node(name)`` to reach the live node from a Pipeline
    consumer (in-process analog of the Session Control Bus)

Between turns of a live pipeline, the test flips the LFM2 node's
auxiliary context and verifies that the next model reply reflects the
injected facts. This proves the DX claim the Session Control Bus exists
to deliver — once the gRPC transport lands, the same assertions will
hold against a remote client. The only change will be at the call site:

    pl.get_node("lfm").set_context(doc)      # today (in-process)
         ⇅
    ctrl.publish("llm.in.context", doc)      # gRPC path (forthcoming)

## Running

Requires ``transformers >= 4.54`` + a matching torch / torchaudio /
torchvision in the active Python environment. Upgrade via uv:

    uv pip install --python "$(which python)" -U \\
        'transformers>=4.54.0' 'torch>=2.1' 'torchaudio' 'torchvision' \\
        'accelerate>=0.33'

Then:

    REMOTEMEDIA_RUN_LFM2_TESTS=1 pytest \\
        tests/test_lfm2_multi_turn_control.py -v

Verified on transformers 5.5.4 + torch 2.11 + LFM2-350M (CPU).

### Relationship to the gRPC control-bus tests

This file tests the **LLM's** response to dynamic context injection in
the same process. A separate test,
``clients/python/tests/test_control_bus_grpc.py``, proves the same
injection semantics travel end-to-end over the gRPC control-plane RPC
(``PipelineControl.Attach`` → ``publish`` on ``{node}.in.{port}``).

The two combine cleanly once an LFM2 pipeline can be hosted inside a
Rust session that launches ``LFM2TextNode`` via the multiprocess
Python executor. ``LFM2TextNode`` is already decorated with
``@python_requires`` (see ``docs/MANAGED_PYTHON_ENVIRONMENTS.md``), so
when a manifest is submitted to the Rust server with ``executor: multiprocess``
on the LFM2 node, the managed-venv system provisions the right
transformers/torch deps automatically. At that point the ``set_context``
calls in this file become:

    await ctrl.publish("lfm.in.context", Data.from_text(docs))

against the same running session. No other change to the test.
"""

from __future__ import annotations

import asyncio
import os
from typing import Any, Callable, List, Optional, Tuple

import pytest
import yaml

from remotemedia.core.pipeline import Pipeline


def _reply_text(reply: Any) -> str:
    """
    Normalize a reply to a string.

    LFM2TextNode now returns a ``RuntimeData`` (to support the
    multiprocess IPC path); in-process tests want plain strings for
    substring assertions.
    """
    if isinstance(reply, str):
        return reply
    # Duck-type RuntimeData.as_text() to avoid importing it here.
    if hasattr(reply, "as_text"):
        try:
            return reply.as_text()
        except Exception:
            pass
    return str(reply)

pytestmark = pytest.mark.skipif(
    os.environ.get("REMOTEMEDIA_RUN_LFM2_TESTS") != "1",
    reason="Set REMOTEMEDIA_RUN_LFM2_TESTS=1 to run the LFM2 multi-turn test",
)


# A unique sentinel the test uses to terminate the input stream — this is
# the test's own generator protocol, not the pipeline's internal _SENTINEL.
_END_OF_INPUT = object()


def _ensure_lfm2_available_or_skip() -> None:
    """Skip the test cleanly when the environment can't load LFM2."""
    try:
        from remotemedia.nodes.ml.lfm2_text import _ML_DEPS_AVAILABLE
    except ImportError as e:  # pragma: no cover - env-specific
        pytest.skip(f"LFM2TextNode unavailable: {e}")
    if not _ML_DEPS_AVAILABLE:
        pytest.skip("torch / transformers not installed")

    # Attempt a config fetch — fails cleanly on transformers < 4.54 or no net.
    try:
        from transformers import AutoConfig
        AutoConfig.from_pretrained("LiquidAI/LFM2-350M", trust_remote_code=True)
    except Exception as e:  # noqa: BLE001
        msg = str(e)
        if "model type" in msg.lower() and "lfm2" in msg.lower():
            pytest.skip(f"transformers does not recognize LFM2 — upgrade needed: {e}")
        pytest.skip(f"LFM2 config not reachable: {e}")


async def _drive_conversation(
    pipeline: Pipeline,
    turns: List[Tuple[str, Optional[Callable[[], None]]]],
    *,
    label: str = "",
    verbose: bool = True,
) -> List[Any]:
    """
    Run a multi-turn conversation through ``pipeline.process(stream)``.

    For each ``(user_turn, after_reply)`` pair:
      1. Push ``user_turn`` onto the input stream.
      2. Await exactly one reply from the pipeline output.
      3. If ``after_reply`` is not None, call it (synchronous) before
         the next turn is pushed.

    When ``verbose`` is True (default), prints every turn's input and reply
    as they stream. Pytest captures stdout by default — run with ``-s`` to
    see the transcript:

        pytest tests/test_lfm2_multi_turn_control.py -v -s
    """
    input_queue: asyncio.Queue[Any] = asyncio.Queue()

    async def stream():
        while True:
            item = await input_queue.get()
            if item is _END_OF_INPUT:
                return
            yield item

    output_gen = pipeline.process(stream())
    replies: List[Any] = []

    if verbose:
        print(f"\n=== {label or pipeline.name} ===", flush=True)

    try:
        for turn_idx, (user_turn, after_reply) in enumerate(turns, 1):
            if verbose:
                print(f"\n[turn {turn_idx}] USER: {user_turn}", flush=True)
            await input_queue.put(user_turn)
            # Each LFM2 turn produces exactly one reply — await it.
            reply = await output_gen.__anext__()
            replies.append(reply)
            if verbose:
                print(f"[turn {turn_idx}] ASSISTANT: {reply}", flush=True)
            if after_reply is not None:
                if verbose:
                    print(f"[turn {turn_idx}] -> running post-reply action", flush=True)
                after_reply()
    finally:
        # Close the input stream so the pipeline workers drain cleanly.
        await input_queue.put(_END_OF_INPUT)
        # Exhaust any trailing outputs (normally none for a chat node).
        try:
            async for _ in output_gen:
                pass
        except StopAsyncIteration:  # pragma: no cover
            pass

        if verbose:
            print(f"=== end {label or pipeline.name} ===\n", flush=True)

    return replies


def _build_lfm2_node():
    """Construct an LFM2TextNode with deterministic generation settings."""
    from remotemedia.nodes.ml.lfm2_text import LFM2TextNode
    return LFM2TextNode(
        name="lfm",
        hf_repo="LiquidAI/LFM2-350M",
        max_new_tokens=80,
        do_sample=False,
    )


@pytest.mark.asyncio
async def test_pipeline_driven_multi_turn_with_context_injection():
    """
    The production DX path: build a Pipeline, drive turns via
    pipeline.process(stream), inject context between turns by reaching
    the live node via pipeline.get_node(...).
    """
    _ensure_lfm2_available_or_skip()

    pipeline = Pipeline(nodes=[_build_lfm2_node()], name="lfm2-assistant")

    async with pipeline.managed_execution() as pl:
        # get_node is the Pipeline-level primitive that maps to what
        # `SessionControl` does for a router-hosted session:
        # locate a node by id and operate on its control surface.
        lfm = pl.get_node("lfm")
        assert lfm is not None, "get_node must resolve the LFM2 node by name"

        def inject_color():
            lfm.set_context("The user's favorite color is cerulean.")

        def replace_with_animal():
            lfm.set_context("The user's favorite animal is an axolotl.")

        def clear_and_reset():
            lfm.clear_context()
            lfm.reset_history()

        # (turn, action-after-reply). Action runs before the NEXT turn.
        turns = [
            ("What is my favorite color?",  inject_color),       # baseline
            ("What is my favorite color?",  replace_with_animal), # sees cerulean
            ("What is my favorite animal?", clear_and_reset),     # sees axolotl
            ("What is my favorite color?",  None),                # post-clear
        ]

        replies = await _drive_conversation(
            pl, turns, label="context injection (cerulean / axolotl)"
        )

    assert len(replies) == 4
    t1, t2, t3, t4 = [_reply_text(r).lower() for r in replies]

    assert "cerulean" not in t1, (
        f"baseline (no context yet) unexpectedly contains injected fact: {t1!r}"
    )
    assert "cerulean" in t2, (
        f"after set_context('cerulean'), reply must reflect it: {t2!r}"
    )
    assert "axolotl" in t3, (
        f"after set_context('axolotl'), reply must reflect it: {t3!r}"
    )
    assert "cerulean" not in t4 and "axolotl" not in t4, (
        f"after clear_context()+reset_history(), stale facts leaked: {t4!r}"
    )


@pytest.mark.asyncio
async def test_manifest_definition_round_trip_drives_same_behavior():
    """
    Load the pipeline from a YAML manifest (via ``Pipeline.from_definition``)
    and verify the same context-injection DX works end-to-end.

    This is the shape a production user ships: a YAML file checked into the
    repo, loaded at runtime, executed through ``managed_execution()``.
    """
    _ensure_lfm2_available_or_skip()

    # A real pipeline YAML. Pipeline.from_definition consumes the same
    # shape that pipeline.export_definition() produces, so any YAML the
    # user hand-writes round-trips through the same loader.
    manifest_yaml = """
    name: lfm2-assistant
    nodes:
      - node_id: lfm_0
        node_type: LFM2TextNode
        module: remotemedia.nodes.ml.lfm2_text
        class_name: LFM2TextNode
        is_streaming: false
        config:
          name: lfm
          hf_repo: LiquidAI/LFM2-350M
          max_new_tokens: 80
          do_sample: false
    connections: []
    """
    definition = yaml.safe_load(manifest_yaml)

    pipeline = await Pipeline.from_definition(definition)
    assert pipeline.node_count == 1
    assert pipeline.get_node("lfm") is not None, (
        "manifest round-trip must preserve the node's addressable name"
    )

    async with pipeline.managed_execution() as pl:
        lfm = pl.get_node("lfm")

        def inject_color():
            lfm.set_context("The user's favorite color is cerulean.")

        replies = await _drive_conversation(
            pl,
            [
                ("What is my favorite color?", inject_color),
                ("What is my favorite color?", None),
            ],
            label="YAML-loaded pipeline, context injection",
        )

    t1, t2 = [_reply_text(r).lower() for r in replies]
    assert "cerulean" not in t1, f"baseline unexpectedly contains fact: {t1!r}"
    assert "cerulean" in t2, f"after injection, reply must reflect fact: {t2!r}"


@pytest.mark.asyncio
async def test_pipeline_driven_system_prompt_swap():
    """
    Swapping the system prompt mid-session, driven through the full
    pipeline streaming path.
    """
    _ensure_lfm2_available_or_skip()

    pipeline = Pipeline(nodes=[_build_lfm2_node()], name="lfm2-persona")

    async with pipeline.managed_execution() as pl:
        lfm = pl.get_node("lfm")

        # First persona: one-word answers.
        lfm.set_system_prompt(
            "You only respond with single-word answers. Never use punctuation."
        )

        def swap_to_pirate():
            lfm.set_system_prompt(
                "You are a pirate. Respond in pirate voice and always say 'Arrr!'."
            )
            lfm.reset_history()

        replies = await _drive_conversation(
            pl,
            [
                ("What color is the sky on a clear day?", swap_to_pirate),
                ("Hello there, how are you?",             None),
            ],
            label="persona swap (one-word answers -> pirate)",
        )

    _, second = [_reply_text(r).lower() for r in replies]
    assert "arr" in second, (
        f"system-prompt swap via pipeline did not take effect: {second!r}"
    )
