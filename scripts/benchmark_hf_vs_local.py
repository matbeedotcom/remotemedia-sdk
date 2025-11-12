#!/usr/bin/env python3
"""
Benchmark RemoteMedia gRPC pipeline vs a local Python-only baseline.

Features:
- Concurrency control (asyncio)
- E2E latency measurements with percentiles
- CSV output for post-processing
- Local gRPC benchmark using VAD pipeline manifest (unary call)
- Python-only baseline: run any local command per audio (e.g., OmniASR impl)

Usage (PowerShell) - gRPC only:
  python scripts/benchmark_hf_vs_local.py `
    --audio-dir examples/audio_examples `
    --server localhost:50051 `
    --concurrency 8 `
    --repeats 3 `
    --output results.csv

Python-only baseline example (OmniASR local):
  python scripts/benchmark_hf_vs_local.py `
    --audio-dir examples/audio_examples `
    --baseline-cmd "python C:\\Users\\mail\\dev\\personal\\omniasr-transcriptions\\main.py --input {audio}" `
    --baseline-cwd "C:\\Users\\mail\\dev\\personal\\omniasr-transcriptions" `
    --concurrency 4 `
    --repeats 2 `
    --output results.csv
"""

import argparse
import asyncio
import csv
import json
import os
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple
import shlex

# Local gRPC client (generated in repo)
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python-grpc-client"))
try:
    from remotemedia_client import RemoteMediaClient, AudioBuffer as GrpcAudioBuffer
except Exception as e:
    RemoteMediaClient = None  # type: ignore
    GrpcAudioBuffer = None  # type: ignore


@dataclass
class Sample:
    path: Path
    bytes: bytes
    sample_rate: int
    channels: int
    # This simple harness assumes PCM 16-bit WAV input where possible.
    # Adjust to match your baseline/gRPC expectations.


def discover_audio_files(audio_dir: Path) -> List[Path]:
    exts = {".wav", ".mp3", ".flac", ".m4a", ".ogg"}
    files: List[Path] = []
    for p in sorted(audio_dir.rglob("*")):
        if p.suffix.lower() in exts and p.is_file():
            files.append(p)
    return files


def read_audio_bytes(p: Path) -> bytes:
    # Read the audio file bytes; consumed by baseline or gRPC.
    return p.read_bytes()


def wav_meta_guess(p: Path) -> Tuple[int, int]:
    # Best-effort WAV header parse to infer sample_rate and channels.
    # For non-wav, default to 16000 mono as a placeholder for manifest.
    if p.suffix.lower() != ".wav":
        return 16000, 1
    try:
        import wave
        with wave.open(str(p), "rb") as w:
            return w.getframerate(), w.getnchannels()
    except Exception:
        return 16000, 1


def build_vad_manifest() -> Dict[str, Any]:
    """
    Minimal unary VAD pipeline manifest for local gRPC benchmarking.
    Adjust node_type if your repo uses a different registry name.
    """
    return {
        "version": "v1",
        "metadata": {
            "name": "benchmark_vad",
            "description": "VAD-only pipeline for benchmarking"
        },
        "nodes": [
            {
                "id": "source",
                "node_type": "AudioSource",
                "params": json.dumps({}),
                "is_streaming": False
            },
            {
                "id": "vad",
                "node_type": "VADNode",
                "params": json.dumps({"frame_ms": 30}),
                "is_streaming": False
            }
        ],
        "connections": [
            {"from": "source", "to": "vad"}
        ]
    }


