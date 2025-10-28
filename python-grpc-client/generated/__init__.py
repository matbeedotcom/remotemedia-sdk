"""Auto-generated gRPC stubs with clean imports."""

# Clean imports without _pb2 suffix
from .common_pb2 import (
    AudioBuffer,
    AudioFormat,
    ExecutionMetrics,
    NodeMetrics,
    ErrorResponse,
    ErrorType,
    VersionInfo,
    ResourceLimits,
    ExecutionStatus,
    NodeStatus,
    NodeResult,
    AUDIO_FORMAT_F32,
    AUDIO_FORMAT_I16,
    AUDIO_FORMAT_I32,
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
    # Common types
    "AudioBuffer",
    "AudioFormat",
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
    "AudioChunk",
    "ChunkResult",
    
    # Service stubs
    "PipelineExecutionServiceStub",
    "StreamingPipelineServiceStub",
]
