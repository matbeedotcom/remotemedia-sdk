#!/usr/bin/env python3
"""
TTS engine benchmark — QwenTTSMlxNode vs KokoroTTSNode (or any other
node that implements the same async-generator `process()` contract).

Runs a fixed corpus through each engine in-process (no multiprocess IPC,
no router, no WebRTC) and reports:

    TTFA (time-to-first-audio) : wall-clock from input arrival to the
                                 first RuntimeData.Audio frame yielded
    wall                       : total synthesis wall time
    audio                      : total seconds of audio produced
    RTF (real-time factor)     : audio / wall; ≥ 1.0 means faster than
                                 realtime, < 1.0 means the engine is
                                 slower than speech and will starve a
                                 live pipeline
    chunk gap                  : mean inter-chunk Δt ms — a proxy for
                                 how continuous the streaming is

Pre-processing overhead (Metal JIT, weights → device) is absorbed by
running and discarding one warmup synthesis per engine before timing.

Usage
-----

    # Both engines, the default 3-run corpus, no audio saved.
    python clients/python/bench/tts_compare.py

    # Save generated audio for A/B listening.
    python clients/python/bench/tts_compare.py --save-audio /tmp/tts

    # Just the short sentence, 10 runs each, Qwen only.
    python clients/python/bench/tts_compare.py \\
        --engines qwen --corpus short --runs 10

Requirements: numpy (required), soundfile (optional, for --save-audio),
plus whatever the selected engines pull in (mlx-lm + mlx-audio for
qwen; kokoro + soundfile + en_core_web_sm for kokoro).
"""

from __future__ import annotations

import argparse
import asyncio
import logging
import statistics
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Dict, List, Optional, Tuple

try:
    import numpy as np
except ImportError as e:  # pragma: no cover
    raise SystemExit("numpy required: pip install numpy") from e


def _import_runtime_data():
    # Imported lazily: `--help` must work without remotemedia installed
    # in the host interpreter (users commonly run this from the same
    # managed venv that the multiprocess runtime provisions).
    try:
        from remotemedia.core.multiprocessing.data import RuntimeData  # noqa: WPS433
    except ImportError as exc:
        raise SystemExit(
            f"remotemedia import failed: {exc}\n"
            "Install the Python client (`pip install -e clients/python`) "
            "or run this script with the venv that has it."
        ) from exc
    return RuntimeData


# ─────────────────────────── test corpus ────────────────────────────
#
# Short targets TTFA (first-token-latency dominates perceived speed on
# a one-word reply). Medium is representative of typical voice-assistant
# replies — the thing we actually ship. Long stresses throughput; if an
# engine's RTF is < 1 on `long` it can't keep up with continuous speech.
CORPUS: Dict[str, str] = {
    "short": "Hello, Mathieu.",
    "medium": (
        "I am an artificial intelligence and do not have a country of origin."
    ),
    "long": (
        "The morning sun filtered through the kitchen window, casting long "
        "shadows on the worn wooden floor. She poured a second cup of coffee "
        "and watched the birds argue in the backyard. It was going to be a "
        "good day."
    ),
}


@dataclass
class Sample:
    """One (engine, corpus, run) measurement."""

    ttfa_s: float
    wall_s: float
    audio_s: float
    sample_rate: int
    chunk_times_s: List[float] = field(default_factory=list)

    @property
    def chunks(self) -> int:
        return len(self.chunk_times_s)

    @property
    def rtf(self) -> float:
        return self.audio_s / self.wall_s if self.wall_s > 0 else 0.0

    @property
    def mean_chunk_gap_ms(self) -> float:
        if len(self.chunk_times_s) < 2:
            return 0.0
        gaps = [
            self.chunk_times_s[i] - self.chunk_times_s[i - 1]
            for i in range(1, len(self.chunk_times_s))
        ]
        return statistics.mean(gaps) * 1000.0