async def bench_local_grpc(
    address: str,
    samples: List[Sample],
    concurrency: int,
    repeats: int
) -> List[Dict[str, Any]]:
    if RemoteMediaClient is None or GrpcAudioBuffer is None:
        raise RuntimeError("python-grpc-client not available. Ensure generated stubs and module imports succeed.")

    sem = asyncio.Semaphore(concurrency)
    results: List[Dict[str, Any]] = []
    manifest = build_vad_manifest()

    async with RemoteMediaClient(address) as client:
        async def run_one(sample: Sample, idx: int, rep: int) -> None:
            async with sem:
                # Prepare audio input buffer for unary call
                # This example sends raw bytes; your server expects specific format fields.
                # The AudioBuffer proto includes format and num_samples for validation.
                # If sending compressed audio, adjust accordingly.
                buffer = GrpcAudioBuffer(
                    samples=sample.bytes,
                    sample_rate=sample.sample_rate,
                    channels=sample.channels,
                    format=1,  # AUDIO_FORMAT_I16 by enum value (matches client wrapper)
                    num_samples=0,  # If unknown, server may infer or ignore for compressed
                )
                audio_inputs = {"source": buffer}

                t0 = time.perf_counter()
                try:
                    result = await client.execute_pipeline(
                        manifest=manifest,
                        audio_inputs=audio_inputs,
                        data_inputs={}
                    )
                    e2e_ms = (time.perf_counter() - t0) * 1000.0
                    results.append({
                        "target": "local_grpc",
                        "file": str(sample.path.name),
                        "repeat": rep,
                        "latency_ms": e2e_ms,
                        "status": "ok",
                        "details": ""
                    })
                except Exception as e:
                    e2e_ms = (time.perf_counter() - t0) * 1000.0
                    results.append({
                        "target": "local_grpc",
                        "file": str(sample.path.name),
                        "repeat": rep,
                        "latency_ms": e2e_ms,
                        "status": "error",
                        "details": str(e)
                    })

        tasks: List[asyncio.Task] = []
        for rep in range(repeats):
            for i, s in enumerate(samples):
                tasks.append(asyncio.create_task(run_one(s, i, rep)))
        await asyncio.gather(*tasks)
    return results


def summarize(results: List[Dict[str, Any]], target: str) -> Dict[str, Any]:
    vals = [r["latency_ms"] for r in results if r["target"] == target and r["status"] == "ok"]
    if not vals:
        return {"target": target, "count": 0, "p50_ms": None, "p95_ms": None, "p99_ms": None}
    vals.sort()
    def pct(p: float) -> float:
        k = max(0, min(len(vals)-1, int(round(p * (len(vals)-1)))))
        return vals[k]
    return {
        "target": target,
        "count": len(vals),
        "p50_ms": pct(0.50),
        "p95_ms": pct(0.95),
        "p99_ms": pct(0.99),
    }

async def bench_python_baseline(
    cmd_template: str,
    samples: List[Sample],
    concurrency: int,
    repeats: int,
    cwd: Optional[str],
    env_kv: List[str],
    use_shell: bool,
    timeout_s: float,
) -> List[Dict[str, Any]]:
    """
    Run a local Python (or any) command per audio file.
    The command template must include {audio} placeholder.
    Example: "python C:\\path\\to\\main.py --input {audio}"
    """
    sem = asyncio.Semaphore(concurrency)
    results: List[Dict[str, Any]] = []
    env = os.environ.copy()
    for kv in env_kv:
        if "=" in kv:
            k, v = kv.split("=", 1)
            env[k] = v

    async def run_one(sample: Sample, rep: int) -> None:
        async with sem:
            cmd_str = cmd_template.format(audio=str(sample.path))
            t0 = time.perf_counter()
            try:
                if use_shell:
                    proc = await asyncio.create_subprocess_shell(
                        cmd_str,
                        cwd=cwd or None,
                        env=env,
                        stdout=asyncio.subprocess.PIPE,
                        stderr=asyncio.subprocess.PIPE,
                    )
                else:
                    argv = shlex.split(cmd_str, posix=False)
                    proc = await asyncio.create_subprocess_exec(
                        *argv,
                        cwd=cwd or None,
                        env=env,
                        stdout=asyncio.subprocess.PIPE,
                        stderr=asyncio.subprocess.PIPE,
                    )
                try:
                    outs, errs = await asyncio.wait_for(proc.communicate(), timeout=timeout_s)
                except asyncio.TimeoutError:
                    proc.kill()
                    await proc.communicate()
                    e2e_ms = (time.perf_counter() - t0) * 1000.0
                    results.append({
                        "target": "python_baseline",
                        "file": str(sample.path.name),
                        "repeat": rep,
                        "latency_ms": e2e_ms,
                        "status": "timeout",
                        "details": ""
                    })
                    return

                e2e_ms = (time.perf_counter() - t0) * 1000.0
                ok = (proc.returncode == 0)
                details_src = (outs or b"") if ok else (errs or b"")
                details = details_src.decode(errors="ignore")[:200]
                results.append({
                    "target": "python_baseline",
                    "file": str(sample.path.name),
                    "repeat": rep,
                    "latency_ms": e2e_ms,
                    "status": "ok" if ok else f"rc_{proc.returncode}",
                    "details": details
                })
            except Exception as e:
                e2e_ms = (time.perf_counter() - t0) * 1000.0
                results.append({
                    "target": "python_baseline",
                    "file": str(sample.path.name),
                    "repeat": rep,
                    "latency_ms": e2e_ms,
                    "status": "error",
                    "details": str(e)[:200]
                })

    tasks: List[asyncio.Task] = []
    for rep in range(repeats):
        for s in samples:
            tasks.append(asyncio.create_task(run_one(s, rep)))
    await asyncio.gather(*tasks)
    return results


