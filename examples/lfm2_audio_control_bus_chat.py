#!/usr/bin/env python3
"""
Production-style multi-turn LFM2-Audio speech-to-speech chat over the
Session Control Bus.

This is the audio sibling of ``lfm2_control_bus_chat.py``. The server
hosts an ``LFM2AudioNode`` inside a multiprocess-Python worker. A
client attaches to the session and drives it entirely through the
control plane:

    publish("audio.in", audio_bytes)          -> user speaks a turn
    publish("audio.in.context", docs)         -> RAG context for next turn
    publish("audio.in.system_prompt", prompt) -> persona swap
    publish("audio.in.reset", "")             -> drop prior history
    publish("audio.in.barge_in", "")          -> interrupt current reply
    subscribe("audio.out")                    -> streamed interleaved text + audio

Replies come back as a mix of ``RuntimeData.Text`` events (caption
fragments, with terminal ``<|text_end|>`` / ``<|audio_end|>`` markers)
and ``RuntimeData.Audio`` events (24 kHz mono float32 frames).

## Whisper transcription taps

Transcription runs **server-side** as two extra pipeline nodes — not
in the client. The helper manifest wires:

    stt_in  (WhisperSTTNode)  -- receives user audio from the client
    stt_out (WhisperSTTNode)  -- receives LFM2-Audio output, fan-out from `audio`

The client simply subscribes to each node's output port:

    subscribe("stt_in.out")   → transcript of what the user said
    subscribe("stt_out.out")  → transcript of what LFM2 spoke back

The user audio is published twice — once to ``audio.in`` (consumed by
LFM2-Audio) and once to ``stt_in.in`` (consumed by the input Whisper
node). The LFM2 → ``stt_out`` fan-out is declared in the manifest, so
no second publish is needed there.

Because the transcripts come through the Session Control Bus, they are
interceptable / tappable / droppable / node-state-disableable using
the same primitives the rest of the conversation uses. Applications
wire an ``on_transcript(role, turn_idx, text, client)`` hook that can
call ``client.set_context(...)`` / ``set_system_prompt(...)`` /
``reset_history()`` / ``barge_in()`` based on what it hears.

## Running

Zero-config (auto-spawns the helper server, uses the bundled demo WAV):

    PYTHONPATH=clients/python \\
        python examples/lfm2_audio_control_bus_chat.py

Give it your own WAVs (one per turn), against a running server:

    python examples/lfm2_audio_control_bus_chat.py \\
        --address 127.0.0.1:50051 --session SID \\
        --turn1 q1.wav --turn2 q2.wav --turn3 q3.wav --turn4 q4.wav

Replies are written to ``./lfm2_audio_replies/turn_<N>.wav``.

## Requirements

- ``liquid_audio``, ``torch``, ``torchaudio``, ``transformers>=4.54``,
  ``soundfile``, ``numpy`` in the server-spawned Python environment.
  See ``docs/MANAGED_PYTHON_ENVIRONMENTS.md``.
- An input WAV at the model's native rate (24 kHz mono). The script
  resamples on the client side if ``soundfile`` gives us a different rate.
"""

from __future__ import annotations

import argparse
import asyncio
import os
import re
import subprocess
import sys
import threading
import wave
from contextlib import asynccontextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import (
    Any,
    AsyncIterator,
    Awaitable,
    Callable,
    List,
    Optional,
    Tuple,
)

import numpy as np

from remotemedia.control import Data, attach

REPO_ROOT = Path(__file__).resolve().parents[1]
CLIENTS_PYTHON = REPO_ROOT / "clients" / "python"
DEFAULT_DEMO_WAV = REPO_ROOT / "examples" / "transcribe_demo.wav"

MODEL_SAMPLE_RATE = 24000  # LFM2-Audio native rate (both in and out)
WHISPER_SAMPLE_RATE = 16000  # what every whisper model wants

# Role constants for the transcript hook. Using plain strings keeps the
# callback signature import-free for users.
ROLE_USER = "user"
ROLE_ASSISTANT = "assistant"


# ─── Audio helpers ────────────────────────────────────────────────────────

