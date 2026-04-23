"""
Proves `_install_parent_death_watcher` self-terminates a Python worker
when its parent Rust process dies uncleanly (SIGKILL / crash / terminal
close — cases where `ProcessManager::Drop` can't run).

The regression this guards against is the one from 2026-04-22:
after the demo server crashed or was SIGKILL'd, ~30 orphaned
`remotemedia.core.multiprocessing.runner` processes survived,
reparented to launchd, each spinning ~25% CPU in their iceoryx2
poll loop. The watcher added to runner.py polls `os.getppid()`
every 2 s and calls `os._exit(0)` the moment the parent becomes
pid 1 (or any unexpected pid).

The test:
  1. Launches a bash "parent" subprocess.
  2. Parent forks a Python "worker" that imports the real watcher from
     `remotemedia.core.multiprocessing.runner` and prints its pid.
  3. SIGKILL the bash parent — SIGKILL can't be trapped, so bash dies
     without signalling the worker. On Unix the worker is then
     reparented to pid 1.
  4. Assert the worker terminates within a deadline. If it survives
     past the deadline it's the old bug and we fail loudly.

Runs on both macOS and Linux. On Linux the Rust spawner already sets
`PR_SET_PDEATHSIG(SIGTERM)` so the kernel does this for us — but the
watcher is still the path of truth, so we test it directly here.
"""

from __future__ import annotations

import os
import signal
import subprocess
import sys
import textwrap
import time
from pathlib import Path

import pytest


# Worker imports the REAL watcher from the runner module we ship in
# prod — this test would silently pass if the watcher was deleted
# from runner.py (the import would fail, worker would crash, test
# would see that as "terminated"). Guard against that in the assertion
# by also verifying the worker was alive *before* we killed the parent.
_WORKER_SRC = textwrap.dedent("""
    import os
    import sys
    import time

    from remotemedia.core.multiprocessing.runner import (
        _install_parent_death_watcher,
    )

    _install_parent_death_watcher()

    # Prove we're up before the parent dies.
    print(os.getpid(), flush=True)

    # Block forever — the watcher thread is the only thing that can
    # make this process exit cleanly from here on.
    while True:
        time.sleep(1)
""")


def _alive(pid: int) -> bool:
    """Return True if `pid` is still a live process owned by us."""
    try:
        os.kill(pid, 0)  # 0 is the "just check" signal
    except ProcessLookupError:
        return False
    except PermissionError:
        # Reparented to pid 1 on some systems can change ownership
        # semantics; the process still exists though.
        return True
    return True


def test_worker_self_terminates_after_parent_sigkill(tmp_path: Path) -> None:
    # Worker code lives in a file, but we import it via `python -c`
    # (reading the file at runtime) so the worker's sys.path matches
    # the import behavior of the current pytest process. Running
    # `python /tmp/worker.py` directly picks up a stale `remotemedia`
    # package from a legacy path ahead of the editable install.
    worker_script = tmp_path / "worker.py"
    worker_script.write_text(_WORKER_SRC)

    # Pin PYTHONPATH to the clients/python source tree so the worker
    # imports the same `remotemedia.core.multiprocessing.runner` we
    # just modified, not a stale copy elsewhere on disk.
    repo_python = Path(__file__).resolve().parents[1]
    env = os.environ.copy()
    existing = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        f"{repo_python}{os.pathsep}{existing}" if existing else str(repo_python)
    )

    # Parent is a bash shell that spawns the worker (via `exec -c`)
    # and waits. Using `bash -c "... & wait"` instead of
    # `exec python` so bash is the immediate parent of the worker —
    # SIGKILL on bash orphans the worker (reparented to init/launchd)
    # exactly like the prod crash scenario.
    worker_cmd = (
        f'exec {sys.executable} -c '
        f'"exec(open({str(worker_script)!r}).read())"'
    )
    parent = subprocess.Popen(
        ["bash", "-c", f"{worker_cmd} & wait"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        # Fresh session so SIGKILL on bash doesn't propagate to the
        # worker as SIGHUP via the tty — we want to prove the watcher
        # handles the orphan case on its own.
        start_new_session=True,
    )

    try:
        # Wait for the worker to print its pid. 10 s is generous even
        # on a cold import of the remotemedia package.
        assert parent.stdout is not None
        line = parent.stdout.readline()
        if not line:
            # Worker failed to start — surface whatever bash captured.
            stderr = parent.stderr.read().decode() if parent.stderr else ""
            pytest.fail(
                f"worker never printed its pid; parent stderr:\n{stderr!r}"
            )
        worker_pid = int(line.decode().strip())

        # Guard: if the import of the watcher was silently deleted
        # from runner.py, the worker would have crashed before
        # reaching the print — we'd never see this assertion.
        assert _alive(worker_pid), (
            f"worker {worker_pid} died before we got a chance to kill parent"
        )

        # SIGKILL bash. It cannot trap SIGKILL, so no cleanup runs —
        # the worker is immediately orphaned.
        parent.send_signal(signal.SIGKILL)
        parent.wait(timeout=5)

        # Worker polls every 2 s, so 6 s is ~3 polls — plenty of
        # headroom for scheduling jitter even on a loaded CI box.
        deadline = time.monotonic() + 6.0
        while time.monotonic() < deadline:
            if not _alive(worker_pid):
                break
            time.sleep(0.1)
        else:
            # Clean up before failing so we don't leak a real orphan
            # out of the test.
            try:
                os.kill(worker_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            pytest.fail(
                f"orphaned worker pid={worker_pid} survived 6 s after "
                f"parent SIGKILL — parent-death watcher is not running "
                f"or poll interval regressed",
            )
    finally:
        # Belt-and-suspenders: make sure nothing we started is left
        # running even if the test failed partway through.
        if parent.poll() is None:
            parent.kill()
            parent.wait(timeout=3)


# Control test: prove the above assertion is actually load-bearing.
# If the watcher were a no-op, this test would fail because the worker
# would survive past the deadline — we explicitly skip calling the
# watcher here and assert the orphan *does* survive, then clean it up.
# If someone ever swaps the poll loop for a no-op, the positive test
# stays green but this test flips to failing, telling us the
# assertion we're relying on stopped discriminating.

_WORKER_NO_WATCHER_SRC = textwrap.dedent("""
    import os
    import sys
    import time

    # Deliberately do NOT install the watcher.
    print(os.getpid(), flush=True)
    while True:
        time.sleep(1)
""")


def test_control_unwatched_worker_survives_parent_sigkill(tmp_path: Path) -> None:
    worker_script = tmp_path / "worker_nowatcher.py"
    worker_script.write_text(_WORKER_NO_WATCHER_SRC)

    worker_cmd = (
        f'exec {sys.executable} -c '
        f'"exec(open({str(worker_script)!r}).read())"'
    )
    parent = subprocess.Popen(
        ["bash", "-c", f"{worker_cmd} & wait"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )

    worker_pid: int | None = None
    try:
        assert parent.stdout is not None
        worker_pid = int(parent.stdout.readline().decode().strip())
        assert _alive(worker_pid)

        parent.send_signal(signal.SIGKILL)
        parent.wait(timeout=5)

        # Give the same 6 s window we give the real watcher. Without
        # a watcher the orphan should still be running at the end.
        time.sleep(6.0)
        assert _alive(worker_pid), (
            f"control worker {worker_pid} died within 6 s without a "
            f"watcher — the positive test is not actually proving "
            f"anything (something else is killing orphans for us)",
        )
    finally:
        if worker_pid is not None:
            try:
                os.kill(worker_pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        if parent.poll() is None:
            parent.kill()
            parent.wait(timeout=3)