def write_csv(path: Path, rows: List[Dict[str, Any]]) -> None:
    if not rows:
        return
    keys = ["target", "file", "repeat", "latency_ms", "status", "details"]
    with path.open("w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=keys)
        w.writeheader()
        for r in rows:
            w.writerow({k: r.get(k, "") for k in keys})


async def main_async(args: argparse.Namespace) -> None:
    audio_dir = Path(args.audio_dir)
    files = discover_audio_files(audio_dir)
    if not files:
        print(f"No audio files found in {audio_dir}", file=sys.stderr)
        sys.exit(1)

    # Load samples
    samples: List[Sample] = []
    for p in files:
        b = read_audio_bytes(p)
        sr, ch = wav_meta_guess(p)
        samples.append(Sample(path=p, bytes=b, sample_rate=sr, channels=ch))

    all_rows: List[Dict[str, Any]] = []

    if args.server:
        print(f"Benchmarking local gRPC at {args.server} with concurrency={args.concurrency}, repeats={args.repeats}")
        rows = await bench_local_grpc(args.server, samples, args.concurrency, args.repeats)
        all_rows.extend(rows)
        s = summarize(all_rows, "local_grpc")
        print(f"local_grpc: n={s['count']} p50={s['p50_ms']}ms p95={s['p95_ms']}ms p99={s['p99_ms']}ms")


    if args.baseline_cmd:
        print(f"Benchmarking Python baseline cmd with concurrency={args.concurrency}, repeats={args.repeats}")
        rows = await bench_python_baseline(
            cmd_template=args.baseline_cmd,
            samples=samples,
            concurrency=args.concurrency,
            repeats=args.repeats,
            cwd=args.baseline_cwd,
            env_kv=args.baseline_env or [],
            use_shell=args.baseline_shell,
            timeout_s=args.baseline_timeout,
        )
        all_rows.extend(rows)
        s = summarize(all_rows, "python_baseline")
        print(f"python_baseline: n={s['count']} p50={s['p50_ms']}ms p95={s['p95_ms']}ms p99={s['p99_ms']}ms")

    if args.output:
        out = Path(args.output)
        out.parent.mkdir(parents=True, exist_ok=True)
        write_csv(out, all_rows)
        print(f"Wrote results to {out}")


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser(description="Benchmark local gRPC pipeline vs Python baseline")
    ap.add_argument("--audio-dir", required=True, help="Directory containing audio files")
    ap.add_argument("--server", default="", help="Local gRPC server address host:port (e.g., localhost:50051)")
    ap.add_argument("--concurrency", type=int, default=8, help="Concurrent requests")
    ap.add_argument("--repeats", type=int, default=3, help="Number of passes over the dataset")
    ap.add_argument("--output", default="", help="CSV output path")
    # Python baseline options
    ap.add_argument("--baseline-cmd", default="", help="Command template with {audio} placeholder to run per file")
    ap.add_argument("--baseline-cwd", default="", help="Working directory for baseline command")
    ap.add_argument("--baseline-env", action="append", default=[], help="Env vars for baseline, e.g. KEY=VAL (can repeat)")
    ap.add_argument("--baseline-shell", action="store_true", help="Run baseline command via shell")
    ap.add_argument("--baseline-timeout", type=float, default=600.0, help="Timeout per baseline run in seconds")
    return ap.parse_args()


def main() -> None:
    args = parse_args()
    asyncio.run(main_async(args))


if __name__ == "__main__":
    main()


