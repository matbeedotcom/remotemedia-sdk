"""
RemoteMedia Python gRPC Client

Modern async Python client for the Rust gRPC service (003-rust-grpc-service).
Compatible with protocol version v1 (Phases 1-5).
"""

import asyncio
import json
import logging
from dataclasses import dataclass
from typing import Optional, Dict, Any, AsyncGenerator, Tuple, List
from enum import Enum

import grpc

# Import generated proto stubs (with clean names from generated/__init__.py)
try:
    from generated import (
        # Common types
        AudioBuffer as ProtoAudioBuffer,
        AudioFormat as ProtoAudioFormat,
        ExecutionMetrics as ProtoExecutionMetrics,
        ErrorResponse as ProtoErrorResponse,
        ErrorType as ProtoErrorType,
        VersionInfo as ProtoVersionInfo,
        ExecutionStatus as ProtoExecutionStatus,
        # Execution types
        PipelineManifest as ProtoPipelineManifest,
        ManifestMetadata as ProtoManifestMetadata,
        NodeManifest as ProtoNodeManifest,
        Connection as ProtoConnection,
        ExecuteRequest as ProtoExecuteRequest,
        ExecuteResponse as ProtoExecuteResponse,
        ExecutionResult as ProtoExecutionResult,
        VersionRequest as ProtoVersionRequest,
        VersionResponse as ProtoVersionResponse,
        RuntimeHint as ProtoRuntimeHint,
        # Streaming types
        StreamRequest as ProtoStreamRequest,
        StreamInit as ProtoStreamInit,
        AudioChunk as ProtoAudioChunk,
        StreamControl as ProtoStreamControl,
        ChunkResult as ProtoChunkResult,
        # Service stubs
        PipelineExecutionServiceStub,
        StreamingPipelineServiceStub,
        # Enums (need to import the modules for enum values)
    )
    from generated import common_pb2, execution_pb2, streaming_pb2
except ImportError:
    raise ImportError(
        "Generated proto stubs not found. Run: python generate_protos.py"
    )

logger = logging.getLogger(__name__)


# ============================================================================
# Type Aliases and Enums
# ============================================================================

class AudioFormat(Enum):
    """Audio sample format."""
    F32 = common_pb2.AUDIO_FORMAT_F32
    I16 = common_pb2.AUDIO_FORMAT_I16
    I32 = common_pb2.AUDIO_FORMAT_I32


class ErrorType(Enum):
    """Error categories for client error handling."""
    VALIDATION = common_pb2.ERROR_TYPE_VALIDATION
    NODE_EXECUTION = common_pb2.ERROR_TYPE_NODE_EXECUTION
    RESOURCE_LIMIT = common_pb2.ERROR_TYPE_RESOURCE_LIMIT
    AUTHENTICATION = common_pb2.ERROR_TYPE_AUTHENTICATION
    VERSION_MISMATCH = common_pb2.ERROR_TYPE_VERSION_MISMATCH
    INTERNAL = common_pb2.ERROR_TYPE_INTERNAL


class RuntimeHint(Enum):
    """Runtime hint for Python node execution."""
    AUTO = execution_pb2.RUNTIME_HINT_AUTO
    RUSTPYTHON = execution_pb2.RUNTIME_HINT_RUSTPYTHON
    CPYTHON = execution_pb2.RUNTIME_HINT_CPYTHON
    CPYTHON_WASM = execution_pb2.RUNTIME_HINT_CPYTHON_WASM


# ============================================================================
# Data Classes
# ============================================================================

@dataclass
class AudioBuffer:
    """Multi-channel audio buffer with metadata."""
    samples: bytes
    sample_rate: int
    channels: int
    format: AudioFormat
    num_samples: int
    
    def to_proto(self) -> ProtoAudioBuffer:
        """Convert to protobuf message."""
        return ProtoAudioBuffer(
            samples=self.samples,
            sample_rate=self.sample_rate,
            channels=self.channels,
            format=self.format.value,
            num_samples=self.num_samples
        )
    
    @classmethod
    def from_proto(cls, proto: ProtoAudioBuffer) -> 'AudioBuffer':
        """Create from protobuf message."""
        return cls(
            samples=proto.samples,
            sample_rate=proto.sample_rate,
            channels=proto.channels,
            format=AudioFormat(proto.format),
            num_samples=proto.num_samples
        )


@dataclass
class ExecutionMetrics:
    """Performance metrics for pipeline execution."""
    wall_time_ms: float
    cpu_time_ms: float
    memory_used_bytes: int
    serialization_time_ms: float
    node_metrics: Dict[str, Any]
    
    @classmethod
    def from_proto(cls, proto: ProtoExecutionMetrics) -> 'ExecutionMetrics':
        """Create from protobuf message."""
        return cls(
            wall_time_ms=proto.wall_time_ms,
            cpu_time_ms=proto.cpu_time_ms,
            memory_used_bytes=proto.memory_used_bytes,
            serialization_time_ms=proto.serialization_time_ms,
            node_metrics={k: v for k, v in proto.node_metrics.items()}
        )