# ───────────────────────── engine factories ─────────────────────────
#
# Each factory returns a ready-to-`initialize()` node instance. Kept
# lazy so we don't import mlx-audio if the user only wants Kokoro (and
# vice versa) — loading the wrong stack can take seconds.
def _make_qwen():
    from remotemedia.nodes.ml.qwen_tts_mlx import QwenTTSMlxNode  # noqa: WPS433
    # Disable passthrough text so the engine's output stream is pure
    # audio — simplifies the bench inner loop and avoids surprising
    # first-frame-is-text timing artefacts.
    return QwenTTSMlxNode(node_id="bench_qwen", passthrough_text=False)


def _make_kokoro():
    from remotemedia.nodes.tts import KokoroTTSNode  # noqa: WPS433
    return KokoroTTSNode(node_id="bench_kokoro")


ENGINE_FACTORIES: Dict[str, Callable[[], object]] = {
    "qwen": _make_qwen,
    "kokoro": _make_kokoro,
}


# ─────────────────────────── measurement ────────────────────────────
async def _run_one(node, text: str) -> Tuple[Sample, List[np.ndarray]]:
    """Feed `text` through `node.process()` and time each audio chunk."""
    RuntimeData = _import_runtime_data()
    t0 = time.monotonic()
    ttfa: Optional[float] = None
    chunk_times: List[float] = []
    total_samples = 0
    sample_rate = 0
    frames: List[np.ndarray] = []

    async for frame in node.process(RuntimeData.text(text)):
        if frame is None or not frame.is_audio():
            continue
        now = time.monotonic() - t0
        if ttfa is None:
            ttfa = now
        chunk_times.append(now)

        arr = frame.as_numpy().astype(np.float32, copy=False).reshape(-1)
        frames.append(arr)
        total_samples += arr.size
        md = getattr(frame, "metadata", None)
        if md is not None:
            sample_rate = int(getattr(md, "sample_rate", sample_rate or 0))

    wall = time.monotonic() - t0
    audio_s = total_samples / sample_rate if sample_rate > 0 else 0.0

    return (
        Sample(
            ttfa_s=ttfa if ttfa is not None else wall,
            wall_s=wall,
            audio_s=audio_s,
            sample_rate=sample_rate,
            chunk_times_s=chunk_times,
        ),
        frames,
    )


async def run_engine(
    engine_name: str,
    corpus_items: List[Tuple[str, str]],
    runs: int,
    out_dir: Optional[Path],
) -> Dict[str, List[Sample]]:
    print(f"\n[{engine_name}] initialising...", flush=True)
    t_init = time.monotonic()
    node = ENGINE_FACTORIES[engine_name]()
    await node.initialize()
    print(f"[{engine_name}]   init wall: {time.monotonic() - t_init:.2f}s")

    # Warmup — discard. Absorbs Metal JIT, lazy weight load, tokenizer
    # spin-up. Without this the first real run gets a 5-10 s penalty
    # that has nothing to do with steady-state throughput.
    print(f"[{engine_name}]   warmup (discarded)...", flush=True)
    RuntimeData = _import_runtime_data()
    async for _ in node.process(RuntimeData.text("Warm up.")):
        pass

    results: Dict[str, List[Sample]] = {}
    for label, text in corpus_items:
        samples: List[Sample] = []
        saved = False
        for run_idx in range(runs):
            sample, frames = await _run_one(node, text)
            samples.append(sample)

            if out_dir is not None and not saved and frames:
                # Save only the first run per (engine, corpus) — A/B
                # listening just needs one sample per cell.
                try:
                    import soundfile as sf  # noqa: WPS433
                except ImportError:
                    print("[warn] soundfile not installed; skipping --save-audio",
                          file=sys.stderr)
                    out_dir = None  # stop trying
                else:
                    fpath = out_dir / f"{engine_name}_{label}.wav"
                    sf.write(fpath, np.concatenate(frames), sample.sample_rate)
                    saved = True

        ttfa_ms = [s.ttfa_s * 1000 for s in samples]
        rtfs = [s.rtf for s in samples]
        print(
            f"[{engine_name}]   {label:<6} ({len(text):>3} chars): "
            f"TTFA p50={statistics.median(ttfa_ms):>6.0f} ms  "
            f"RTF p50={statistics.median(rtfs):.2f}  "
            f"runs={len(samples)}"
        )
        results[label] = samples

    await node.cleanup()
    return results


