"""Request models."""

from typing import Any, Optional

from pydantic import BaseModel, Field


class PredictRequest(BaseModel):
    """Request for unary prediction."""

    pipeline: str = Field(..., description="Name of the pipeline to execute")
    input_data: Optional[str] = Field(
        None, description="Base64 encoded input data"
    )
    input_type: str = Field(
        "audio", description="Type of input data (audio, text, image)"
    )
    config: Optional[dict[str, Any]] = Field(
        None, description="Optional pipeline configuration overrides"
    )


class StreamStartRequest(BaseModel):
    """Request to start a streaming session."""

    pipeline: str = Field(..., description="Name of the pipeline to execute")
    config: Optional[dict[str, Any]] = Field(
        None, description="Optional pipeline configuration overrides"
    )


class StreamInputRequest(BaseModel):
    """Request to send input to a streaming session."""

    input_data: Optional[str] = Field(
        None, description="Base64 encoded input data"
    )
    input_type: str = Field(
        "audio", description="Type of input data (audio, text, image)"
    )
