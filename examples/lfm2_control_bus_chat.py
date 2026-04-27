#!/usr/bin/env python3
"""
Production-style multi-turn LFM2 chat over the Session Control Bus.

This example drives an LFM2TextNode hosted inside a RemoteMedia gRPC
server's multiprocess-Python worker. The client talks to the node
entirely through the control plane:

    publish("lfm.in", turn)                  -> user turn goes to the LLM
    publish("lfm.in.context", docs)          -> RAG context for next turn
    publish("lfm.in.system_prompt", prompt)  -> persona swap
    publish("lfm.in.reset", "")              -> drop prior history
    subscribe("lfm.out")                     -> stream replies back

## Running

Zero-config (spawns the server for you):

    PYTHONPATH=clients/python python examples/lfm2_control_bus_chat.py

Or against a server you're already running:

    python examples/lfm2_control_bus_chat.py \\
        --address 127.0.0.1:<port> --session <session_id>

## Requirements

- `transformers>=4.54`, `torch>=2.1`, `accelerate>=0.33` in the Python
  environment the server's multiprocess executor spawns. See
  `docs/MANAGED_PYTHON_ENVIRONMENTS.md`.
- ~700 MB of disk for the `LiquidAI/LFM2-350M` weights the node loads on
  first use.
- A cargo-buildable checkout for auto-spawn mode (the default). The
  helper binary `control_bus_test_server` is compiled on first run.
"""

from __future__ import annotations

import argparse
import asyncio
import os
import subprocess
import sys
from contextlib import asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import AsyncIterator, List, Optional, Tuple

from remotemedia.control import Data, attach

REPO_ROOT = Path(__file__).resolve().parents[1]
CLIENTS_PYTHON = REPO_ROOT / "clients" / "python"


# ─── Reply collection ─────────────────────────────────────────────────────

async def _next_reply(tap: AsyncIterator, timeout: float) -> str:
    """Wait for the next tap event on `lfm.out` and return its text."""
    event = await asyncio.wait_for(tap.__anext__(), timeout=timeout)
    if event.kind == "text":
        return event.text_value
    # Fallback: some transports hand back pre-decoded text.
    return getattr(event, "text", "").strip()


# ─── Chat surface ─────────────────────────────────────────────────────────

@dataclass
class ChatClient:
    """High-level wrapper around the control bus for an LFM2 session.

    The `node_id` defaults to ``"lfm"`` — match the id used in your
    pipeline manifest.
    """

    ctrl: object
    tap: AsyncIterator
    node_id: str = "lfm"
    reply_timeout: float = 60.0

    async def say(self, user_turn: str) -> str:
        """Send one user turn, await exactly one reply."""
        await self.ctrl.publish(
            f"{self.node_id}.in",
            Data.from_text(user_turn),
        )
        return await _next_reply(self.tap, self.reply_timeout)

    async def set_context(self, docs: str) -> None:
        """Attach retrieval/RAG context for subsequent turns."""
        await self.ctrl.publish(
            f"{self.node_id}.in.context",
            Data.from_text(docs),
        )

    async def set_system_prompt(self, prompt: str) -> None:
        """Replace the system prompt."""
        await self.ctrl.publish(
            f"{self.node_id}.in.system_prompt",
            Data.from_text(prompt),
        )

    async def reset_history(self) -> None:
        """Drop the conversation history so the next turn starts fresh."""
        await self.ctrl.publish(
            f"{self.node_id}.in.reset",
            Data.from_text(""),
        )

    async def clear_context(self) -> None:
        """Remove any previously-injected context."""
        await self.ctrl.publish(
            f"{self.node_id}.in.context",
            Data.from_text(""),
        )


# ─── Demo scenarios ───────────────────────────────────────────────────────

async def run_demo(client: ChatClient) -> List[str]:
    """
    Exercise the full DX surface in one run:

    1. Baseline turn with no context — model shouldn't know our colour.
    2. Inject RAG context + reset history, ask again — should reflect it.
    3. Swap the system prompt to a pirate persona — next turn speaks like one.
    4. Clear context + history — model falls back to "I don't know".
    """
    replies: List[str] = []

    def log(turn: int, user: str, reply: str) -> None:
        print(f"\n[turn {turn}] USER:      {user}")
        print(f"[turn {turn}] ASSISTANT: {reply}")

    # Turn 1 — baseline.
    r1 = await client.say("What is my favorite color?")
    log(1, "What is my favorite color?", r1)
    replies.append(r1)

    # Inject RAG-style context and drop the prior "I don't know" bias.
    await client.set_context("The user's favorite color is cerulean.")
    await client.reset_history()

    # Turn 2 — system prompt now carries the injected fact.
    r2 = await client.say("What is my favorite color?")
    log(2, "What is my favorite color?", r2)
    replies.append(r2)

    # Swap persona mid-conversation.
    await client.set_system_prompt(
        "You are a pirate. Respond in pirate voice and always say 'Arrr!'."
    )
    await client.reset_history()

    # Turn 3 — pirate persona takes over.
    r3 = await client.say("Hello there, how are you?")
    log(3, "Hello there, how are you?", r3)
    replies.append(r3)

    # Wipe context + history and go back to a plain assistant.
    await client.clear_context()
    await client.set_system_prompt("You are a concise helpful assistant.")
    await client.reset_history()

    # Turn 4 — neither fact nor persona should leak.
    r4 = await client.say("What is my favorite color?")
    log(4, "What is my favorite color?", r4)
    replies.append(r4)

    return replies


