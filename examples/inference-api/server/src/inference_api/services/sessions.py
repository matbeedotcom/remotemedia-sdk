"""Session manager for streaming pipelines."""

import asyncio
import logging
import time
import uuid
from dataclasses import dataclass, field
from typing import Any, AsyncGenerator, Optional

logger = logging.getLogger(__name__)


@dataclass
class StreamOutput:
    """Output from a streaming session."""

    output_type: str
    data: Optional[bytes]
    metadata: dict[str, Any]
    timestamp_ms: int


@dataclass
class StreamSession:
    """A streaming session."""

    session_id: str
    pipeline: dict[str, Any]
    config: Optional[dict[str, Any]]
    created_at: float = field(default_factory=time.time)

    _input_queue: asyncio.Queue = field(default_factory=asyncio.Queue)
    _output_queue: asyncio.Queue = field(default_factory=asyncio.Queue)
    _closed: bool = field(default=False)

    async def send_input(
        self, data: Optional[bytes], input_type: str = "audio"
    ) -> None:
        """Send input data to the session."""
        if self._closed:
            raise RuntimeError("Session is closed")

        await self._input_queue.put((data, input_type))

        # TODO: Process through actual pipeline
        # For now, simulate output
        output = StreamOutput(
            output_type="text",
            data=b"[Simulated streaming output]",
            metadata={"input_type": input_type},
            timestamp_ms=int(time.time() * 1000),
        )
        await self._output_queue.put(output)

    async def stream_outputs(self) -> AsyncGenerator[StreamOutput, None]:
        """Stream outputs from the session."""
        while not self._closed:
            try:
                output = await asyncio.wait_for(
                    self._output_queue.get(), timeout=30.0
                )
                yield output
            except asyncio.TimeoutError:
                # Send keepalive
                continue
            except Exception:
                break

    async def close(self) -> None:
        """Close the session."""
        self._closed = True
        # Clear queues
        while not self._input_queue.empty():
            self._input_queue.get_nowait()
        while not self._output_queue.empty():
            self._output_queue.get_nowait()


class SessionManager:
    """Manages streaming sessions."""

    def __init__(self) -> None:
        self._sessions: dict[str, StreamSession] = {}

    async def create_session(
        self,
        pipeline: dict[str, Any],
        config: Optional[dict[str, Any]] = None,
    ) -> StreamSession:
        """Create a new streaming session."""
        session_id = str(uuid.uuid4())

        session = StreamSession(
            session_id=session_id,
            pipeline=pipeline,
            config=config,
        )

        self._sessions[session_id] = session
        logger.info(f"Created session: {session_id}")

        return session

    def get_session(self, session_id: str) -> Optional[StreamSession]:
        """Get a session by ID."""
        return self._sessions.get(session_id)

    def has_session(self, session_id: str) -> bool:
        """Check if a session exists."""
        return session_id in self._sessions

    async def close_session(self, session_id: str) -> None:
        """Close and remove a session."""
        session = self._sessions.pop(session_id, None)
        if session:
            await session.close()
            logger.info(f"Closed session: {session_id}")

    async def close_all(self) -> None:
        """Close all sessions."""
        for session_id in list(self._sessions.keys()):
            await self.close_session(session_id)

    def active_count(self) -> int:
        """Get the number of active sessions."""
        return len(self._sessions)
