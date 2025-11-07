# Python API Contract

## Node Process API

### Node Base Class

```python
from abc import ABC, abstractmethod
from typing import Optional, Dict, Any
import asyncio

class MultiprocessNode(ABC):
    """Base class for nodes running in separate processes."""

    def __init__(self, node_id: str, config: Dict[str, Any]):
        self.node_id = node_id
        self.config = config
        self.input_channels: Dict[str, Subscriber] = {}
        self.output_channels: Dict[str, Publisher] = {}
        self._status = NodeStatus.IDLE

    @abstractmethod
    async def initialize(self) -> None:
        """Initialize node resources (load models, etc)."""
        pass

    @abstractmethod
    async def process(self, data: RuntimeData) -> Optional[RuntimeData]:
        """Process incoming data and return output."""
        pass

    @abstractmethod
    async def cleanup(self) -> None:
        """Clean up resources before shutdown."""
        pass

    async def run(self) -> None:
        """Main execution loop (framework handles this)."""
        # Managed by framework
        pass

class NodeStatus(Enum):
    IDLE = "idle"
    INITIALIZING = "initializing"
    READY = "ready"
    PROCESSING = "processing"
    ERROR = "error"
    STOPPING = "stopping"
```

### Node Registration

```python
from remotemedia.core.multiprocessing import register_node

@register_node("my_custom_node")
class MyCustomNode(MultiprocessNode):
    """Example custom node implementation."""

    async def initialize(self):
        # Load models, setup resources
        self.model = await load_model(self.config["model_path"])
        self._status = NodeStatus.READY

    async def process(self, data: RuntimeData) -> Optional[RuntimeData]:
        if data.type == DataType.AUDIO:
            # Process audio data
            result = self.model.process(data.payload)
            return RuntimeData(
                type=DataType.AUDIO,
                payload=result,
                metadata=data.metadata
            )
        return None

    async def cleanup(self):
        # Release resources
        if hasattr(self, 'model'):
            self.model.unload()
```

## Data Transfer API

### RuntimeData Class

```python
from dataclasses import dataclass
from enum import Enum
from typing import Union, Optional
import numpy as np

class DataType(Enum):
    AUDIO = "audio"
    VIDEO = "video"
    TEXT = "text"
    TENSOR = "tensor"

@dataclass
class AudioMetadata:
    sample_rate: int  # Hz
    channels: int     # 1=mono, 2=stereo
    format: str       # "f32", "i16", etc.

@dataclass
class VideoMetadata:
    width: int
    height: int
    format: str  # "rgb", "yuv420", etc.
    fps: float

@dataclass
class RuntimeData:
    """Zero-copy data container for IPC."""

    type: DataType
    payload: Union[np.ndarray, bytes, str]
    session_id: str
    timestamp: float
    metadata: Optional[Union[AudioMetadata, VideoMetadata]] = None

    @property
    def size(self) -> int:
        """Get payload size in bytes."""
        if isinstance(self.payload, np.ndarray):
            return self.payload.nbytes
        elif isinstance(self.payload, (bytes, str)):
            return len(self.payload)
        return 0

    def as_numpy(self) -> np.ndarray:
        """Get payload as numpy array (zero-copy when possible)."""
        if isinstance(self.payload, np.ndarray):
            return self.payload
        elif isinstance(self.payload, bytes):
            return np.frombuffer(self.payload, dtype=np.uint8)
        raise ValueError(f"Cannot convert {type(self.payload)} to numpy")
```

### Channel Operations

```python
from remotemedia.core.multiprocessing import Publisher, Subscriber

class Publisher:
    """Publish data to shared memory channel."""

    def __init__(self, channel_name: str, capacity: int = 10):
        self.channel_name = channel_name
        self.capacity = capacity

    def publish(self, data: RuntimeData) -> None:
        """Publish data (blocks if backpressure enabled)."""
        # Implementation handles zero-copy transfer
        pass

    def try_publish(self, data: RuntimeData) -> bool:
        """Try to publish without blocking."""
        # Returns False if channel full
        pass

class Subscriber:
    """Subscribe to shared memory channel."""

    def __init__(self, channel_name: str):
        self.channel_name = channel_name

    def receive(self, timeout: Optional[float] = None) -> Optional[RuntimeData]:
        """Receive data (blocks until available or timeout)."""
        # Implementation handles zero-copy transfer
        pass

    async def receive_async(self) -> RuntimeData:
        """Async receive for use in async nodes."""
        # Async version for event loops
        pass

    def __iter__(self):
        """Iterate over incoming messages."""
        while True:
            data = self.receive()
            if data is None:
                break
            yield data
```