@dataclass
class VersionInfo:
    """Service version and compatibility information."""
    protocol_version: str
    runtime_version: str
    supported_node_types: List[str]
    supported_protocols: List[str]
    build_timestamp: str
    
    @classmethod
    def from_proto(cls, proto: ProtoVersionInfo) -> 'VersionInfo':
        """Create from protobuf message."""
        return cls(
            protocol_version=proto.protocol_version,
            runtime_version=proto.runtime_version,
            supported_node_types=list(proto.supported_node_types),
            supported_protocols=list(proto.supported_protocols),
            build_timestamp=proto.build_timestamp
        )


@dataclass
class ExecutionResult:
    """Result from successful pipeline execution."""
    audio_outputs: Dict[str, AudioBuffer]
    data_outputs: Dict[str, str]
    metrics: ExecutionMetrics
    status: str
    
    @classmethod
    def from_proto(cls, proto: ProtoExecutionResult) -> 'ExecutionResult':
        """Create from protobuf message."""
        return cls(
            audio_outputs={
                k: AudioBuffer.from_proto(v)
                for k, v in proto.audio_outputs.items()
            },
            data_outputs={k: v for k, v in proto.data_outputs.items()},
            metrics=ExecutionMetrics.from_proto(proto.metrics),
            status=common_pb2.ExecutionStatus.Name(proto.status)
        )


@dataclass
class ChunkResult:
    """Result from processing a single streaming chunk."""
    sequence: int
    audio_outputs: Dict[str, AudioBuffer]
    data_outputs: Dict[str, str]
    processing_time_ms: float
    total_samples_processed: int
    
    @classmethod
    def from_proto(cls, proto: ProtoChunkResult) -> 'ChunkResult':
        """Create from protobuf message."""
        return cls(
            sequence=proto.sequence,
            audio_outputs={
                k: AudioBuffer.from_proto(v)
                for k, v in proto.audio_outputs.items()
            },
            data_outputs={k: v for k, v in proto.data_outputs.items()},
            processing_time_ms=proto.processing_time_ms,
            total_samples_processed=proto.total_samples_processed
        )


# ============================================================================
# Exception Classes
# ============================================================================

class RemoteMediaError(Exception):
    """Base exception for RemoteMedia client errors."""
    
    def __init__(
        self,
        message: str,
        error_type: Optional[ErrorType] = None,
        failing_node_id: Optional[str] = None,
        context: Optional[str] = None,
        stack_trace: Optional[str] = None
    ):
        super().__init__(message)
        self.error_type = error_type
        self.failing_node_id = failing_node_id
        self.context = context
        self.stack_trace = stack_trace
    
    @classmethod
    def from_proto(cls, proto: ProtoErrorResponse) -> 'RemoteMediaError':
        """Create exception from protobuf ErrorResponse."""
        return cls(
            message=proto.message,
            error_type=ErrorType(proto.error_type) if proto.error_type else None,
            failing_node_id=proto.failing_node_id or None,
            context=proto.context or None,
            stack_trace=proto.stack_trace or None
        )


# ============================================================================
# Main Client
# ============================================================================

