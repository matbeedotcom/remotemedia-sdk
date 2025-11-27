"""Health check endpoint."""

from fastapi import APIRouter, Request

from inference_api.models.responses import HealthResponse

router = APIRouter()


@router.get("/health", response_model=HealthResponse)
async def health_check(request: Request) -> HealthResponse:
    """Check API health status.

    Returns information about service status, available pipelines,
    and runtime availability.
    """
    registry = request.app.state.pipeline_registry
    session_manager = request.app.state.session_manager

    # Check if runtime is available
    runtime_available = True  # TODO: Actually check FFI availability

    return HealthResponse(
        status="healthy" if runtime_available else "degraded",
        version="0.1.0",
        pipelines_loaded=len(registry.list_pipelines()),
        active_sessions=session_manager.active_count(),
        runtime_available=runtime_available,
    )