def _load_wav_mono24k(path: Path) -> np.ndarray:
    """
    Load a WAV file as mono float32 at 24 kHz.

    Prefers ``soundfile`` (handles most wav subtypes); falls back to the
    stdlib ``wave`` module for simple 16-bit PCM files so this example
    runs in environments without soundfile installed.
    """
    try:
        import soundfile as sf
        data, sr = sf.read(str(path), dtype="float32", always_2d=False)
    except ImportError:
        with wave.open(str(path), "rb") as wf:
            sr = wf.getframerate()
            n_channels = wf.getnchannels()
            sampwidth = wf.getsampwidth()
            if sampwidth != 2:
                raise RuntimeError(
                    f"{path}: only 16-bit PCM WAVs supported without soundfile "
                    f"(got {sampwidth * 8}-bit). Install `soundfile` or resample."
                )
            raw = wf.readframes(wf.getnframes())
        data = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
        if n_channels == 2:
            data = data.reshape(-1, 2)

    # Downmix stereo → mono if needed.
    if data.ndim == 2:
        data = data.mean(axis=1)

    if sr != MODEL_SAMPLE_RATE:
        # Simple linear resample — good enough for a demo. For production
        # use `resampy.resample(data, sr, MODEL_SAMPLE_RATE)` which
        # preserves audio quality.
        try:
            import resampy
            data = resampy.resample(data, sr, MODEL_SAMPLE_RATE)
        except ImportError:
            ratio = MODEL_SAMPLE_RATE / sr
            n_out = int(round(len(data) * ratio))
            data = np.interp(
                np.linspace(0.0, len(data) - 1.0, n_out, dtype=np.float64),
                np.arange(len(data), dtype=np.float64),
                data.astype(np.float64),
            ).astype(np.float32)

    return np.ascontiguousarray(data.astype(np.float32))


