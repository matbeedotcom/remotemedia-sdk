"""
Python client for the RemoteMedia Session Control Bus.

Opens a gRPC bidirectional `PipelineControl.Attach` stream to a running
pipeline session and exposes an idiomatic async surface for the four
operations:

  - ``subscribe(address)``   — async iterator of node outputs (tap)
  - ``publish(address, data)`` — inject data at a node's input
  - ``intercept(address, ...)`` — edit / drop a node's output
  - ``set_node_state(node, state)`` — enable / bypass / disable a node

Usage::

    import asyncio
    from remotemedia.control import attach, NodeState, Data

    async def main():
        async with attach("localhost:50051", session_id="sess-1") as ctrl:
            # tap transcripts, inject RAG context
            async for turn in ctrl.subscribe("whisper.out"):
                docs = await vectordb.search(turn.text)
                await ctrl.publish("llm.in.context", Data.text("\\n".join(docs)))

See ``docs/SESSION_CONTROL.md`` for the semantic model.
"""

from .client import (
    AttachedSession,
    Data,
    NodeState,
    UnknownNodeError,
    SessionNotFoundError,
    attach,
)

__all__ = [
    "AttachedSession",
    "Data",
    "NodeState",
    "UnknownNodeError",
    "SessionNotFoundError",
    "attach",
]
