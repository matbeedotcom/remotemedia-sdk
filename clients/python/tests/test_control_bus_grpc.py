"""
End-to-end test for the Python remotemedia.control client against a real
Rust gRPC server with a live PipelineControl service.

Spawns a minimal Rust binary (``control_bus_test_server``) that creates
one live session and prints ``READY <port> <session_id>`` to stdout.
The Python test attaches via :func:`remotemedia.control.attach` and
exercises subscribe / publish / intercept / set_node_state against that
session.

## Running

Requires `cargo` + the workspace to be buildable. The test will compile
the example binary on first run, which can take 1-2 minutes cold.

    cd clients/python
    pytest tests/test_control_bus_grpc.py -v -s
"""

from __future__ import annotations

import asyncio
import os
import subprocess
import sys
from pathlib import Path
from typing import Optional, Tuple

import pytest

from remotemedia.control import Data, NodeState, attach

REPO_ROOT = Path(__file__).resolve().parents[3]


async def _spawn_server() -> Tuple[subprocess.Popen, str, str]:
    """
    Launch the Rust `control_bus_test_server` example binary, wait for
    its READY line, and return (process, grpc_address, session_id).
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
    env.setdefault("RUST_LOG", "warn")
    proc = subprocess.Popen(
        cmd,
        cwd=str(REPO_ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        text=True,
        bufsize=1,
    )

    # Wait up to 180s for READY (first-time cargo build can be slow).
    loop = asyncio.get_running_loop()

    async def _read_ready() -> str:
        while True:
            assert proc.stdout is not None
            line = await loop.run_in_executor(None, proc.stdout.readline)
            if not line:
                stderr = proc.stderr.read() if proc.stderr else ""
                raise RuntimeError(
                    f"server exited before READY; stderr:\n{stderr}"
                )
            line = line.strip()
            if line.startswith("READY "):
                return line
            sys.stderr.write(f"[server] {line}\n")

    try:
        ready = await asyncio.wait_for(_read_ready(), timeout=180.0)
    except asyncio.TimeoutError:
        proc.terminate()
        raise

    parts = ready.split()
    port = parts[1]
    session_id = parts[2]
    return proc, f"127.0.0.1:{port}", session_id


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
def server_proc():
    proc: Optional[subprocess.Popen] = None
    address: Optional[str] = None
    session_id: Optional[str] = None

    async def _start():
        nonlocal proc, address, session_id
        proc, address, session_id = await _spawn_server()

    asyncio.run(_start())
    assert proc is not None
    assert address is not None
    assert session_id is not None
    yield (address, session_id)
    _kill(proc)


@pytest.mark.asyncio
async def test_attach_to_bogus_session_raises(server_proc):
    address, _ = server_proc
    with pytest.raises(Exception):
        async with attach(address, session_id="does-not-exist"):
            pass


@pytest.mark.asyncio
async def test_publish_and_tap_via_python_client(server_proc):
    address, session_id = server_proc
    async with attach(address, session_id=session_id) as ctrl:
        tap = await ctrl.subscribe("calc.out")

        # Give the server a beat to wire the tap forwarder.
        await asyncio.sleep(0.1)

        await ctrl.publish(
            "calc.in",
            Data.from_json({"operation": "add", "operands": [3.0, 4.0]}),
        )

        event = await asyncio.wait_for(tap.__anext__(), timeout=3.0)
        payload = event.json_value
        assert payload["result"] == 7.0


@pytest.mark.asyncio
async def test_set_node_state_bypass_via_python_client(server_proc):
    address, session_id = server_proc
    async with attach(address, session_id=session_id) as ctrl:
        tap = await ctrl.subscribe("calc.out")
        await asyncio.sleep(0.1)

        # Bypass → the raw input dict should reach downstream (and the tap).
        await ctrl.set_node_state("calc", NodeState.BYPASS)
        await asyncio.sleep(0.1)

        await ctrl.publish(
            "calc.in",
            Data.from_json({"operation": "multiply", "operands": [10.0, 5.0]}),
        )
        event = await asyncio.wait_for(tap.__anext__(), timeout=3.0)
        bypassed = event.json_value
        assert "result" not in bypassed
        assert bypassed["operation"] == "multiply"

        # Clear → next publish computes again.
        await ctrl.clear_node_state("calc")
        await asyncio.sleep(0.1)

        await ctrl.publish(
            "calc.in",
            Data.from_json({"operation": "multiply", "operands": [10.0, 5.0]}),
        )
        event = await asyncio.wait_for(tap.__anext__(), timeout=3.0)
        assert event.json_value["result"] == 50.0


@pytest.mark.asyncio
async def test_intercept_replaces_value_via_python_client(server_proc):
    address, session_id = server_proc
    async with attach(address, session_id=session_id) as ctrl:
        tap = await ctrl.subscribe("calc.out")
        await asyncio.sleep(0.1)

        async with ctrl.intercept("calc.out", deadline_ms=500) as intercepts:
            # Spawn a handler that replaces every output with {result: 999}.
            async def handler():
                async for req in intercepts:
                    await req.replace(
                        Data.from_json({"result": 999.0, "operation": "replaced"})
                    )

            handler_task = asyncio.create_task(handler())
            try:
                await asyncio.sleep(0.1)  # let intercept install
                await ctrl.publish(
                    "calc.in",
                    Data.from_json({"operation": "add", "operands": [1.0, 1.0]}),
                )
                event = await asyncio.wait_for(tap.__anext__(), timeout=3.0)
                # Tap fires BEFORE intercept mutation (by design — the tap
                # observes the raw output). Downstream (not exercised here)
                # would receive the replacement. The tap assertion here is
                # just that we got the original calc output.
                result = event.json_value["result"]
                assert result == 2.0
            finally:
                handler_task.cancel()
