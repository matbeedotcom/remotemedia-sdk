"""
True end-to-end test: Rust gRPC server + LFM2 multiprocess-Python node +
Session Control Bus from the Python client.

This is the production DX proof point — the same node class that
``test_lfm2_multi_turn_control.py`` tests in-process now runs inside a
Python subprocess that the Rust server spawns via the multiprocess
executor. The Python client talks to the server ONLY through the
Session Control Bus (``PipelineControl.Attach``); every user turn is a
``publish("lfm.in", ...)`` and every context injection is a
``publish("lfm.in.context", ...)``. Replies come back through a tap on
``lfm.out``.

Mapping to the DX doc:

    node.set_context(docs)   ≡   ctrl.publish("lfm.in.context",
                                             Data.from_text(docs))
    node.process(turn)       ≡   ctrl.publish("lfm.in",
                                             Data.from_text(turn))
    reply                    ≡   next message on tap("lfm.out")

## Running

- Requires ``transformers>=4.54``, ``torch``, and ~700 MB of model
  weights for ``LiquidAI/LFM2-350M``.
- Requires the ``remotemedia`` Python package importable by the
  spawned multiprocess subprocess (``PYTHONPATH`` set below).
- First invocation compiles the ``control_bus_test_server`` Rust
  binary (~30 s) and fetches the LFM2 model (~10–30 s) — total cold
  run can easily be 90 s.

Gated by ``REMOTEMEDIA_RUN_LFM2_TESTS=1`` (same flag as the
in-process test).
"""

from __future__ import annotations

import asyncio
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional, Tuple

import pytest

from remotemedia.control import Data, attach

REPO_ROOT = Path(__file__).resolve().parents[3]
CLIENTS_PYTHON = REPO_ROOT / "clients" / "python"

pytestmark = pytest.mark.skipif(
    os.environ.get("REMOTEMEDIA_RUN_LFM2_TESTS") != "1",
    reason="Set REMOTEMEDIA_RUN_LFM2_TESTS=1 to run the LFM2 gRPC e2e test",
)


async def _spawn_server() -> Tuple[subprocess.Popen, str, str]:
    """
    Launch the Rust test-server in LFM2 mode and wait for READY.
    """
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "-p",
        "remotemedia-grpc",
        "--example",
        "control_bus_test_server",
    ]
    env = dict(os.environ)
    env["TEST_SESSION_KIND"] = "lfm2"
    env.setdefault("RUST_LOG", "info")
    # Make sure the Python subprocess that the Rust multiprocess executor
    # spawns can import `remotemedia`.
    existing_pp = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{CLIENTS_PYTHON}:{existing_pp}" if existing_pp else str(CLIENTS_PYTHON)
    )

    # Capture server stderr to a file so post-mortem debugging works.
    log_path = Path("/tmp") / f"lfm2_server_{os.getpid()}.log"
    log_fh = open(log_path, "w")
    sys.stderr.write(f"[server] stderr -> {log_path}\n")

    proc = subprocess.Popen(
        cmd,
        cwd=str(REPO_ROOT),
        stdout=subprocess.PIPE,
        stderr=log_fh,
        env=env,
        text=True,
        bufsize=1,
    )

    loop = asyncio.get_running_loop()

    async def _read_ready() -> str:
        while True:
            assert proc.stdout is not None
            line = await loop.run_in_executor(None, proc.stdout.readline)
            if not line:
                raise RuntimeError(
                    f"server exited before READY; see {log_path}"
                )
            line = line.strip()
            if line.startswith("READY "):
                return line
            sys.stderr.write(f"[server] {line}\n")

    # LFM2 mode takes longer: session creation triggers model load in the
    # spawned Python subprocess.
    try:
        ready = await asyncio.wait_for(_read_ready(), timeout=300.0)
    except asyncio.TimeoutError:
        proc.terminate()
        raise RuntimeError(f"server didn't print READY within 300s; see {log_path}")

    parts = ready.split()
    return proc, f"127.0.0.1:{parts[1]}", parts[2]


def _kill(proc: Optional[subprocess.Popen]) -> None:
    if proc is None:
        return
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


@pytest.fixture(scope="module")
def lfm2_server():
    proc: Optional[subprocess.Popen] = None
    address: Optional[str] = None
    session_id: Optional[str] = None

    async def _start():
        nonlocal proc, address, session_id
        proc, address, session_id = await _spawn_server()

    asyncio.run(_start())
    assert proc and address and session_id
    yield (address, session_id)
    _kill(proc)


async def _next_tap_text(tap) -> str:
    """
    Wait for the next tap event and return its text body.

    Budget is dominated by LFM2 generation on CPU (~5–15 s for 80 tokens
    on the 350M model). The per-hop IPC path itself is sub-millisecond
    once the `EndOfInput` sentinel from multiprocess nodes landed —
    earlier versions fell back to a 30 s scheduler timeout on every
    aux-port publish because Rust had no way to tell the node was done.
    """
    event = await asyncio.wait_for(tap.__anext__(), timeout=60.0)
    if event.kind == "text":
        return event.text_value
    return event.text.strip()


@pytest.mark.asyncio
async def test_lfm2_via_control_bus_reflects_injected_context(lfm2_server):
    """
    Full production path. The LFM2 node lives inside a Python subprocess
    the Rust server spawned. The Python test talks to it only through
    ``SessionControl``.
    """
    address, session_id = lfm2_server

    async with attach(address, session_id=session_id) as ctrl:
        tap = await ctrl.subscribe("lfm.out")

        # Give the subscription a moment to register on the server.
        await asyncio.sleep(0.2)

        # ─ Turn 1: baseline, no context injected yet ─
        await ctrl.publish("lfm.in", Data.from_text("What is my favorite color?"))
        t1 = (await _next_tap_text(tap)).lower()
        assert "cerulean" not in t1, (
            f"baseline reply unexpectedly mentions cerulean: {t1!r}"
        )

        # ─ Inject context via the aux port, and reset the conversation
        #   history so the model isn't biased by turn 1's "I don't know". ─
        await ctrl.publish(
            "lfm.in.context",
            Data.from_text("The user's favorite color is cerulean."),
        )
        await ctrl.publish("lfm.in.reset", Data.from_text(""))
        await asyncio.sleep(0.2)

        # ─ Turn 2: the system prompt now carries the cerulean fact. ─
        await ctrl.publish("lfm.in", Data.from_text("What is my favorite color?"))
        t2 = (await _next_tap_text(tap)).lower()
        assert "cerulean" in t2, (
            f"turn 2 did not reflect injected context: {t2!r}"
        )
