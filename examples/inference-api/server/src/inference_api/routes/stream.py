"""Streaming endpoints with SSE."""

import base64
from typing import AsyncGenerator

from fastapi import APIRouter, HTTPException, Request
from sse_starlette.sse import EventSourceResponse

from inference_api.models.requests import StreamInputRequest, StreamStartRequest
from inference_api.models.responses import StreamSessionResponse
from inference_api.services.sessions import StreamSession

router = APIRouter()


@router.post("", response_model=StreamSessionResponse)
async def start_stream(request: Request, body: StreamStartRequest) -> StreamSessionResponse:
    """Start a new streaming session.

    Creates a session for bidirectional streaming. Use the returned
    session_id for subsequent input/output operations.
    """
    registry = request.app.state.pipeline_registry
    session_manager = request.app.state.session_manager

    pipeline = registry.get_pipeline(body.pipeline)
    if pipeline is None:
        raise HTTPException(status_code=404, detail=f"Pipeline '{body.pipeline}' not found")

    # Create session
    session = await session_manager.create_session(pipeline, body.config)

    return StreamSessionResponse(
        session_id=session.session_id,
        pipeline=body.pipeline,
        status="active",
    )


@router.post("/{session_id}/input")
async def send_stream_input(
    session_id: str,
    request: Request,
    body: StreamInputRequest,
) -> dict:
    """Send input data to a streaming session.

    Input data should be base64 encoded if binary.
    """
    session_manager = request.app.state.session_manager
    session = session_manager.get_session(session_id)

    if session is None:
        raise HTTPException(status_code=404, detail=f"Session '{session_id}' not found")

    # Decode input
    input_data = base64.b64decode(body.input_data) if body.input_data else None

    # Send to session
    await session.send_input(input_data, body.input_type)

    return {"status": "accepted"}


@router.get("/{session_id}/output")
async def stream_output(session_id: str, request: Request) -> EventSourceResponse:
    """Stream outputs from a session via Server-Sent Events.

    Returns an SSE stream with output events. Each event contains:
    - event: output type (e.g., "transcription", "audio", "text")
    - data: JSON with output_type, output_data (base64), metadata
    """
    session_manager = request.app.state.session_manager
    session = session_manager.get_session(session_id)

    if session is None:
        raise HTTPException(status_code=404, detail=f"Session '{session_id}' not found")

    async def generate_events() -> AsyncGenerator[dict, None]:
        async for output in session.stream_outputs():
            # Encode binary data
            output_data = (
                base64.b64encode(output.data).decode() if output.data else None
            )

            yield {
                "event": output.output_type,
                "data": {
                    "output_type": output.output_type,
                    "output_data": output_data,
                    "metadata": output.metadata,
                    "timestamp_ms": output.timestamp_ms,
                },
            }

    return EventSourceResponse(generate_events())


@router.delete("/{session_id}")
async def close_stream(session_id: str, request: Request) -> dict:
    """Close a streaming session.

    Stops the pipeline and releases resources.
    """
    session_manager = request.app.state.session_manager

    if not session_manager.has_session(session_id):
        raise HTTPException(status_code=404, detail=f"Session '{session_id}' not found")

    await session_manager.close_session(session_id)

    return {"status": "closed", "session_id": session_id}
