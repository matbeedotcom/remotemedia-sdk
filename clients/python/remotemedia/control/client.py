"""
Async gRPC client for the Session Control Bus.

Mirrors the Rust `SessionControl` surface over a single bidirectional
`PipelineControl.Attach` stream. Wire encoding is the same `ControlFrame`
/ `ControlEvent` protobuf used by both the gRPC and WebRTC servers.

This module is DATA-plane-agnostic: you can submit a pipeline via any
transport (gRPC streaming, WebRTC, or the FFI) and then open a control
attach to the same session over gRPC. The server keys lookups by
`session_id` in `SessionControlBus`.
"""

from __future__ import annotations

import asyncio
import enum
import logging
import re
from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Any, AsyncIterator, Callable, Dict, Optional, Union

import grpc

from remotemedia.protos import control_pb2 as _pb
from remotemedia.protos import control_pb2_grpc as _pbg
from remotemedia.protos import common_pb2 as _common_pb

logger = logging.getLogger(__name__)


# ─── Data wrapper ────────────────────────────────────────────────────────────


@dataclass
class Data:
    """Thin tagged wrapper around a protobuf `DataBuffer`.

    Constructors ensure the correct oneof variant is set without the caller
    having to touch the protobuf types directly.
    """

    _buf: _common_pb.DataBuffer

    # ─── Constructors ──
    # Named `from_*` to avoid colliding with the `.text` / `.json_value`
    # / `.binary_value` instance accessors below.
    @classmethod
    def from_text(cls, s: str) -> "Data":
        buf = _common_pb.DataBuffer()
        buf.text.text_data = s.encode("utf-8")
        return cls(buf)

    @classmethod
    def from_json(cls, value: Any) -> "Data":
        import json as _json
        buf = _common_pb.DataBuffer()
        buf.json.json_payload = _json.dumps(value)
        return cls(buf)

    @classmethod
    def from_bytes(cls, data: bytes) -> "Data":
        buf = _common_pb.DataBuffer()
        buf.binary.binary_data = data
        return cls(buf)

    # Backwards-friendly short aliases.
    text_of = from_text
    json_of = from_json
    bytes_of = from_bytes

    # ─── Accessors ──
    @property
    def kind(self) -> str:
        which = self._buf.WhichOneof("data_type")
        return which or "unset"

    @property
    def text_value(self) -> str:
        if self.kind != "text":
            raise AttributeError(f".text_value requires kind='text', got {self.kind!r}")
        return self._buf.text.text_data.decode("utf-8", errors="replace")

    # Shorthand: many consumers just want the text. Degrade gracefully for
    # JSON payloads that wrap a text field.
    @property
    def text(self) -> str:
        if self.kind == "text":
            return self.text_value
        if self.kind == "json":
            return self.json_value if isinstance(self.json_value, str) else str(self.json_value)
        if self.kind == "binary":
            try:
                return self._buf.binary.binary_data.decode("utf-8")
            except UnicodeDecodeError:
                return repr(self._buf.binary.binary_data)
        return ""

    @property
    def json_value(self) -> Any:
        if self.kind != "json":
            raise AttributeError(f".json_value requires kind='json', got {self.kind!r}")
        import json as _json
        return _json.loads(self._buf.json.json_payload)

    def as_buffer(self) -> _common_pb.DataBuffer:
        """Return the raw protobuf DataBuffer (escape hatch for binary types
        not wrapped here)."""
        return self._buf


# ─── Enums ───────────────────────────────────────────────────────────────────


class NodeState(enum.IntEnum):
    """Runtime execution state for a node. Mirrors Rust ``NodeState``."""

    UNSPECIFIED = _pb.NODE_STATE_UNSPECIFIED
    ENABLED = _pb.NODE_STATE_ENABLED
    BYPASS = _pb.NODE_STATE_BYPASS
    DISABLED = _pb.NODE_STATE_DISABLED


class _Direction(enum.IntEnum):
    IN = _pb.CONTROL_DIRECTION_IN
    OUT = _pb.CONTROL_DIRECTION_OUT


# ─── Errors ──────────────────────────────────────────────────────────────────


class SessionNotFoundError(RuntimeError):
    pass


class UnknownNodeError(RuntimeError):
    pass


class ControlAddressError(ValueError):
    pass


# ─── Address parsing ─────────────────────────────────────────────────────────


_ADDR_RE = re.compile(r"^(?P<node>[A-Za-z0-9_\-]+)\.(?P<dir>in|out)(?:\.(?P<port>[A-Za-z0-9_\-]+))?$")


def _parse_address(spec: str) -> _pb.ControlAddress:
    """Parse ``"node.in[.port]"`` / ``"node.out[.port]"`` into a protobuf
    ControlAddress. Node existence is not validated against the manifest
    here — the server will return ``UNKNOWN_NODE`` if needed."""
    match = _ADDR_RE.match(spec)
    if not match:
        raise ControlAddressError(
            f"invalid control address {spec!r}; expected 'node.in[.port]' or 'node.out[.port]'"
        )
    direction = _Direction.IN if match["dir"] == "in" else _Direction.OUT
    return _pb.ControlAddress(
        node_id=match["node"],
        port=match["port"] or "",
        direction=int(direction),
    )