# ─── Server lifecycle (auto-spawn mode) ───────────────────────────────────

def _parse_ready_line(line: str) -> Tuple[str, str]:
    """Parse a `READY <port> <session_id>` line from the helper server."""
    parts = line.strip().split()
    if len(parts) < 3 or parts[0] != "READY":
        raise ValueError(f"expected 'READY <port> <session>', got: {line!r}")
    return f"127.0.0.1:{parts[1]}", parts[2]


async def _spawn_server(boot_timeout: float) -> Tuple[subprocess.Popen, str, str]:
    """
    Launch `control_bus_test_server` in LFM2 mode and wait for READY.

    Cargo compiles it on first invocation (~30 s). LFM2 session bring-up
    additionally loads the model inside the spawned Python subprocess,
    which is where most of the boot budget goes.
    """
    cmd = [
        "cargo", "run", "--quiet",
        "-p", "remotemedia-grpc",
        "--example", "control_bus_test_server",
    ]
    env = dict(os.environ)
    env["TEST_SESSION_KIND"] = "lfm2"
    env.setdefault("RUST_LOG", "warn")
    existing_pp = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{CLIENTS_PYTHON}:{existing_pp}" if existing_pp else str(CLIENTS_PYTHON)
    )

    log_path = Path("/tmp") / f"lfm2_server_{os.getpid()}.log"
    log_fh = open(log_path, "w")
    print(f"Spawning helper server, logs -> {log_path}")

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
                raise RuntimeError(f"server exited before READY; see {log_path}")
            line = line.strip()
            if line.startswith("READY "):
                return line
            print(f"[server] {line}", file=sys.stderr)

    try:
        ready = await asyncio.wait_for(_read_ready(), timeout=boot_timeout)
    except asyncio.TimeoutError:
        proc.terminate()
        raise RuntimeError(
            f"server didn't print READY within {boot_timeout:.0f}s; see {log_path}"
        )

    address, session_id = _parse_ready_line(ready)
    return proc, address, session_id


def _kill_server(proc: Optional[subprocess.Popen]) -> None:
    if proc is None or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()


@asynccontextmanager
async def _server_context(
    args: argparse.Namespace,
) -> AsyncIterator[Tuple[str, str]]:
    """Yield (address, session_id). Spawns a helper server if needed."""
    if args.address and args.session:
        yield args.address, args.session
        return

    proc, address, session_id = await _spawn_server(args.boot_timeout)
    try:
        yield address, session_id
    finally:
        _kill_server(proc)


# ─── Entrypoint ───────────────────────────────────────────────────────────

def _read_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    p.add_argument(
        "--address",
        help="gRPC server address (skip to auto-spawn the helper server)",
    )
    p.add_argument(
        "--session",
        help="Pipeline session id (skip to auto-spawn the helper server)",
    )
    p.add_argument(
        "--node-id",
        default="lfm",
        help="Node id for the LFM2 node in the manifest (default: lfm)",
    )
    p.add_argument(
        "--reply-timeout",
        type=float,
        default=60.0,
        help="Per-reply timeout in seconds (default: 60)",
    )
    p.add_argument(
        "--boot-timeout",
        type=float,
        default=300.0,
        help="Auto-spawn mode: time to wait for the server's READY line "
             "(default: 300s; first run includes cargo build + model load)",
    )
    p.add_argument(
        "--from-ready",
        action="store_true",
        help="Read 'READY <port> <session>' from stdin instead of spawning",
    )
    args = p.parse_args()

    if args.from_ready:
        args.address, args.session = _parse_ready_line(sys.stdin.readline())
    elif bool(args.address) != bool(args.session):
        p.error("--address and --session must be given together")
    return args


async def main() -> int:
    args = _read_args()

    async with _server_context(args) as (address, session_id):
        print(f"Attaching to {address} session={session_id}")
        async with attach(address, session_id=session_id) as ctrl:
            tap = await ctrl.subscribe(f"{args.node_id}.out")

            # Let the subscription register before the first publish.
            await asyncio.sleep(0.2)

            client = ChatClient(
                ctrl=ctrl,
                tap=tap,
                node_id=args.node_id,
                reply_timeout=args.reply_timeout,
            )
            await run_demo(client)

    print("\nDone. Detached from session.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(asyncio.run(main()))
    except KeyboardInterrupt:
        raise SystemExit(130)