class RemoteMediaClient:
    """
    Async client for Rust gRPC service.
    
    Supports:
    - GetVersion: Service version and compatibility check
    - ExecutePipeline: Unary RPC for batch processing
    - StreamPipeline: Bidirectional streaming for real-time audio
    
    Example:
        async with RemoteMediaClient("localhost:50051") as client:
            version = await client.get_version()
            print(f"Connected to service v{version.protocol_version}")
    """
    
    def __init__(
        self,
        address: str,
        ssl: bool = False,
        credentials: Optional[grpc.ChannelCredentials] = None,
        options: Optional[List[Tuple[str, Any]]] = None,
        api_key: Optional[str] = None,
    ):
        """
        Initialize client.
        
        Args:
            address: Server address (host:port), e.g., "localhost:50051"
            ssl: Whether to use SSL/TLS
            credentials: gRPC channel credentials (if ssl=True)
            options: Additional gRPC channel options
        """
        self.address = address
        self.ssl = ssl
        self.credentials = credentials
        self.options = options or []
        self.api_key = api_key
        
        self.channel: Optional[grpc.aio.Channel] = None
        self.execution_stub: Optional[execution_pb2_grpc.PipelineExecutionServiceStub] = None
        self.streaming_stub: Optional[streaming_pb2_grpc.StreamingPipelineServiceStub] = None
        
        logger.debug(f"RemoteMediaClient initialized for {address}")
    
    async def connect(self) -> None:
        """
        Establish connection to server.
        
        Raises:
            RemoteMediaError: If connection fails
        """
        try:
            if self.ssl:
                if not self.credentials:
                    self.credentials = grpc.ssl_channel_credentials()
                self.channel = grpc.aio.secure_channel(
                    self.address,
                    self.credentials,
                    options=self.options
                )
            else:
                self.channel = grpc.aio.insecure_channel(
                    self.address,
                    options=self.options
                )
            
            # Create stubs
            self.execution_stub = PipelineExecutionServiceStub(self.channel)
            self.streaming_stub = StreamingPipelineServiceStub(self.channel)
            
            # Test connection with GetVersion
            await self.get_version()
            
            logger.info(f"Connected to RemoteMedia service at {self.address}")
            
        except grpc.aio.AioRpcError as e:
            raise RemoteMediaError(
                f"Failed to connect to {self.address}: {e.details()}",
                error_type=ErrorType.INTERNAL
            ) from e
    
    async def disconnect(self) -> None:
        """Close connection to server."""
        if self.channel:
            await self.channel.close()
            self.channel = None
            self.execution_stub = None
            self.streaming_stub = None
            logger.debug("Disconnected from RemoteMedia service")
    
    async def get_version(
        self,
        client_version: str = "v1"
    ) -> VersionInfo:
        """
        Get service version and compatibility information.
        
        Args:
            client_version: Client protocol version
        
        Returns:
            Service version information
        
        Raises:
            RemoteMediaError: If request fails
        """
        if not self.execution_stub:
            raise RemoteMediaError("Not connected to service")
        
        try:
            request = ProtoVersionRequest(client_version=client_version)
            
            response = await self.execution_stub.GetVersion(request, metadata=self._metadata())
            
            if not response.compatible:
                logger.warning(
                    f"Version incompatibility: {response.compatibility_message}"
                )
            
            return VersionInfo.from_proto(response.version_info)
            
        except grpc.aio.AioRpcError as e:
            raise RemoteMediaError(
                f"GetVersion failed: {e.details()}",
                error_type=ErrorType.INTERNAL
            ) from e
    
    async def execute_pipeline(
        self,
        manifest: Dict[str, Any],
        audio_inputs: Optional[Dict[str, AudioBuffer]] = None,
        data_inputs: Optional[Dict[str, str]] = None,
        client_version: str = "v1"
    ) -> ExecutionResult:
        """
        Execute a pipeline (unary RPC).
        
        Args:
            manifest: Pipeline manifest dictionary (will be converted to proto)
            audio_inputs: Audio buffers keyed by node ID
            data_inputs: JSON data inputs keyed by node ID
            client_version: Client protocol version
        
        Returns:
            Execution result with outputs and metrics
        
        Raises:
            RemoteMediaError: If execution fails
        """
        if not self.execution_stub:
            raise RemoteMediaError("Not connected to service")
        
        try:
            # Convert manifest to proto
            manifest_proto = self._dict_to_manifest(manifest)
            
            # Convert audio inputs
            audio_inputs_proto = {}
            if audio_inputs:
                audio_inputs_proto = {
                    k: v.to_proto() for k, v in audio_inputs.items()
                }
            
            # Create request
            request = ProtoExecuteRequest(
                manifest=manifest_proto,
                audio_inputs=audio_inputs_proto,
                data_inputs=data_inputs or {},
                client_version=client_version
            )
            
            # Execute
            response = await self.execution_stub.ExecutePipeline(request, metadata=self._metadata())
            
            # Check for error
            if response.HasField("error"):
                raise RemoteMediaError.from_proto(response.error)
            
            # Return result
            return ExecutionResult.from_proto(response.result)
            
        except grpc.aio.AioRpcError as e:
            raise RemoteMediaError(
                f"ExecutePipeline failed: {e.details()}",
                error_type=ErrorType.INTERNAL
            ) from e
    
    async def stream_pipeline(
        self,
        manifest: Dict[str, Any],
        audio_chunks: AsyncGenerator[Tuple[str, AudioBuffer, int], None],
        data_inputs: Optional[Dict[str, str]] = None,
        client_version: str = "v1",
        expected_chunk_size: int = 1600
    ) -> AsyncGenerator[ChunkResult, None]:
        """
        Execute a pipeline with streaming audio (bidirectional RPC).
        
        Args:
            manifest: Pipeline manifest dictionary
            audio_chunks: Async generator yielding (node_id, buffer, sequence)
            data_inputs: Initial JSON data inputs keyed by node ID
            client_version: Client protocol version
            expected_chunk_size: Expected samples per chunk
        
        Yields:
            ChunkResult for each processed chunk
        
        Raises:
            RemoteMediaError: If streaming fails
        
        Example:
            async def generate_chunks():
                for i in range(100):
                    buffer = AudioBuffer(...)
                    yield ("source", buffer, i)
            
            async for result in client.stream_pipeline(manifest, generate_chunks()):
                print(f"Chunk {result.sequence}: {result.processing_time_ms}ms")
        """
        if not self.streaming_stub:
            raise RemoteMediaError("Not connected to service")
        
        try:
            # Request generator
            async def request_generator():
                # First message: initialization
                manifest_proto = self._dict_to_manifest(manifest)
                init_msg = ProtoStreamInit(
                    manifest=manifest_proto,
                    data_inputs=data_inputs or {},
                    client_version=client_version,
                    expected_chunk_size=expected_chunk_size
                )
                yield ProtoStreamRequest(init=init_msg)
                
                # Subsequent messages: audio chunks
                async for node_id, buffer, sequence in audio_chunks:
                    chunk = ProtoAudioChunk(
                        node_id=node_id,
                        buffer=buffer.to_proto(),
                        sequence=sequence
                    )
                    yield ProtoStreamRequest(audio_chunk=chunk)
                
                # Final message: close
                control = ProtoStreamControl(
                    command=streaming_pb2.StreamControl.COMMAND_CLOSE
                )
                yield ProtoStreamRequest(control=control)
            
            # Start streaming
            response_stream = self.streaming_stub.StreamPipeline(
                request_generator(),
                metadata=self._metadata(),
            )
            
            # Process responses
            async for response in response_stream:
                if response.HasField("ready"):
                    logger.debug(
                        f"Stream ready: session_id={response.ready.session_id}"
                    )
                
                elif response.HasField("result"):
                    yield ChunkResult.from_proto(response.result)
                
                elif response.HasField("error"):
                    raise RemoteMediaError.from_proto(response.error)
                
                elif response.HasField("metrics"):
                    logger.debug(
                        f"Stream metrics: {response.metrics.chunks_processed} chunks, "
                        f"{response.metrics.average_latency_ms:.2f}ms avg latency"
                    )
                
                elif response.HasField("closed"):
                    logger.debug(
                        f"Stream closed: {response.closed.reason}"
                    )
                    break
            
        except grpc.aio.AioRpcError as e:
            raise RemoteMediaError(
                f"StreamPipeline failed: {e.details()}",
                error_type=ErrorType.INTERNAL
            ) from e
    
    # ========================================================================
    # Helper Methods
    # ========================================================================
    
    def _dict_to_manifest(self, manifest: Dict[str, Any]) -> ProtoPipelineManifest:
        """Convert Python dict to PipelineManifest proto."""
        metadata = manifest.get("metadata", {})
        metadata_proto = ProtoManifestMetadata(
            name=metadata.get("name", "unnamed"),
            description=metadata.get("description", ""),
            created_at=metadata.get("created_at", "")
        )
        
        nodes_proto = []
        for node in manifest.get("nodes", []):
            node_proto = ProtoNodeManifest(
                id=node["id"],
                node_type=node["node_type"],
                params=node.get("params", "{}"),
                is_streaming=node.get("is_streaming", False)
            )
            
            # Optional runtime hint
            if "runtime_hint" in node:
                if isinstance(node["runtime_hint"], RuntimeHint):
                    node_proto.runtime_hint = node["runtime_hint"].value
                elif isinstance(node["runtime_hint"], int):
                    node_proto.runtime_hint = node["runtime_hint"]
            
            nodes_proto.append(node_proto)
        
        connections_proto = []
        for conn in manifest.get("connections", []):
            conn_proto = ProtoConnection(
                from_=conn.get("from", conn.get("from_node", "")),
                to=conn.get("to", conn.get("to_node", ""))
            )
            connections_proto.append(conn_proto)
        
        return ProtoPipelineManifest(
            version=manifest.get("version", "v1"),
            metadata=metadata_proto,
            nodes=nodes_proto,
            connections=connections_proto
        )

    def _metadata(self) -> List[Tuple[str, str]]:
        """Build default gRPC metadata including preview feature flag and auth."""
        md: List[Tuple[str, str]] = []
        if self.api_key:
            md.append(("authorization", f"Bearer {self.api_key}"))
        # Enable GPT-5 Codex preview by default unless disabled via env
        import os
        disable = os.getenv("DISABLE_GPT5_CODEX_PREVIEW", "").lower()
        if disable not in ("true", "1", "yes", "on"):
            md.append(("x-preview-features", "gpt5-codex"))
        md.append(("x-client", "python"))
        return md
    
    # ========================================================================
    # Context Manager
    # ========================================================================
    
    async def __aenter__(self):
        """Async context manager entry."""
        await self.connect()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.disconnect()
