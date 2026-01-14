"""
Type-safe Data Types for Generic Streaming Protocol (Feature 004)

Provides static type hints for RemoteMedia SDK's universal streaming protocol.
Compatible with mypy and other Python type checkers.

Supports 6 data types: Audio, Video, Tensor, JSON, Text, Binary
"""

from typing import Union, Literal, TypedDict, Optional, Dict, List, Any, TypeGuard, cast
from enum import Enum
from dataclasses import dataclass


# ============================================================================
# Enums
# ============================================================================

class AudioFormat(Enum):
    """Audio sample format"""
    F32 = "F32"
    I16 = "I16"
    I32 = "I32"


class PixelFormat(Enum):
    """Video pixel format"""
    RGB24 = "RGB24"
    RGBA32 = "RGBA32"
    YUV420P = "YUV420P"
    GRAY8 = "GRAY8"


class TensorDtype(Enum):
    """Tensor data type"""
    F32 = "F32"
    F16 = "F16"
    I32 = "I32"
    I8 = "I8"
    U8 = "U8"


class DataTypeHint(Enum):
    """Data type hint for validation"""
    AUDIO = "AUDIO"
    VIDEO = "VIDEO"
    TENSOR = "TENSOR"
    JSON = "JSON"
    TEXT = "TEXT"
    BINARY = "BINARY"
    ANY = "ANY"


# ============================================================================
# Data Classes
# ============================================================================

@dataclass
class AudioBuffer:
    """Audio buffer with multi-channel PCM data"""
    samples: bytes
    sample_rate: int
    channels: int
    format: AudioFormat
    num_samples: int


@dataclass
class VideoFrame:
    """Video frame with pixel data"""
    pixel_data: bytes
    width: int
    height: int
    format: PixelFormat
    frame_number: int
    timestamp_us: int


@dataclass
class TensorBuffer:
    """Multi-dimensional tensor buffer"""
    data: bytes
    shape: List[int]
    dtype: TensorDtype
    layout: Optional[str] = None


@dataclass
class JsonData:
    """JSON data with optional schema"""
    json_payload: str
    schema_type: Optional[str] = None


@dataclass
class TextBuffer:
    """Text buffer with encoding info"""
    text_data: bytes
    encoding: Optional[str] = None
    language: Optional[str] = None


@dataclass
class BinaryBuffer:
    """Binary data with MIME type"""
    data: bytes
    mime_type: Optional[str] = None


# ============================================================================
# TypedDict Definitions (for discriminated unions)
# ============================================================================

class AudioBufferDict(TypedDict):
    """Typed dictionary for audio buffer"""
    type: Literal["audio"]
    data: AudioBuffer
    metadata: Optional[Dict[str, str]]


class VideoFrameDict(TypedDict):
    """Typed dictionary for video frame"""
    type: Literal["video"]
    data: VideoFrame
    metadata: Optional[Dict[str, str]]


class TensorBufferDict(TypedDict):
    """Typed dictionary for tensor buffer"""
    type: Literal["tensor"]
    data: TensorBuffer
    metadata: Optional[Dict[str, str]]


class JsonDataDict(TypedDict):
    """Typed dictionary for JSON data"""
    type: Literal["json"]
    data: JsonData
    metadata: Optional[Dict[str, str]]


class TextBufferDict(TypedDict):
    """Typed dictionary for text buffer"""
    type: Literal["text"]
    data: TextBuffer
    metadata: Optional[Dict[str, str]]


class BinaryBufferDict(TypedDict):
    """Typed dictionary for binary buffer"""
    type: Literal["binary"]
    data: BinaryBuffer
    metadata: Optional[Dict[str, str]]


# ============================================================================
# Discriminated Union
# ============================================================================

DataBuffer = Union[
    AudioBufferDict,
    VideoFrameDict,
    TensorBufferDict,
    JsonDataDict,
    TextBufferDict,
    BinaryBufferDict,
]
"""
Discriminated union for all data buffer types.

Type checkers can narrow the type based on the 'type' field.

Example:
    >>> buffer: DataBuffer = create_audio_buffer()
    >>> if buffer["type"] == "audio":
    ...     # Type checker knows buffer["data"] is AudioBuffer
    ...     print(f"Sample rate: {buffer['data'].sample_rate}")
"""


@dataclass
class DataChunk:
    """Data chunk for streaming with optional named buffers"""
    node_id: str
    sequence: int
    timestamp_ms: int
    buffer: Optional[DataBuffer] = None
    named_buffers: Optional[Dict[str, DataBuffer]] = None


# ============================================================================
# Type Guards
# ============================================================================

def is_audio_buffer(buffer: DataBuffer) -> TypeGuard[AudioBufferDict]:
    """Type guard: Check if buffer is AudioBuffer"""
    return buffer["type"] == "audio"


def is_video_frame(buffer: DataBuffer) -> TypeGuard[VideoFrameDict]:
    """Type guard: Check if buffer is VideoFrame"""
    return buffer["type"] == "video"


def is_tensor_buffer(buffer: DataBuffer) -> TypeGuard[TensorBufferDict]:
    """Type guard: Check if buffer is TensorBuffer"""
    return buffer["type"] == "tensor"


