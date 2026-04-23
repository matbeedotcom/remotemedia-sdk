#!/usr/bin/env python3
"""
Standalone smoke test for PersonaPlexAudioMlxNode.

Bypasses Rust, multiprocess IPC, and WebRTC entirely. Instantiates the
node in-process, feeds it synthesized or file-based audio, collects
streaming outputs, and prints a PASS/FAIL summary with per-phase
timing. Designed to be the fast feedback loop while iterating on the
node — the full WebRTC stack only tells you "something timed out",
this tells you *which* phase.

## Usage

    # Minimal: 2 s of 440 Hz sine wave, default voice + persona.
    python examples/audio_examples/test_personaplex_pipeline.py

    # Feed a real wav (24 kHz mono, any duration).
    python examples/audio_examples/test_personaplex_pipeline.py \\
        --input-wav path/to/sample.wav

    # Try a specific voice + custom persona + injected knowledge.
    python examples/audio_examples/test_personaplex_pipeline.py \\
        --voice NATM0 \\
        --system-prompt "You are a terse assistant." \\
        --context "The capital of Mars is Olympus City."

    # Skip the warmup so you can measure cold-start cost separately.
    python examples/audio_examples/test_personaplex_pipeline.py --no-warmup

    # Use 4-bit quant for a smaller/faster first-run download.
    python examples/audio_examples/test_personaplex_pipeline.py --quantized 4

## What this exercises

- Module import (`personaplex_mlx`, `rustymimi`, `mlx`) — most deps
  failures surface here with the actual traceback, not a vague pipe
  error from the Rust side.
- `initialize()` — weight load + optional pre-warm.
- `process()` over several chunks — confirms the frame buffer stitches
  correctly across chunk boundaries (chunks are deliberately sized to
  not align with the 1920-sample frame size).
- Output routing — prints every text piece as it streams, counts audio
  frames, writes decoded audio to `/tmp/personaplex_test_out.wav` so
  you can listen.

Non-goals: no aux-port tests, no multi-turn. Those live in pytest.

## Exit codes

    0 — initialized, emitted at least one audio frame
    1 — init failed
    2 — no audio emitted after N chunks
    3 — unexpected exception
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import sys
import time
from pathlib import Path
from typing import List, Optional, Tuple

# The repo has two `remotemedia` packages — a minimal one at the repo
# root and the full client at clients/python/remotemedia. We need the
# latter for RuntimeData + the ml subpackage, so put it FIRST on
# sys.path. Also drop the old entry that points at repo root.
_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_CLIENT_ROOT = _REPO_ROOT / "clients" / "python"
sys.path.insert(0, str(_CLIENT_ROOT))
sys.path.insert(0, str(_REPO_ROOT))

import numpy as np  # noqa: E402

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname).1s %(name)s | %(message)s",
    datefmt="%H:%M:%S",
)
logger = logging.getLogger("personaplex.smoke")


def _build_sine(duration_s: float, sample_rate: int, freq_hz: float = 440.0) -> np.ndarray:
    """Synthesize mono float32 sine wave at ``sample_rate``."""
    t = np.linspace(0.0, duration_s, int(sample_rate * duration_s), dtype=np.float32)
    return (0.2 * np.sin(2.0 * np.pi * freq_hz * t)).astype(np.float32)


def _load_wav(path: Path, sample_rate: int) -> np.ndarray:
    """Load a wav file, resample to ``sample_rate`` if necessary.

    Uses ``sphn`` when available (already a personaplex-mlx dep); falls
    back to ``soundfile`` + ``numpy`` decimation (low-quality — warn).
    """
    try:
        import sphn  # type: ignore
    except ImportError:
        sphn = None  # type: ignore
    if sphn is not None:
        pcm, sr = sphn.read(str(path), sample_rate=sample_rate)
        # sphn returns (channels, samples); collapse to mono if needed.
        if pcm.ndim == 2 and pcm.shape[0] > 1:
            pcm = pcm.mean(axis=0, keepdims=True)
        return pcm.reshape(-1).astype(np.float32, copy=False)
    try:
        import soundfile as sf  # type: ignore
    except ImportError as e:
        raise RuntimeError(
            "neither sphn nor soundfile available; "
            "`pip install sphn` or `pip install soundfile`"
        ) from e
    data, file_sr = sf.read(str(path), dtype="float32", always_2d=False)
    if data.ndim > 1:
        data = data.mean(axis=1)
    if file_sr != sample_rate:
        logger.warning(
            "input is %dHz, need %dHz — using naive linear resample "
            "(install sphn for better quality)",
            file_sr, sample_rate,
        )
        # Naive: linear interp; fine for smoke-test purposes.
        ratio = sample_rate / file_sr
        new_len = int(len(data) * ratio)
        xp = np.linspace(0, 1, len(data), dtype=np.float32)
        xq = np.linspace(0, 1, new_len, dtype=np.float32)
        data = np.interp(xq, xp, data).astype(np.float32)
    return data.astype(np.float32, copy=False)


def _chunks(samples: np.ndarray, chunk_size: int):
    """Yield deliberately-misaligned chunks (chunk_size not a multiple
    of the 1920-sample frame size) so we exercise the node's pending
    buffer logic."""
    for i in range(0, len(samples), chunk_size):
        yield samples[i : i + chunk_size]


def _write_wav(path: Path, pcm: np.ndarray, sample_rate: int) -> None:
    try:
        import soundfile as sf  # type: ignore
        sf.write(str(path), pcm, sample_rate)
    except ImportError:
        try:
            import sphn  # type: ignore
            import rustymimi  # type: ignore
            # rustymimi.write_wav takes a 1-D float array
            rustymimi.write_wav(str(path), pcm, sample_rate=sample_rate)
            _ = sphn  # unused; just prove the import worked
        except ImportError:
            logger.warning("no soundfile / rustymimi — skipping output .wav write")


async def run_smoke(
    *,
    hf_repo: str,
    voice: str,
    system_prompt: str,
    context: Optional[str],
    quantized: Optional[int],
    input_wav: Optional[Path],
    duration_s: float,
    chunk_ms: int,
    warmup: bool,
    output_wav: Path,
) -> Tuple[bool, int, int]:
    """Run one end-to-end smoke pass. Returns (ok, text_pieces, audio_frames)."""

    t_import_start = time.time()
    try:
        from remotemedia.core.multiprocessing.data import RuntimeData
    except ImportError as e:
        print(f"FAIL: RuntimeData binding missing: {e}", file=sys.stderr)
        print("Build the Rust FFI: `./crates/transports/ffi/dev-install.sh`", file=sys.stderr)
        return False, 0, 0
    try:
        from remotemedia.nodes.ml.personaplex_audio_mlx import (
            PersonaPlexAudioMlxNode,
            _ML_DEPS_AVAILABLE,
            _ML_IMPORT_ERROR,
        )
    except ImportError as e:
        print(f"FAIL: node module missing: {e}", file=sys.stderr)
        return False, 0, 0
    if not _ML_DEPS_AVAILABLE:
        print(
            f"FAIL: personaplex_mlx / mlx deps not importable: {_ML_IMPORT_ERROR}",
            file=sys.stderr,
        )
        return False, 0, 0
    logger.info("imports OK in %.2fs", time.time() - t_import_start)

    sample_rate = 24000

    # ── Build input ────────────────────────────────────────────────
    if input_wav is not None:
        logger.info("loading %s @ %dHz", input_wav, sample_rate)
        samples = _load_wav(input_wav, sample_rate)
    else:
        logger.info("synthesizing %.1fs of 440Hz sine @ %dHz", duration_s, sample_rate)
        samples = _build_sine(duration_s, sample_rate)
    logger.info("input: %d samples (%.2fs)", len(samples), len(samples) / sample_rate)

    # ── Construct node ─────────────────────────────────────────────
    node = PersonaPlexAudioMlxNode(
        node_id="smoke_pp",
        hf_repo=hf_repo,
        voice=voice,
        system_prompt=system_prompt,
        quantized=quantized,
        sample_rate=sample_rate,
        # Give the smoke test full control — no warmup when --no-warmup,
        # otherwise the default "default" session is pre-primed.
        warmup_session_id=("default" if warmup else None),
    )
    if context:
        node.set_context(context)
        logger.info("context injected: %d chars", len(context))

    # ── Initialize (includes optional warmup) ──────────────────────
    t_init = time.time()
    try:
        await node.initialize()
    except Exception as e:  # noqa: BLE001
        logger.exception("initialize() failed: %s", e)
        return False, 0, 0
    init_elapsed = time.time() - t_init
    logger.info("initialize() OK in %.2fs (warmup=%s)", init_elapsed, warmup)

    # Loud warning about multiprocess-path implications. The Python
    # runner sends READY to Rust before `initialize()` returns (so it
    # can buffer input during model loading), which means Rust's
    # per-node execution timeout starts counting down BEFORE the node
    # is actually ready to process. The first real frame has to fit
    # into `DEFAULT_TIMEOUT_MS` (30 s in
    # crates/core/src/executor/streaming_scheduler.rs) *minus* however
    # long init still has to run. If init is longer than 30 s, every
    # multiprocess deployment needs a per-node timeout override.
    min_required_timeout_ms = int((init_elapsed + 30.0) * 1000)
    if init_elapsed > 25.0:
        logger.warning(
            "init took %.1fs — multiprocess deployments MUST bump the "
            "per-node execution timeout. Recommended minimum: "
            "scheduler_config.with_node_timeout(\"<node_id>\", %d) "
            "(init + 30 s headroom).",
            init_elapsed, min_required_timeout_ms,
        )
    else:
        logger.info(
            "init fits under the 30 s default timeout with %.1fs to spare — "
            "no multiprocess timeout override strictly required.",
            30.0 - init_elapsed,
        )

    # ── Feed audio in deliberately-misaligned chunks ───────────────
    # Chunk size chosen NOT to be a multiple of FRAME_SIZE (1920) so
    # the pending-buffer stitching path is actually exercised.
    chunk_size = max(1, int(sample_rate * chunk_ms / 1000))
    if chunk_size % 1920 == 0:
        chunk_size += 137  # force misalignment
    logger.info(
        "streaming %d chunks of %d samples (%.1f ms each, misaligned vs 80 ms frame)",
        (len(samples) + chunk_size - 1) // chunk_size,
        chunk_size, 1000.0 * chunk_size / sample_rate,
    )

    text_pieces: List[str] = []
    audio_frames: List[np.ndarray] = []

    t_first_out: Optional[float] = None
    t_feed_start = time.time()
    try:
        for chunk_idx, chunk in enumerate(_chunks(samples, chunk_size)):
            rd_in = RuntimeData.audio(
                chunk.astype(np.float32, copy=False), sample_rate, channels=1
            )
            # Process is an async generator; drain everything this chunk emits.
            async for out in node.process(rd_in):
                if t_first_out is None:
                    t_first_out = time.time()
                    logger.info(
                        "first output at %.2fs after feed start",
                        t_first_out - t_feed_start,
                    )
                if out.is_text():
                    piece = out.as_text()
                    text_pieces.append(piece)
                    sys.stdout.write(piece)
                    sys.stdout.flush()
                elif out.is_audio():
                    # Pure-Python RuntimeData exposes audio as .payload
                    # (numpy ndarray); the PyO3 build adds as_audio().
                    # Match the dual-path pattern the node itself uses.
                    if hasattr(out, "as_audio"):
                        raw, _sr, _ch, _fmt, _n = out.as_audio()
                        arr = np.frombuffer(raw, dtype=np.float32)
                    else:
                        payload = getattr(out, "payload", None)
                        if isinstance(payload, np.ndarray):
                            arr = payload.astype(np.float32, copy=False).reshape(-1)
                        elif isinstance(payload, (bytes, bytearray, memoryview)):
                            arr = np.frombuffer(bytes(payload), dtype=np.float32)
                        else:
                            logger.warning(
                                "unsupported audio payload type: %s",
                                type(payload).__name__,
                            )
                            continue
                    audio_frames.append(arr)
                else:
                    logger.warning(
                        "unexpected output type from node on chunk %d", chunk_idx
                    )
    except Exception as e:  # noqa: BLE001
        logger.exception("process() raised on chunk stream: %s", e)
        await node.cleanup()
        return False, len(text_pieces), len(audio_frames)
    print()  # newline after streamed text

    total_feed = time.time() - t_feed_start
    logger.info(
        "feed complete in %.2fs — %d text pieces, %d audio frames",
        total_feed, len(text_pieces), len(audio_frames),
    )

    # ── Write decoded audio for listening ──────────────────────────
    if audio_frames:
        full_pcm = np.concatenate(audio_frames)
        _write_wav(output_wav, full_pcm, sample_rate)
        logger.info(
            "wrote %s (%.2fs, peak=%.3f)",
            output_wav, len(full_pcm) / sample_rate,
            float(np.abs(full_pcm).max() if full_pcm.size else 0.0),
        )

    # ── Teardown ──────────────────────────────────────────────────
    await node.cleanup()

    # ── Verdict ───────────────────────────────────────────────────
    ok = len(audio_frames) > 0
    return ok, len(text_pieces), len(audio_frames)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--hf-repo", default="nvidia/personaplex-7b-v1")
    parser.add_argument("--voice", default="NATF2")
    parser.add_argument(
        "--system-prompt",
        default=(
            "You are a wise and friendly teacher. Respond briefly and clearly."
        ),
    )
    parser.add_argument("--context", default=None, help="Knowledge text to inject")
    parser.add_argument("--quantized", type=int, choices=[4, 8], default=8)
    parser.add_argument(
        "--input-wav", type=Path, default=None,
        help="Path to a wav file. If omitted, a sine wave is synthesized.",
    )
    parser.add_argument("--duration", type=float, default=2.0,
                        help="Synthesized sine duration in seconds (ignored with --input-wav)")
    parser.add_argument("--chunk-ms", type=int, default=200,
                        help="Chunk size in ms. Auto-adjusted to misalign with 80 ms frames.")
    parser.add_argument("--no-warmup", action="store_true",
                        help="Skip pre-warm; measure cold-start on first frame instead")
    parser.add_argument(
        "--output-wav", type=Path, default=Path("/tmp/personaplex_test_out.wav"),
    )
    args = parser.parse_args()

    t_all = time.time()
    try:
        ok, text_count, audio_count = asyncio.run(
            run_smoke(
                hf_repo=args.hf_repo,
                voice=args.voice,
                system_prompt=args.system_prompt,
                context=args.context,
                quantized=args.quantized,
                input_wav=args.input_wav,
                duration_s=args.duration,
                chunk_ms=args.chunk_ms,
                warmup=not args.no_warmup,
                output_wav=args.output_wav,
            )
        )
    except KeyboardInterrupt:
        print("\ninterrupted", file=sys.stderr)
        return 3
    except Exception as e:  # noqa: BLE001
        logger.exception("unexpected failure: %s", e)
        return 3

    elapsed = time.time() - t_all
    print(
        f"\n{'PASS' if ok else 'FAIL'}  "
        f"text={text_count} audio_frames={audio_count} "
        f"total={elapsed:.2f}s  out={args.output_wav}"
    )
    if ok:
        return 0
    if audio_count == 0:
        return 2
    return 1


if __name__ == "__main__":
    sys.exit(main())