# ────────────────────────────── report ──────────────────────────────
def _pct(xs: List[float], p: float) -> float:
    if not xs:
        return 0.0
    xs = sorted(xs)
    idx = min(len(xs) - 1, int(round((p / 100.0) * (len(xs) - 1))))
    return xs[idx]


def print_report(all_results: Dict[str, Dict[str, List[Sample]]]) -> None:
    print("\n## Benchmark results\n")
    print(
        "| engine | corpus | TTFA p50 / p95 (ms) | wall p50 (s) | "
        "audio (s) | RTF p50 | chunks | chunk Δt (ms) |"
    )
    print("|---|---|---|---|---|---|---|---|")
    for engine, by_label in all_results.items():
        for label, runs in by_label.items():
            ttfa_ms = [s.ttfa_s * 1000 for s in runs]
            walls = [s.wall_s for s in runs]
            audios = [s.audio_s for s in runs]
            rtfs = [s.rtf for s in runs]
            chunk_counts = [s.chunks for s in runs]
            gaps = [s.mean_chunk_gap_ms for s in runs]

            print(
                f"| {engine} | {label} | "
                f"{_pct(ttfa_ms, 50):.0f} / {_pct(ttfa_ms, 95):.0f} | "
                f"{_pct(walls, 50):.2f} | "
                f"{statistics.mean(audios):.2f} | "
                f"{_pct(rtfs, 50):.2f} | "
                f"{statistics.mean(chunk_counts):.1f} | "
                f"{statistics.mean(gaps):.0f} |"
            )
    print()

    # Head-to-head ratio when both engines ran the same corpus labels.
    engines = list(all_results.keys())
    if len(engines) < 2:
        return
    a, b = engines[0], engines[1]
    shared = [k for k in all_results[a] if k in all_results[b]]
    if not shared:
        return
    print(f"## Head-to-head: {a} vs {b}\n")
    print(f"| corpus | TTFA ratio ({a}/{b}) | RTF ratio ({a}/{b}) |")
    print("|---|---|---|")
    for label in shared:
        ttfa_a = statistics.median(s.ttfa_s for s in all_results[a][label])
        ttfa_b = statistics.median(s.ttfa_s for s in all_results[b][label])
        rtf_a = statistics.median(s.rtf for s in all_results[a][label])
        rtf_b = statistics.median(s.rtf for s in all_results[b][label])
        print(
            f"| {label} | "
            f"{(ttfa_a / ttfa_b) if ttfa_b else float('nan'):.2f}x | "
            f"{(rtf_a / rtf_b) if rtf_b else float('nan'):.2f}x |"
        )
    print()


# ────────────────────────────── main ────────────────────────────────
async def main_async(args: argparse.Namespace) -> None:
    corpus_items = [(k, CORPUS[k]) for k in args.corpus]
    all_results: Dict[str, Dict[str, List[Sample]]] = {}

    for engine in args.engines:
        all_results[engine] = await run_engine(
            engine, corpus_items, args.runs, args.save_audio
        )

    print_report(all_results)


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--engines",
        nargs="+",
        default=list(ENGINE_FACTORIES),
        choices=list(ENGINE_FACTORIES),
        help="Which TTS engines to benchmark (default: all).",
    )
    parser.add_argument(
        "--corpus",
        nargs="+",
        default=list(CORPUS),
        choices=list(CORPUS),
        help="Which corpus items to synthesise (default: all).",
    )
    parser.add_argument(
        "--runs", type=int, default=3,
        help="Repetitions per (engine, corpus) pair (default: 3).",
    )
    parser.add_argument(
        "--save-audio",
        type=Path,
        default=None,
        metavar="DIR",
        help="Write the first run of each (engine, corpus) to DIR/<engine>_<label>.wav.",
    )
    parser.add_argument(
        "--verbose", action="store_true",
        help="Print engine-internal logs (INFO level).",
    )
    args = parser.parse_args()

    logging.basicConfig(
        level=logging.INFO if args.verbose else logging.WARNING,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    if args.save_audio is not None:
        args.save_audio.mkdir(parents=True, exist_ok=True)

    asyncio.run(main_async(args))


if __name__ == "__main__":
    main()
