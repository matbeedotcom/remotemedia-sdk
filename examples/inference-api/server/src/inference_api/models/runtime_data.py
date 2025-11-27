"""RuntimeData conversion utilities."""

import base64
from dataclasses import dataclass
from typing import Any, Optional


@dataclass
class RuntimeData:
    """Wrapper for runtime data with type information."""

    data_type: str
    data: bytes
    metadata: dict[str, Any]


def from_base64(
    encoded: str,
    data_type: str = "audio",
    metadata: Optional[dict[str, Any]] = None,
) -> RuntimeData:
    """Create RuntimeData from base64 encoded string."""
    return RuntimeData(
        data_type=data_type,
        data=base64.b64decode(encoded),
        metadata=metadata or {},
    )


def to_base64(runtime_data: RuntimeData) -> str:
    """Convert RuntimeData to base64 encoded string."""
    return base64.b64encode(runtime_data.data).decode()


def audio_from_bytes(
    data: bytes,
    sample_rate: int = 16000,
    channels: int = 1,
    format: str = "f32",
) -> RuntimeData:
    """Create audio RuntimeData from raw bytes."""
    return RuntimeData(
        data_type="audio",
        data=data,
        metadata={
            "sample_rate": sample_rate,
            "channels": channels,
            "format": format,
        },
    )


def text_from_string(text: str) -> RuntimeData:
    """Create text RuntimeData from string."""
    return RuntimeData(
        data_type="text",
        data=text.encode("utf-8"),
        metadata={},
    )


def text_to_string(runtime_data: RuntimeData) -> str:
    """Extract string from text RuntimeData."""
    if runtime_data.data_type != "text":
        raise ValueError(f"Expected text, got {runtime_data.data_type}")
    return runtime_data.data.decode("utf-8")