## Session Management API

### Session Creation

```python
from remotemedia.core.multiprocessing import Session, Pipeline

class Pipeline:
    """Pipeline configuration."""

    def __init__(self):
        self.nodes: List[NodeConfig] = []
        self.connections: List[Connection] = []

    def add_node(self,
                 node_type: str,
                 node_id: str,
                 config: Dict[str, Any]) -> 'Pipeline':
        """Add a node to pipeline."""
        self.nodes.append(NodeConfig(node_type, node_id, config))
        return self

    def connect(self,
                from_node: str,
                from_port: str,
                to_node: str,
                to_port: str) -> 'Pipeline':
        """Connect two nodes."""
        self.connections.append(
            Connection(from_node, from_port, to_node, to_port)
        )
        return self

class Session:
    """Multiprocess pipeline session."""

    def __init__(self, session_id: str, pipeline: Pipeline):
        self.session_id = session_id
        self.pipeline = pipeline
        self.processes: List[Process] = []
        self.status = SessionStatus.CREATED

    async def initialize(self,
                        progress_callback: Optional[Callable] = None) -> None:
        """Initialize all nodes in pipeline."""
        for node in self.pipeline.nodes:
            # Spawn process for each node
            process = await spawn_node_process(node)
            self.processes.append(process)

            if progress_callback:
                progress_callback(InitProgress(
                    node_id=node.node_id,
                    status="initializing",
                    progress=0.5
                ))

        self.status = SessionStatus.READY

    async def start(self) -> None:
        """Start processing pipeline."""
        self.status = SessionStatus.RUNNING
        # Start all node processes

    async def stop(self) -> None:
        """Stop pipeline gracefully."""
        self.status = SessionStatus.STOPPING
        # Terminate all processes
        for process in self.processes:
            await process.terminate()
        self.status = SessionStatus.STOPPED

    async def cleanup(self) -> None:
        """Clean up all resources."""
        await self.stop()
        # Clean up shared memory
        # Remove IPC channels
```

### Progress Tracking

```python
@dataclass
class InitProgress:
    """Initialization progress update."""
    node_id: str
    status: str  # "starting", "loading", "connecting", "ready", "failed"
    progress: float  # 0.0 to 1.0
    message: Optional[str] = None

# Usage example
async def create_pipeline_with_progress():
    pipeline = (Pipeline()
        .add_node("whisper", "asr", {"model": "base"})
        .add_node("lfm2_audio", "s2s", {"model": "large"})
        .add_node("vibe_voice", "tts", {"voice": "sarah"})
        .connect("asr", "text_out", "s2s", "text_in")
        .connect("s2s", "audio_out", "tts", "audio_in"))

    session = Session("session_123", pipeline)

    def on_progress(progress: InitProgress):
        print(f"{progress.node_id}: {progress.status} ({progress.progress:.0%})")

    await session.initialize(progress_callback=on_progress)
    await session.start()
```

## Error Handling

### Exception Types

```python
class MultiprocessError(Exception):
    """Base exception for multiprocess operations."""
    pass

class ProcessSpawnError(MultiprocessError):
    """Failed to spawn node process."""
    pass

class ChannelFullError(MultiprocessError):
    """Channel buffer is full (backpressure)."""
    pass

class ProcessCrashError(MultiprocessError):
    """Node process crashed unexpectedly."""
    def __init__(self, node_id: str, exit_code: int, signal: Optional[int] = None):
        self.node_id = node_id
        self.exit_code = exit_code
        self.signal = signal
        super().__init__(f"Process {node_id} crashed with code {exit_code}")

class InitializationTimeout(MultiprocessError):
    """Node initialization timed out."""
    pass

class ResourceLimitError(MultiprocessError):
    """Resource limit exceeded (processes, memory)."""
    pass
```

### Error Recovery

```python
# Automatic pipeline termination on crash
async def handle_process_crash(session: Session, crashed_node: str):
    """Handle node crash by terminating pipeline."""
    try:
        # Log the crash
        logger.error(f"Node {crashed_node} crashed, terminating pipeline")

        # Stop all other nodes
        await session.stop()

        # Notify user
        await notify_user(f"Pipeline terminated due to {crashed_node} failure")

    finally:
        # Always cleanup resources
        await session.cleanup()
```