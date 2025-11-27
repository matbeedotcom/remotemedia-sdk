"""Response models."""

from typing import Any, Optional

from pydantic import BaseModel, Field


class HealthResponse(BaseModel):
    """Health check response."""

    status: str = Field(..., description="Service status (healthy, degraded, unhealthy)")
    version: str = Field(..., description="API version")
    pipelines_loaded: int = Field(..., description="Number of loaded pipelines")
    active_sessions: int = Field(..., description="Number of active streaming sessions")
    runtime_available: bool = Field(..., description="Whether the runtime is available")


class PipelineInfo(BaseModel):
    """Pipeline information."""

    name: str = Field(..., description="Pipeline name")
    description: str = Field(..., description="Pipeline description")
    version: str = Field(..., description="Pipeline version")
    input_type: str = Field(..., description="Expected input type")
    output_type: str = Field(..., description="Produced output type")
    streaming: bool = Field(..., description="Whether pipeline supports streaming")
    nodes: Optional[list[dict[str, Any]]] = Field(
        None, description="Pipeline nodes (only in detail view)"
    )
    connections: Optional[list[dict[str, Any]]] = Field(
        None, description="Node connections (only in detail view)"
    )


class PipelineListResponse(BaseModel):
    """List of pipelines response."""

    pipelines: list[PipelineInfo] = Field(..., description="Available pipelines")


class PredictResponse(BaseModel):
    """Prediction response."""

    output_type: str = Field(..., description="Type of output data")
    output_data: Optional[str] = Field(
        None, description="Base64 encoded output data"
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict, description="Additional metadata"
    )
    timing_ms: float = Field(..., description="Execution time in milliseconds")


class StreamSessionResponse(BaseModel):
    """Streaming session response."""

    session_id: str = Field(..., description="Unique session identifier")
    pipeline: str = Field(..., description="Pipeline being executed")
    status: str = Field(..., description="Session status")


class ErrorResponse(BaseModel):
    """Error response."""

    code: str = Field(..., description="Error code")
    message: str = Field(..., description="Human-readable error message")
    recoverable: bool = Field(..., description="Whether the error is recoverable")
    suggestion: Optional[str] = Field(None, description="Suggested action")