def is_json_data(buffer: DataBuffer) -> TypeGuard[JsonDataDict]:
    """Type guard: Check if buffer is JsonData"""
    return buffer["type"] == "json"


def is_text_buffer(buffer: DataBuffer) -> TypeGuard[TextBufferDict]:
    """Type guard: Check if buffer is TextBuffer"""
    return buffer["type"] == "text"


def is_binary_buffer(buffer: DataBuffer) -> TypeGuard[BinaryBufferDict]:
    """Type guard: Check if buffer is BinaryBuffer"""
    return buffer["type"] == "binary"


# ============================================================================
# Extract Helpers
# ============================================================================

def extract_audio_data(buffer: DataBuffer) -> Optional[AudioBuffer]:
    """Extract AudioBuffer if buffer is audio type"""
    return buffer["data"] if is_audio_buffer(buffer) else None


def extract_video_data(buffer: DataBuffer) -> Optional[VideoFrame]:
    """Extract VideoFrame if buffer is video type"""
    return buffer["data"] if is_video_frame(buffer) else None


def extract_tensor_data(buffer: DataBuffer) -> Optional[TensorBuffer]:
    """Extract TensorBuffer if buffer is tensor type"""
    return buffer["data"] if is_tensor_buffer(buffer) else None


def extract_json_data(buffer: DataBuffer) -> Optional[JsonData]:
    """Extract JsonData if buffer is JSON type"""
    return buffer["data"] if is_json_data(buffer) else None


def extract_text_data(buffer: DataBuffer) -> Optional[TextBuffer]:
    """Extract TextBuffer if buffer is text type"""
    return buffer["data"] if is_text_buffer(buffer) else None


def extract_binary_data(buffer: DataBuffer) -> Optional[BinaryBuffer]:
    """Extract BinaryBuffer if buffer is binary type"""
    return buffer["data"] if is_binary_buffer(buffer) else None


# ============================================================================
# Pipeline Types
# ============================================================================

@dataclass
class TypedNodeManifest:
    """Node manifest with type constraints"""
    id: str
    node_type: str
    is_streaming: bool
    input_types: List[DataTypeHint]
    output_types: List[DataTypeHint]
    params: Optional[str] = None
    capabilities: Optional[Dict] = None
    host: Optional[str] = None
    runtime_hint: int = 0


@dataclass
class TypedPipelineManifest:
    """Pipeline manifest with type safety"""
    version: str
    metadata: Dict[str, str]
    nodes: List[TypedNodeManifest]
    connections: List[Dict[str, Any]]


@dataclass
class StreamResult:
    """Type-safe streaming result"""
    sequence: int
    data_outputs: Dict[str, DataBuffer]
    processing_time_ms: float
    total_items_processed: int


# ============================================================================
# Validation
# ============================================================================

class TypeValidationError(Exception):
    """Type validation error"""
    def __init__(
        self,
        message: str,
        expected: DataTypeHint,
        actual: DataTypeHint,
        node_id: str
    ):
        super().__init__(message)
        self.expected = expected
        self.actual = actual
        self.node_id = node_id


def buffer_to_type_hint(buffer: DataBuffer) -> DataTypeHint:
    """Convert DataBuffer to DataTypeHint"""
    type_map = {
        "audio": DataTypeHint.AUDIO,
        "video": DataTypeHint.VIDEO,
        "tensor": DataTypeHint.TENSOR,
        "json": DataTypeHint.JSON,
        "text": DataTypeHint.TEXT,
        "binary": DataTypeHint.BINARY,
    }
    return type_map[buffer["type"]]


def validate_buffer_type(
    buffer: DataBuffer,
    expected_type: DataTypeHint,
    node_id: str
) -> None:
    """
    Validate that data buffer matches expected type.

    Raises:
        TypeValidationError: If types don't match
    """
    actual_type = buffer_to_type_hint(buffer)

    if expected_type != DataTypeHint.ANY and actual_type != expected_type:
        raise TypeValidationError(
            f"Node '{node_id}' expects {expected_type.value} input but received {actual_type.value}",
            expected_type,
            actual_type,
            node_id
        )


# ============================================================================
# Builder Helpers
# ============================================================================

def create_audio_buffer(
    samples: bytes,
    sample_rate: int,
    channels: int,
    format: AudioFormat,
    num_samples: int,
    metadata: Optional[Dict[str, str]] = None
) -> AudioBufferDict:
    """Create type-safe audio buffer"""
    return {
        "type": "audio",
        "data": AudioBuffer(samples, sample_rate, channels, format, num_samples),
        "metadata": metadata,
    }


def create_video_frame(
    pixel_data: bytes,
    width: int,
    height: int,
    format: PixelFormat,
    frame_number: int,
    timestamp_us: int,
    metadata: Optional[Dict[str, str]] = None
) -> VideoFrameDict:
    """Create type-safe video frame"""
    return {
        "type": "video",
        "data": VideoFrame(pixel_data, width, height, format, frame_number, timestamp_us),
        "metadata": metadata,
    }


def create_json_data(
    json_payload: str,
    schema_type: Optional[str] = None,
    metadata: Optional[Dict[str, str]] = None
) -> JsonDataDict:
    """Create type-safe JSON data"""
    return {
        "type": "json",
        "data": JsonData(json_payload, schema_type),
        "metadata": metadata,
    }