# ─── Intercept request helper ────────────────────────────────────────────────


@dataclass
class InterceptRequest:
    """Passed to user intercept handlers. Call ``.pass_()`` / ``.replace(data)``
    / ``.drop()`` (or return one of those values from a decorator) to resolve."""

    data: Data
    _correlation_id: int
    _reply_fn: Callable[[int, _pb.InterceptDecision], "asyncio.Future[None]"]

    async def pass_through(self) -> None:
        decision = _pb.InterceptDecision(**{"pass": _pb.Empty()})
        await self._reply_fn(self._correlation_id, decision)

    async def replace(self, data: Data) -> None:
        decision = _pb.InterceptDecision(replace=data.as_buffer())
        await self._reply_fn(self._correlation_id, decision)

    async def drop(self) -> None:
        decision = _pb.InterceptDecision(drop=_pb.Empty())
        await self._reply_fn(self._correlation_id, decision)


# ─── Main attach handle ──────────────────────────────────────────────────────


class AttachedSession:
    """The client-side view of one ``PipelineControl.Attach`` stream.

    Construct via the :func:`attach` async context manager.
    """

    def __init__(self, session_id: str, stub: _pbg.PipelineControlStub, attach_id: str):
        self.session_id = session_id
        self.attach_id = attach_id
        self._stub = stub
        self._out_queue: asyncio.Queue[_pb.ControlFrame] = asyncio.Queue()
        self._subscribers: Dict[tuple, asyncio.Queue[Data]] = {}
        self._intercepts: Dict[tuple, asyncio.Queue[InterceptRequest]] = {}
        self._reader_task: Optional[asyncio.Task] = None
        self._closed_event = asyncio.Event()
        self._attached_event = asyncio.Event()
        self._attach_error: Optional[Exception] = None

    async def _send(self, frame: _pb.ControlFrame) -> None:
        await self._out_queue.put(frame)

    async def _frames(self) -> AsyncIterator[_pb.ControlFrame]:
        """Request stream: first Hello, then whatever the user queues."""
        yield _pb.ControlFrame(
            hello=_pb.Hello(session_id=self.session_id, attach_id=self.attach_id)
        )
        while True:
            frame = await self._out_queue.get()
            if frame is None:  # sentinel
                return
            yield frame

    async def _start(self) -> None:
        # Kick off the bidi RPC and spawn the reader.
        call = self._stub.Attach(self._frames())
        self._reader_task = asyncio.create_task(self._reader_loop(call))

        # Wait for either Attached or an Error.
        done, _ = await asyncio.wait(
            [
                asyncio.create_task(self._attached_event.wait()),
                asyncio.create_task(self._closed_event.wait()),
            ],
            return_when=asyncio.FIRST_COMPLETED,
            timeout=5.0,
        )
        if self._attach_error:
            raise self._attach_error
        if not self._attached_event.is_set():
            raise SessionNotFoundError(
                f"attach timed out or server closed stream before Attached: {self.session_id}"
            )

    async def _reader_loop(self, call) -> None:
        try:
            async for event in call:
                which = event.WhichOneof("event")
                if which == "attached":
                    self._attached_event.set()
                elif which == "tap":
                    key = (event.tap.addr.node_id, event.tap.addr.port)
                    q = self._subscribers.get(key)
                    if q is not None:
                        await q.put(Data(event.tap.data))
                elif which == "intercept_request":
                    key = (event.intercept_request.addr.node_id, event.intercept_request.addr.port)
                    q = self._intercepts.get(key)
                    if q is not None:
                        req = InterceptRequest(
                            data=Data(event.intercept_request.data),
                            _correlation_id=event.intercept_request.correlation_id,
                            _reply_fn=self._send_intercept_reply,
                        )
                        await q.put(req)
                elif which == "error":
                    code = event.error.code
                    msg = event.error.message
                    # SESSION_NOT_FOUND is fatal to the attach — raise.
                    if code == _pb.CONTROL_ERROR_CODE_SESSION_NOT_FOUND:
                        self._attach_error = SessionNotFoundError(msg)
                        self._closed_event.set()
                        return
                    if code == _pb.CONTROL_ERROR_CODE_UNKNOWN_NODE:
                        logger.warning("control bus: %s", msg)
                    else:
                        logger.warning("control bus error: %s (code=%d)", msg, code)
                elif which == "session_closed":
                    logger.info("control bus: session closed (reason=%d)", event.session_closed.reason)
                    self._closed_event.set()
                    return
        except grpc.aio.AioRpcError as e:
            if e.code() == grpc.StatusCode.NOT_FOUND:
                self._attach_error = SessionNotFoundError(e.details() or str(e))
            elif e.code() == grpc.StatusCode.CANCELLED:
                pass
            else:
                self._attach_error = e
            self._closed_event.set()
        finally:
            self._closed_event.set()

    async def _send_intercept_reply(
        self, correlation_id: int, decision: _pb.InterceptDecision
    ) -> None:
        await self._send(
            _pb.ControlFrame(
                intercept_reply=_pb.InterceptReply(
                    correlation_id=correlation_id, decision=decision
                )
            )
        )

    # ─── Public operations ──

    async def subscribe(self, address: str) -> AsyncIterator[Data]:
        """Async-iterator that yields node outputs as they arrive."""
        addr = _parse_address(address)
        if addr.direction != int(_Direction.OUT):
            raise ControlAddressError("subscribe requires '.out' address")
        key = (addr.node_id, addr.port)
        queue: asyncio.Queue[Data] = asyncio.Queue()
        self._subscribers[key] = queue
        await self._send(_pb.ControlFrame(subscribe=_pb.Subscribe(addr=addr)))

        async def _iter() -> AsyncIterator[Data]:
            try:
                while not self._closed_event.is_set() or not queue.empty():
                    get_task = asyncio.create_task(queue.get())
                    closed_task = asyncio.create_task(self._closed_event.wait())
                    done, _pending = await asyncio.wait(
                        [get_task, closed_task], return_when=asyncio.FIRST_COMPLETED
                    )
                    if get_task in done:
                        closed_task.cancel()
                        yield get_task.result()
                    else:
                        get_task.cancel()
                        break
            finally:
                self._subscribers.pop(key, None)
                try:
                    await self._send(
                        _pb.ControlFrame(unsubscribe=_pb.Unsubscribe(addr=addr))
                    )
                except Exception:
                    pass

        return _iter()

    async def publish(self, address: str, data: Union[Data, str]) -> None:
        """Inject data at a node's input."""
        addr = _parse_address(address)
        if addr.direction != int(_Direction.IN):
            raise ControlAddressError("publish requires '.in' address")
        if isinstance(data, str):
            data = Data.text(data)
        await self._send(
            _pb.ControlFrame(publish=_pb.Publish(addr=addr, data=data.as_buffer()))
        )

    @asynccontextmanager
    async def intercept(
        self, address: str, deadline_ms: int = 50
    ) -> AsyncIterator[AsyncIterator[InterceptRequest]]:
        """Context manager yielding an async iterator of ``InterceptRequest``s.

        Each request must be resolved via ``.pass_through()``, ``.replace(data)``,
        or ``.drop()``. If the handler doesn't reply within ``deadline_ms`` on
        the server side, the router forwards the original frame unchanged.
        """
        addr = _parse_address(address)
        if addr.direction != int(_Direction.OUT):
            raise ControlAddressError("intercept requires '.out' address")
        key = (addr.node_id, addr.port)
        queue: asyncio.Queue[InterceptRequest] = asyncio.Queue()
        self._intercepts[key] = queue
        await self._send(
            _pb.ControlFrame(
                intercept=_pb.Intercept(addr=addr, deadline_ms=deadline_ms)
            )
        )

        async def _iter() -> AsyncIterator[InterceptRequest]:
            while not self._closed_event.is_set() or not queue.empty():
                get_task = asyncio.create_task(queue.get())
                closed_task = asyncio.create_task(self._closed_event.wait())
                done, _pending = await asyncio.wait(
                    [get_task, closed_task], return_when=asyncio.FIRST_COMPLETED
                )
                if get_task in done:
                    closed_task.cancel()
                    yield get_task.result()
                else:
                    get_task.cancel()
                    break

        try:
            yield _iter()
        finally:
            self._intercepts.pop(key, None)
            try:
                await self._send(
                    _pb.ControlFrame(remove_intercept=_pb.RemoveIntercept(addr=addr))
                )
            except Exception:
                pass

    async def set_node_state(self, node_id: str, state: NodeState) -> None:
        """Flip a node's runtime execution state."""
        await self._send(
            _pb.ControlFrame(
                set_node_state=_pb.SetNodeState(node_id=node_id, state=int(state))
            )
        )

    async def clear_node_state(self, node_id: str) -> None:
        await self._send(
            _pb.ControlFrame(clear_node_state=_pb.ClearNodeState(node_id=node_id))
        )

    async def close(self) -> None:
        if self._closed_event.is_set():
            return
        await self._out_queue.put(None)  # type: ignore[arg-type]
        if self._reader_task is not None:
            try:
                await asyncio.wait_for(self._reader_task, timeout=2.0)
            except asyncio.TimeoutError:
                self._reader_task.cancel()


# ─── Entry point ─────────────────────────────────────────────────────────────


@asynccontextmanager
async def attach(
    address: str,
    *,
    session_id: str,
    attach_id: str = "python-client",
    credentials: Optional[grpc.ChannelCredentials] = None,
) -> AsyncIterator[AttachedSession]:
    """Open a control-plane attach to a running session.

    ``address`` is the gRPC server's ``host:port``. ``session_id`` must name
    a live session in the server's ``SessionControlBus``.
    """
    channel = (
        grpc.aio.secure_channel(address, credentials)
        if credentials is not None
        else grpc.aio.insecure_channel(address)
    )
    try:
        stub = _pbg.PipelineControlStub(channel)
        handle = AttachedSession(session_id=session_id, stub=stub, attach_id=attach_id)
        await handle._start()
        try:
            yield handle
        finally:
            await handle.close()
    finally:
        await channel.close()
