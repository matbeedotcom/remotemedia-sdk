"""Auto-generated gRPC stubs with clean imports."""

# Clean imports without _pb2 suffix
from .common_pb2 import (
    # Messages
    AudioBuffer,
    VideoFrame,
    TensorBuffer,
    JsonData,
    TextBuffer,
    BinaryBuffer,
    DataBuffer,
    ExecutionMetrics,
    NodeMetrics,
    ErrorResponse,
    ErrorType,
    VersionInfo,
    ResourceLimits,
    ExecutionStatus,
    NodeStatus,
    NodeResult,
    # Enums
    AudioFormat,
    PixelFormat,
    TensorDtype,
    DataTypeHint,
    # Enum values
    AUDIO_FORMAT_F32,
    AUDIO_FORMAT_I16,
    AUDIO_FORMAT_I32,
    PIXEL_FORMAT_RGB24,
    PIXEL_FORMAT_RGBA32,
    PIXEL_FORMAT_YUV420P,
    PIXEL_FORMAT_GRAY8,
    ERROR_TYPE_VALIDATION,
    ERROR_TYPE_NODE_EXECUTION,
    ERROR_TYPE_RESOURCE_LIMIT,
    ERROR_TYPE_AUTHENTICATION,
    ERROR_TYPE_VERSION_MISMATCH,
    ERROR_TYPE_INTERNAL,
    EXECUTION_STATUS_SUCCESS,
    EXECUTION_STATUS_PARTIAL_SUCCESS,
    EXECUTION_STATUS_FAILED,
)

from .execution_pb2 import (
    PipelineManifest,
    ManifestMetadata,
    NodeManifest,
    Connection,
    ExecuteRequest,
    ExecuteResponse,
    ExecutionResult,
    VersionRequest,
    VersionResponse,
    CapabilityRequirements,
    GpuRequirement,
    CpuRequirement,
    RuntimeHint,
    RUNTIME_HINT_AUTO,
    RUNTIME_HINT_RUSTPYTHON,
    RUNTIME_HINT_CPYTHON,
    RUNTIME_HINT_CPYTHON_WASM,
)

from .execution_pb2_grpc import (
    PipelineExecutionServiceStub,
    PipelineExecutionServiceServicer,
)

from .streaming_pb2 import (
    StreamRequest,
    StreamResponse,
    StreamInit,
    DataChunk,
    AudioChunk,
    StreamControl,
    StreamReady,
    ChunkResult,
    StreamMetrics,
    StreamClosed,
    StreamErrorResponse,
    StreamErrorType,
)

from .streaming_pb2_grpc import (
    StreamingPipelineServiceStub,
    StreamingPipelineServiceServicer,
)

__all__ = [
    # Common types - Data Buffers
    "AudioBuffer",
    "VideoFrame",
    "TensorBuffer",
    "JsonData",
    "TextBuffer",
    "BinaryBuffer",
    "DataBuffer",

    # Common types - Enums
    "AudioFormat",
    "PixelFormat",
    "TensorDtype",
    "DataTypeHint",

    # Common types - Others
    "ExecutionMetrics",
    "ErrorResponse",
    "ErrorType",
    "VersionInfo",

    # Execution types
    "PipelineManifest",
    "ExecuteRequest",
    "ExecuteResponse",
    "ExecutionResult",
    "VersionRequest",
    "VersionResponse",

    # Streaming types
    "StreamRequest",
    "StreamResponse",
    "StreamInit",
    "DataChunk",
    "AudioChunk",
    "ChunkResult",

    # Service stubs
    "PipelineExecutionServiceStub",
    "StreamingPipelineServiceStub",
]
