"""
Session management for multiprocess Python nodes.

This module provides the Session class for managing pipeline execution
with progress tracking and initialization monitoring.
"""

from enum import Enum
from typing import Dict, List, Optional, Callable
from dataclasses import dataclass
import time


class SessionStatus(Enum):
    """Session execution status."""
    INITIALIZING = "initializing"
    READY = "ready"
    RUNNING = "running"
    TERMINATING = "terminating"
    TERMINATED = "terminated"
    ERROR = "error"


class InitStatus(Enum):
    """Node initialization status."""
    STARTING = "starting"
    LOADING_MODEL = "loading_model"
    CONNECTING = "connecting"
    READY = "ready"
    FAILED = "failed"


@dataclass
class InitProgress:
    """Initialization progress information for a node."""

    node_id: str
    """Node identifier"""

    node_type: str
    """Node type"""

    status: InitStatus
    """Current initialization status"""

    progress: float
    """Progress percentage (0.0 to 1.0)"""

    message: str
    """Human-readable progress message"""

    timestamp: float
    """Timestamp of this progress update"""


class Session:
    """
    Pipeline session for managing multiprocess node execution.

    A session represents a single pipeline execution context with
    multiple Python nodes running in separate processes.

    Example:
        ```python
        from remotemedia.core.multiprocessing import Session

        # Create a new session
        session = Session("my_pipeline_session")

        # Wait for all nodes to initialize (with progress tracking)
        def on_progress(progress: InitProgress):
            print(f"{progress.node_id}: {progress.message} ({progress.progress * 100}%)")

        session.wait_for_initialization(
            timeout_secs=30,
            progress_callback=on_progress
        )

        # Run the pipeline
        session.run()

        # Cleanup
        session.terminate()
        ```
    """

    def __init__(self, session_id: str):
        """
        Create a new session.

        Args:
            session_id: Unique identifier for this session
        """
        self.session_id = session_id
        self.status = SessionStatus.INITIALIZING
        self._init_progress: Dict[str, InitProgress] = {}
        self._progress_callbacks: List[Callable[[InitProgress], None]] = []
        self._created_at = time.time()

    def wait_for_initialization(
        self,
        timeout_secs: float = 30.0,
        progress_callback: Optional[Callable[[InitProgress], None]] = None,
        poll_interval_ms: float = 100.0
    ) -> bool:
        """
        Wait for all nodes in the session to complete initialization.

        Args:
            timeout_secs: Maximum time to wait for initialization (default: 30s)
            progress_callback: Optional callback for progress updates
            poll_interval_ms: Polling interval in milliseconds (default: 100ms)

        Returns:
            True if initialization completed successfully, False on timeout

        Raises:
            RuntimeError: If any node fails to initialize

        Example:
            ```python
            def on_progress(progress):
                print(f"{progress.node_id}: {progress.message}")

            if session.wait_for_initialization(progress_callback=on_progress):
                print("All nodes ready!")
            else:
                print("Initialization timeout")
            ```
        """
        if progress_callback:
            self.add_progress_callback(progress_callback)

        start_time = time.time()
        poll_interval = poll_interval_ms / 1000.0

        while True:
            # Check timeout
            elapsed = time.time() - start_time
            if elapsed > timeout_secs:
                return False

            # Check if any nodes failed
            for node_id, progress in self._init_progress.items():
                if progress.status == InitStatus.FAILED:
                    raise RuntimeError(
                        f"Node {node_id} failed to initialize: {progress.message}"
                    )

            # Check if all nodes are ready
            if self._init_progress and all(
                p.status == InitStatus.READY
                for p in self._init_progress.values()
            ):
                self.status = SessionStatus.READY
                return True

            # Wait before checking again
            time.sleep(poll_interval)

    def update_init_progress(
        self,
        node_id: str,
        node_type: str,
        status: InitStatus,
        progress: float,
        message: str
    ) -> None:
        """
        Update initialization progress for a node.

        Args:
            node_id: Node identifier
            node_type: Node type
            status: Current initialization status
            progress: Progress percentage (0.0 to 1.0)
            message: Human-readable progress message
        """
        progress_info = InitProgress(
            node_id=node_id,
            node_type=node_type,
            status=status,
            progress=max(0.0, min(1.0, progress)),  # Clamp to [0, 1]
            message=message,
            timestamp=time.time()
        )

        self._init_progress[node_id] = progress_info

        # Notify callbacks
        for callback in self._progress_callbacks:
            try:
                callback(progress_info)
            except Exception as e:
                print(f"Error in progress callback: {e}")

    def get_init_progress(self) -> List[InitProgress]:
        """
        Get initialization progress for all nodes.

        Returns:
            List of initialization progress for each node
        """
        return list(self._init_progress.values())

    def add_progress_callback(
        self,
        callback: Callable[[InitProgress], None]
    ) -> None:
        """
        Add a progress callback function.

        The callback will be called whenever node initialization progress is updated.

        Args:
            callback: Function that takes an InitProgress parameter
        """
        self._progress_callbacks.append(callback)

    def remove_progress_callback(
        self,
        callback: Callable[[InitProgress], None]
    ) -> None:
        """
        Remove a progress callback function.

        Args:
            callback: Callback function to remove
        """
        if callback in self._progress_callbacks:
            self._progress_callbacks.remove(callback)

    def run(self) -> None:
        """
        Start running the pipeline.

        Raises:
            RuntimeError: If session is not in READY status
        """
        if self.status != SessionStatus.READY:
            raise RuntimeError(
                f"Cannot run session in {self.status.value} status. "
                "Must be in READY status."
            )

        self.status = SessionStatus.RUNNING

    def terminate(self) -> None:
        """
        Terminate the session and cleanup resources.
        """
        self.status = SessionStatus.TERMINATING
        # Cleanup will be handled by the Rust runtime
        self.status = SessionStatus.TERMINATED

    def get_status(self) -> SessionStatus:
        """
        Get the current session status.

        Returns:
            Current session status
        """
        return self.status

    def get_uptime(self) -> float:
        """
        Get session uptime in seconds.

        Returns:
            Seconds since session creation
        """
        return time.time() - self._created_at