def _write_wav_mono24k(path: Path, samples: np.ndarray) -> None:
    """Write a 24 kHz mono float32 numpy array as a 16-bit PCM WAV."""
    path.parent.mkdir(parents=True, exist_ok=True)
    clipped = np.clip(samples, -1.0, 1.0)
    pcm = (clipped * 32767.0).astype(np.int16)
    with wave.open(str(path), "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(MODEL_SAMPLE_RATE)
        wf.writeframes(pcm.tobytes())


def _numpy_to_audio_data(samples: np.ndarray) -> Data:
    """
    Wrap a 24 kHz mono float32 numpy array as a control-bus `Data` with
    the audio oneof. ``Data.from_*`` doesn't have an audio constructor,
    so we populate ``AudioBuffer`` directly. Field names match
    ``proto/common.proto::AudioBuffer``.
    """
    from remotemedia.protos import common_pb2 as _common_pb

    if samples.dtype != np.float32:
        samples = samples.astype(np.float32)
    samples = np.ascontiguousarray(samples)
    n = int(samples.shape[0])

    buf = _common_pb.DataBuffer()
    buf.audio.samples = samples.tobytes()
    buf.audio.sample_rate = MODEL_SAMPLE_RATE
    buf.audio.channels = 1
    buf.audio.format = _common_pb.AUDIO_FORMAT_F32
    buf.audio.num_samples = n
    return Data(buf)


# ─── Transcript hook signature ────────────────────────────────────────────

#: Signature: ``async fn(role, turn_idx, text, client) -> None``.
#: ``role`` is ``ROLE_USER`` or ``ROLE_ASSISTANT``. Fires when a
#: server-side Whisper node publishes a transcript. The hook gets the
#: same ``S2SClient`` handle the rest of the code uses, so it can
#: ``await client.set_context(...)`` / ``set_system_prompt(...)`` /
#: ``reset_history()`` / ``barge_in()`` to steer the conversation.
TranscriptHook = Callable[[str, int, str, "S2SClient"], Awaitable[None]]


# ─── Reply accumulation ───────────────────────────────────────────────────

@dataclass
class TurnReply:
    """One assistant turn: streamed text + concatenated audio samples."""

    text: str = ""
    audio: List[np.ndarray] = field(default_factory=list)
    text_done: bool = False
    audio_done: bool = False

    @property
    def is_complete(self) -> bool:
        return self.text_done and self.audio_done

    def audio_samples(self) -> np.ndarray:
        return (
            np.concatenate(self.audio).astype(np.float32)
            if self.audio else np.zeros(0, dtype=np.float32)
        )


def _extract_audio_samples(data: Data) -> Optional[np.ndarray]:
    """
    Pull 24 kHz float32 samples out of a tap ``Data`` if it carries audio.
    Expects ``AudioBuffer`` format with f32 LE in ``samples``.
    """
    if data.kind != "audio":
        return None
    audio = data.as_buffer().audio
    if not audio.samples:
        return None
    return np.frombuffer(audio.samples, dtype=np.float32).copy()


async def _collect_turn(
    tap: AsyncIterator[Data],
    *,
    timeout: float,
) -> TurnReply:
    """
    Consume tap ``Data`` events until we see both terminal markers
    (``<|text_end|>`` and ``<|audio_end|>``) yielded once per turn by
    ``LFM2AudioNode``.
    """
    reply = TurnReply()
    deadline = asyncio.get_event_loop().time() + timeout
    while not reply.is_complete:
        remaining = deadline - asyncio.get_event_loop().time()
        if remaining <= 0:
            raise asyncio.TimeoutError(f"turn did not complete within {timeout}s")

        data: Data = await asyncio.wait_for(tap.__anext__(), timeout=remaining)

        if data.kind == "text":
            frag = data.text_value
            if frag == "<|text_end|>":
                reply.text_done = True
            elif frag == "<|audio_end|>":
                reply.audio_done = True
            else:
                reply.text += frag
            continue

        samples = _extract_audio_samples(data)
        if samples is not None:
            reply.audio.append(samples)

    return reply


# ─── Chat surface ─────────────────────────────────────────────────────────

@dataclass
class S2SClient:
    ctrl: object
    #: tap on ``audio.out`` — interleaved text + audio reply stream
    tap: AsyncIterator
    #: tap on ``stt_in.out`` — transcript of what the user said
    stt_in_tap: Optional[AsyncIterator] = None
    #: tap on ``stt_out.out`` — transcript of what LFM2 said back
    stt_out_tap: Optional[AsyncIterator] = None
    node_id: str = "audio"
    stt_in_node_id: str = "stt_in"
    reply_timeout: float = 300.0
    on_transcript: Optional[TranscriptHook] = None

    async def say(self, audio: np.ndarray) -> TurnReply:
        """
        Send one audio user turn, collect the interleaved reply.

        Publishes to BOTH ``audio.in`` (LFM2 consumes) and
        ``stt_in.in`` (input Whisper node consumes). The LFM2 output
        fans out to ``stt_out`` inside the server manifest, so we only
        need to subscribe to ``stt_out.out`` — no second publish there.
        """
        audio_data = _numpy_to_audio_data(audio)
        if self.stt_in_tap is not None:
            await self.ctrl.publish(f"{self.stt_in_node_id}.in", audio_data)
        await self.ctrl.publish(f"{self.node_id}.in", audio_data)
        return await _collect_turn(self.tap, timeout=self.reply_timeout)

    async def drain_transcript(
        self,
        which: str,
        *,
        timeout: float,
    ) -> str:
        """
        Read exactly one transcript event from ``stt_in.out`` or
        ``stt_out.out``. Returns ``""`` if the tap isn't wired up or
        the timeout expires.
        """
        tap = self.stt_in_tap if which == ROLE_USER else self.stt_out_tap
        if tap is None:
            return ""
        try:
            data: Data = await asyncio.wait_for(tap.__anext__(), timeout=timeout)
        except asyncio.TimeoutError:
            return ""
        if data.kind != "text":
            return ""
        return data.text_value.strip()

    async def set_context(self, docs: str) -> None:
        await self.ctrl.publish(
            f"{self.node_id}.in.context", Data.from_text(docs)
        )

    async def set_system_prompt(self, prompt: str) -> None:
        await self.ctrl.publish(
            f"{self.node_id}.in.system_prompt", Data.from_text(prompt)
        )

    async def reset_history(self) -> None:
        await self.ctrl.publish(
            f"{self.node_id}.in.reset", Data.from_text("")
        )

    async def clear_context(self) -> None:
        await self.ctrl.publish(
            f"{self.node_id}.in.context", Data.from_text("")
        )

    async def barge_in(self) -> None:
        await self.ctrl.publish(
            f"{self.node_id}.in.barge_in", Data.from_text("")
        )


# ─── Demo scenario ────────────────────────────────────────────────────────

async def run_demo(
    client: S2SClient,
    *,
    turn_wavs: List[Path],
    out_dir: Path,
) -> None:
    """
    Four-turn sequence showing the full control surface:

    1. Baseline turn with no injected context.
    2. Inject RAG context + reset history — next turn should use it.
    3. Swap the system prompt to a new persona.
    4. Clear everything — back to the default assistant.
    """
    assert len(turn_wavs) == 4, "demo expects 4 turn WAVs"

    out_dir.mkdir(parents=True, exist_ok=True)

    async def do_turn(idx: int, wav: Path) -> None:
        print(f"\n[turn {idx}] USER_AUDIO:  {wav}")
        user_audio = _load_wav_mono24k(wav)

        # Publish + collect the S2S reply. `say()` also publishes to
        # stt_in.in, so the input Whisper node is already processing
        # concurrently with LFM2.
        reply = await client.say(user_audio)
        reply_path = out_dir / f"turn_{idx}.wav"
        _write_wav_mono24k(reply_path, reply.audio_samples())

        # Drain the transcripts. stt_in emits once per user turn;
        # stt_out emits once per LFM2 output packet, and the server
        # manifest wires `audio → stt_out` so it processes whatever
        # LFM2 just streamed. Both taps are optional — drain with a
        # short deadline so a missing STT node doesn't stall the demo.
        user_stt = await client.drain_transcript(ROLE_USER, timeout=30.0)
        assist_stt = await client.drain_transcript(ROLE_ASSISTANT, timeout=60.0)

        if user_stt:
            print(f"[turn {idx}] USER_STT:    {user_stt}")
        if reply.text.strip():
            print(f"[turn {idx}] REPLY_TEXT:  {reply.text.strip()}")
        if assist_stt:
            print(f"[turn {idx}] ASSIST_STT:  {assist_stt}")
        print(f"[turn {idx}] REPLY_AUDIO: {reply_path} "
              f"({len(reply.audio_samples()) / MODEL_SAMPLE_RATE:.2f}s)")

        # Fire the hooks after both transcripts are known.
        if client.on_transcript is not None:
            await client.on_transcript(ROLE_USER, idx, user_stt, client)
            await client.on_transcript(ROLE_ASSISTANT, idx, assist_stt, client)

    # Turn 1 — baseline.
    await do_turn(1, turn_wavs[0])

    # Inject context, reset so the new system prompt takes effect cleanly.
    await client.set_context(
        "The user's favorite color is cerulean. "
        "They prefer short, friendly replies."
    )
    await client.reset_history()

    # Turn 2 — should reflect the injected context.
    await do_turn(2, turn_wavs[1])

    # Swap persona mid-conversation.
    await client.set_system_prompt(
        "You are a cheerful pirate. Begin every reply with 'Arrr!' "
        "and respond with interleaved text and audio."
    )
    await client.reset_history()

    # Turn 3 — pirate persona takes over.
    await do_turn(3, turn_wavs[2])

    # Wipe everything — back to the default assistant.
    await client.clear_context()
    await client.set_system_prompt(
        "Respond with interleaved text and audio. Be concise."
    )
    await client.reset_history()

    # Turn 4 — no persona, no context.
    await do_turn(4, turn_wavs[3])


# ─── Server lifecycle (auto-spawn mode) ───────────────────────────────────

def _parse_ready_line(line: str) -> Tuple[str, str]:
    parts = line.strip().split()
    if len(parts) < 3 or parts[0] != "READY":
        raise ValueError(f"expected 'READY <port> <session>', got: {line!r}")
    return f"127.0.0.1:{parts[1]}", parts[2]


# iceoryx2 dumps a multi-kB `BuilderWithServiceType { ... }` blob every
# time a service is opened (one per node, per channel). It's true debug
# output — never load-bearing, never actionable. The two follow-up
# markers ("Old dynamic config...", "No config file was loaded...") are
# equally noisy. Drop them from the console tee so real errors aren't
# buried; the raw file archive keeps them if anyone wants to dig.
_NOISE_PATTERNS = (
    re.compile(r"^\s*\d+ \[[WID]\] BuilderWithServiceType\b"),
    re.compile(r"^\s*\|\s*Old dynamic config"),
    re.compile(r"^\s*\|\s*No config file was loaded"),
)


def _is_noise(line: str) -> bool:
    return any(p.match(line) for p in _NOISE_PATTERNS)


def _stream_forwarder(
    source,
    *,
    label: str,
    log_fh,
    console: "object | None",
) -> threading.Thread:
    """
    Tee `source` (a text-mode pipe) into `log_fh` unchanged and — after
    filtering known noise — into `console` (typically sys.stderr), each
    line prefixed with `[server <label>]`. The background thread exits
    when the source closes (i.e. the subprocess ends).
    """

    def _run() -> None:
        for raw in iter(source.readline, ""):
            log_fh.write(raw)
            log_fh.flush()
            if console is None:
                continue
            if _is_noise(raw):
                continue
            console.write(f"[server {label}] {raw}")
            console.flush()
        try:
            source.close()
        except Exception:
            pass

    t = threading.Thread(target=_run, name=f"server-{label}-forwarder", daemon=True)
    t.start()
    return t


async def _spawn_server(boot_timeout: float) -> Tuple[subprocess.Popen, str, str]:
    cmd = [
        "cargo", "run", "--quiet",
        "-p", "remotemedia-grpc",
        # `multiprocess` + `bundled-uv` give us per-node managed venvs
        # backed by uv. Without `bundled-uv`, the fallback SystemBackend
        # uses `python -m venv + pip install`, which needs `python3` to
        # be on the subprocess PATH and is much slower.
        "--features", "multiprocess,bundled-uv",
        "--example", "control_bus_test_server",
    ]
    env = dict(os.environ)
    env["TEST_SESSION_KIND"] = "lfm2audio"
    # `info` is what the env-manager, uv provisioning, and per-node init
    # log at. Without it you only see warnings, which makes "why did this
    # node fail?" unanswerable. Trim with RUST_LOG=warn once it works.
    env.setdefault(
        "RUST_LOG",
        "info,remotemedia_core::python::env_manager=debug,"
        "remotemedia_core::python::multiprocess=info",
    )
    existing_pp = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{CLIENTS_PYTHON}:{existing_pp}" if existing_pp else str(CLIENTS_PYTHON)
    )
    # Use managed per-node venvs so the heavy `liquid-audio` / torch stack
    # for LFM2-Audio stays out of the host interpreter. The manifest's
    # `python_deps` for each node feeds into the env manager, which
    # provisions a dedicated venv via `uv` (see
    # `docs/MANAGED_PYTHON_ENVIRONMENTS.md`). Honour a pre-set value in
    # case the caller wants PYTHON_ENV_MODE=managed_with_python or
    # =system for debugging.
    env.setdefault("PYTHON_ENV_MODE", "managed")
    # Prefer the MLX build of LFM2-Audio on Apple Silicon —
    # no torch, no CUDA, runs on Metal. The helper server's manifest
    # checks $LFM2_AUDIO_BACKEND and swaps `LFM2AudioNode` ↔
    # `LFM2AudioMlxNode` accordingly. Override with
    # `LFM2_AUDIO_BACKEND=torch` if you want the liquid-audio path.
    import platform as _platform
    if _platform.system() == "Darwin" and _platform.machine() == "arm64":
        env.setdefault("LFM2_AUDIO_BACKEND", "mlx")
    # `liquid-audio` requires Python >= 3.12. The default venv
    # interpreter is 3.11, which fails pub-gub resolution. Pin via the
    # env-manager override so `uv` provisions (or downloads) a 3.12
    # interpreter. Manifest-scope `python_env.python_version` isn't
    # plumbed through the session router yet, so we nudge it here.
    env.setdefault("PYTHON_VERSION", "3.12")
    # Install the `remotemedia` client into every managed venv. Without
    # this, the multiprocess runner inside each venv can't import the
    # package that defines LFM2AudioNode / WhisperSTTNode / etc., and
    # `get_node_class` returns `Node type '...' not registered`.
    # Editable install → local edits are picked up on next venv reuse.
    env.setdefault("REMOTEMEDIA_PYTHON_SRC", str(CLIENTS_PYTHON))

    log_path = Path("/tmp") / f"lfm2_audio_server_{os.getpid()}.log"
    log_fh = open(log_path, "w")
    print(f"Spawning helper server (raw log archived at {log_path})")
    print("Server stderr is mirrored below — iceoryx2 debug walls are filtered.")

    # stdout stays on a pipe so we can parse the `READY <port> <session>`
    # handshake. stderr goes through a pipe too and is tee'd to both the
    # archive log and the user's terminal. Anything the server (or the
    # Python subprocesses it forks) prints — tracebacks, missing deps,
    # model-load progress, Rust panics — lands on the console in real time.
    proc = subprocess.Popen(
        cmd,
        cwd=str(REPO_ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        text=True,
        bufsize=1,
    )

    _stream_forwarder(
        proc.stderr,
        label="stderr",
        log_fh=log_fh,
        console=sys.stderr,
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
            # Non-READY stdout lines — rare; forward to the user just in
            # case they carry a last-gasp error the server printed before
            # giving up.
            print(f"[server stdout] {line}", file=sys.stderr)
            log_fh.write(line + "\n")
            log_fh.flush()

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
async def _server_context(args: argparse.Namespace) -> AsyncIterator[Tuple[str, str]]:
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
    p.add_argument("--address", help="gRPC server address (skip to auto-spawn)")
    p.add_argument("--session", help="Pipeline session id (skip to auto-spawn)")
    p.add_argument(
        "--node-id",
        default="audio",
        help="Node id for the LFM2-Audio node in the manifest (default: audio)",
    )
    p.add_argument("--turn1", type=Path, help="WAV for turn 1")
    p.add_argument("--turn2", type=Path, help="WAV for turn 2")
    p.add_argument("--turn3", type=Path, help="WAV for turn 3")
    p.add_argument("--turn4", type=Path, help="WAV for turn 4")
    p.add_argument(
        "--demo-wav",
        type=Path,
        default=DEFAULT_DEMO_WAV,
        help="Fallback WAV used for any turn that isn't overridden "
             f"(default: {DEFAULT_DEMO_WAV.name})",
    )
    p.add_argument(
        "--out-dir",
        type=Path,
        default=Path("lfm2_audio_replies"),
        help="Directory for per-turn reply WAVs (default: ./lfm2_audio_replies)",
    )
    p.add_argument(
        "--reply-timeout",
        type=float,
        default=300.0,
        help="Per-turn timeout for the full reply (default: 300s)",
    )
    p.add_argument(
        "--boot-timeout",
        type=float,
        default=900.0,
        help="Auto-spawn mode: time to wait for READY (default: 900s — first run "
             "includes cargo build + LFM2-Audio model download)",
    )
    p.add_argument(
        "--stt-in-node",
        default="stt_in",
        help="Node id for the input Whisper node (default: stt_in; pass '' to skip)",
    )
    p.add_argument(
        "--stt-out-node",
        default="stt_out",
        help="Node id for the output Whisper node (default: stt_out; pass '' to skip)",
    )
    args = p.parse_args()

    if bool(args.address) != bool(args.session):
        p.error("--address and --session must be given together")
    if not args.demo_wav.exists() and not all([
        args.turn1, args.turn2, args.turn3, args.turn4,
    ]):
        p.error(
            f"demo WAV {args.demo_wav} not found — provide all of "
            "--turn1/2/3/4 explicitly"
        )
    return args


async def _default_transcript_hook(
    role: str, turn_idx: int, text: str, client: "S2SClient"
) -> None:
    """
    Reference hook — the extension point for conversation control
    driven by transcripts. Inspect the text and steer the session via
    the same control-bus surface the rest of the code uses:

        await client.set_context("...")       # RAG / retrieval injection
        await client.set_system_prompt("...") # persona swap
        await client.reset_history()          # drop prior turns
        await client.barge_in()               # interrupt the NEXT turn

    Real applications plug intent classification / function-calling /
    redaction here. The default is a no-op beyond the print the caller
    already does.
    """
    return None


async def main() -> int:
    args = _read_args()

    turn_wavs = [
        args.turn1 or args.demo_wav,
        args.turn2 or args.demo_wav,
        args.turn3 or args.demo_wav,
        args.turn4 or args.demo_wav,
    ]
    for i, path in enumerate(turn_wavs, 1):
        if not path.exists():
            raise SystemExit(f"turn {i} WAV missing: {path}")

    async with _server_context(args) as (address, session_id):
        print(f"Attaching to {address} session={session_id}")
        async with attach(address, session_id=session_id) as ctrl:
            tap = await ctrl.subscribe(f"{args.node_id}.out")

            stt_in_tap = None
            if args.stt_in_node:
                stt_in_tap = await ctrl.subscribe(f"{args.stt_in_node}.out")
            stt_out_tap = None
            if args.stt_out_node:
                stt_out_tap = await ctrl.subscribe(f"{args.stt_out_node}.out")

            await asyncio.sleep(0.2)  # let all subscribes register

            client = S2SClient(
                ctrl=ctrl,
                tap=tap,
                stt_in_tap=stt_in_tap,
                stt_out_tap=stt_out_tap,
                node_id=args.node_id,
                stt_in_node_id=args.stt_in_node or "stt_in",
                reply_timeout=args.reply_timeout,
                on_transcript=_default_transcript_hook,
            )
            await run_demo(client, turn_wavs=turn_wavs, out_dir=args.out_dir)

    print(f"\nDone. Replies written to {args.out_dir}/")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(asyncio.run(main()))
    except KeyboardInterrupt:
        raise SystemExit(130)
