"""
Python API Contract for Model Registry and Shared Memory Tensors

This module provides Python bindings for the model registry and shared memory
tensor functionality, enabling zero-copy tensor exchange with ML frameworks.
"""

from typing import Optional, Dict, Any, Callable, Union, List, Tuple
from dataclasses import dataclass
from enum import Enum
import numpy as np
from contextlib import contextmanager
from abc import ABC, abstractmethod

# ============================================================================
# Model Registry API
# ============================================================================

class DeviceType(Enum):
    """Device types for model placement"""
    CPU = "cpu"
    CUDA = "cuda"
    METAL = "metal"
    
    def with_index(self, index: int) -> str:
        """Get device string with index (e.g., 'cuda:0')"""
        if self == DeviceType.CPU:
            return self.value
        return f"{self.value}:{index}"


class InferenceModel(ABC):
    """Base class for models that can be managed by the registry"""
    
    @property
    @abstractmethod
    def model_id(self) -> str:
        """Unique identifier for this model"""
        pass
    
    @property
    @abstractmethod
    def device(self) -> str:
        """Device this model is loaded on"""
        pass
    
    @property
    @abstractmethod
    def memory_usage(self) -> int:
        """Memory usage in bytes"""
        pass
    
    @abstractmethod
    async def infer(self, input_tensor: 'TensorBuffer') -> 'TensorBuffer':
        """Perform inference"""
        pass


class ModelHandle:
    """Handle to a loaded model with automatic reference counting"""
    
    def __init__(self, model: InferenceModel, handle_id: str, registry: 'ModelRegistry'):
        self._model = model
        self._handle_id = handle_id
        self._registry = registry
    
    @property
    def model(self) -> InferenceModel:
        """Get the model for inference"""
        return self._model
    
    @property
    def handle_id(self) -> str:
        """Get handle identifier"""
        return self._handle_id
    
    def __enter__(self):
        """Context manager entry"""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - releases reference"""
        self._registry.release(self._model.model_id)
    
    def __del__(self):
        """Destructor - ensures reference is released"""
        if hasattr(self, '_registry'):
            self._registry.release(self._model.model_id)


class EvictionPolicy(Enum):
    """Model eviction policies"""
    LRU = "lru"  # Least Recently Used
    LFU = "lfu"  # Least Frequently Used
    TTL = "ttl"  # Time-based only
    MANUAL = "manual"  # Manual only


@dataclass
class RegistryConfig:
    """Configuration for model registry"""
    ttl_seconds: float = 30.0
    max_memory_bytes: Optional[int] = None
    eviction_policy: EvictionPolicy = EvictionPolicy.LRU
    enable_metrics: bool = True


@dataclass
class ModelInfo:
    """Information about a loaded model"""
    model_id: str
    device: str
    memory_bytes: int
    reference_count: int
    loaded_at: float  # Unix timestamp
    last_accessed: float  # Unix timestamp


@dataclass
class RegistryMetrics:
    """Registry performance metrics"""
    total_models: int
    total_memory_bytes: int
    cache_hits: int
    cache_misses: int
    evictions: int
    
    @property
    def hit_rate(self) -> float:
        """Calculate cache hit rate"""
        total = self.cache_hits + self.cache_misses
        return self.cache_hits / total if total > 0 else 0.0


class ModelRegistry:
    """Process-local model registry (singleton)"""
    
    _instance: Optional['ModelRegistry'] = None
    
    def __new__(cls, config: Optional[RegistryConfig] = None):
        """Ensure singleton instance"""
        if cls._instance is None:
            cls._instance = super().__new__(cls)
            cls._instance._initialize(config or RegistryConfig())
        return cls._instance
    
    def _initialize(self, config: RegistryConfig):
        """Initialize registry (called once)"""
        self._config = config
        self._models: Dict[str, InferenceModel] = {}
        self._loading: set = set()
        self._metrics = RegistryMetrics(0, 0, 0, 0, 0)
    
    def get_or_load(
        self,
        key: str,
        loader: Callable[[], InferenceModel],
        session_id: Optional[str] = None
    ) -> ModelHandle:
        """
        Get existing model or load new one.
        
        Args:
            key: Unique model identifier
            loader: Function to load the model if not cached
            session_id: Optional session association
            
        Returns:
            ModelHandle with reference to the model
        """
        # Implementation provided by Rust backend via PyO3
        ...
    
    async def get_or_load_async(
        self,
        key: str,
        loader: Callable[[], InferenceModel],
        session_id: Optional[str] = None
    ) -> ModelHandle:
        """Async version of get_or_load"""
        ...
    
    def release(self, key: str):
        """Release a model reference (decrements count)"""
        ...
    
    def list_models(self) -> List[ModelInfo]:
        """List all loaded models"""
        ...
    
    def metrics(self) -> RegistryMetrics:
        """Get registry metrics"""
        return self._metrics
    
    def evict_expired(self) -> Tuple[int, int]:
        """
        Force eviction of expired models.
        
        Returns:
            Tuple of (evicted_count, freed_memory_bytes)
        """
        ...
    
    def clear(self):
        """Clear all models (for testing)"""
        ...


# Convenience function for module-level access
def get_or_load(
    key: str,
    loader: Callable[[], InferenceModel]
) -> InferenceModel:
    """
    Get or load a model from the global registry.
    
    Example:
        model = get_or_load("whisper_base", lambda: WhisperModel("base"))
    """
    registry = ModelRegistry()
    handle = registry.get_or_load(key, loader)
    return handle.model


# ============================================================================
# Shared Memory Tensor API
# ============================================================================

class DataType(Enum):
    """Tensor data types"""
    F32 = "float32"
    F16 = "float16"
    I32 = "int32"
    I64 = "int64"
    U8 = "uint8"
    
    @property
    def numpy_dtype(self):
        """Get corresponding NumPy dtype"""
        mapping = {
            DataType.F32: np.float32,
            DataType.F16: np.float16,
            DataType.I32: np.int32,
            DataType.I64: np.int64,
            DataType.U8: np.uint8,
        }
        return mapping[self]


class TensorStorage(Enum):
    """Tensor storage backend"""
    HEAP = "heap"
    SHARED_MEMORY = "shared_memory"
    GPU_MEMORY = "gpu_memory"


class TensorBuffer:
    """
    Tensor with multiple storage backend support.
    
    Supports zero-copy exchange with NumPy and PyTorch via standard protocols.
    """
    
    def __init__(
        self,
        data: Union[np.ndarray, bytes],
        shape: Optional[List[int]] = None,
        dtype: Optional[DataType] = None,
        storage: TensorStorage = TensorStorage.HEAP
    ):
        """
        Create a tensor buffer.
        
        Args:
            data: Tensor data (NumPy array or raw bytes)
            shape: Tensor shape (inferred from array if not provided)
            dtype: Data type (inferred from array if not provided)
            storage: Storage backend
        """
        ...
    
    @classmethod
    def from_numpy(
        cls,
        array: np.ndarray,
        zero_copy: bool = True
    ) -> 'TensorBuffer':
        """
        Create from NumPy array.
        
        Args:
            array: NumPy array
            zero_copy: If True, share memory with array
            
        Returns:
            TensorBuffer sharing memory with the array
        """
        ...
    
    @classmethod
    def from_shared_memory(
        cls,
        region_id: str,
        offset: int,
        size: int,
        shape: List[int],
        dtype: DataType
    ) -> 'TensorBuffer':
        """
        Create from shared memory region.
        
        Args:
            region_id: Shared memory region identifier
            offset: Offset within region
            size: Size in bytes
            shape: Tensor shape
            dtype: Data type
            
        Returns:
            TensorBuffer backed by shared memory
        """
        ...
    
    def to_numpy(self) -> np.ndarray:
        """
        Convert to NumPy array (zero-copy when possible).
        
        Returns:
            NumPy array view of the tensor
        """
        ...
    
    @property
    def shape(self) -> Tuple[int, ...]:
        """Tensor shape"""
        ...
    
    @property
    def dtype(self) -> DataType:
        """Data type"""
        ...
    
    @property
    def storage(self) -> TensorStorage:
        """Storage backend"""
        ...
    
    @property
    def nbytes(self) -> int:
        """Size in bytes"""
        ...
    
    def __array__(self) -> np.ndarray:
        """NumPy array protocol for zero-copy conversion"""
        return self.to_numpy()
    
    def __dlpack__(self, stream=None):
        """DLPack protocol for zero-copy exchange with PyTorch/TensorFlow"""
        ...
    
    def __dlpack_device__(self) -> Tuple[int, int]:
        """DLPack device information"""
        ...


class SharedMemoryRegion:
    """Shared memory region accessible by multiple processes"""
    
    @classmethod
    def create(cls, size: int) -> 'SharedMemoryRegion':
        """Create a new shared memory region"""
        ...
    
    @classmethod
    def open(cls, region_id: str) -> 'SharedMemoryRegion':
        """Open an existing shared memory region"""
        ...
    
    @property
    def id(self) -> str:
        """Region identifier"""
        ...
    
    @property
    def size(self) -> int:
        """Region size in bytes"""
        ...
    
    def close(self):
        """Close the region (decrements reference)"""
        ...
    
    def unlink(self):
        """Unlink the region (removes from system)"""
        ...


@dataclass
class AllocatorConfig:
    """Configuration for shared memory allocator"""
    max_memory_bytes: int = 10 * 1024 * 1024 * 1024  # 10GB
    per_session_quota: Optional[int] = None
    cleanup_interval_seconds: float = 60.0


class SharedMemoryAllocator:
    """Allocator for managing shared memory regions"""
    
    def __init__(self, config: Optional[AllocatorConfig] = None):
        self._config = config or AllocatorConfig()
    
    def allocate_tensor(
        self,
        size: int,
        session_id: Optional[str] = None
    ) -> TensorBuffer:
        """
        Allocate a tensor in shared memory.
        
        Args:
            size: Size in bytes
            session_id: Optional session for quota tracking
            
        Returns:
            TensorBuffer backed by shared memory
        """
        ...
    
    def free(self, region_id: str):
        """Free a shared memory region"""
        ...
    
    @contextmanager
    def tensor(self, size: int):
        """
        Context manager for temporary tensor allocation.
        
        Example:
            with allocator.tensor(1024 * 1024) as tensor:
                # Use tensor
                pass
            # Automatically freed
        """
        tensor = self.allocate_tensor(size)
        try:
            yield tensor
        finally:
            if hasattr(tensor, '_region_id'):
                self.free(tensor._region_id)


# ============================================================================
# Model Worker Client API
# ============================================================================

class ModelWorkerClient:
    """Client for connecting to model worker processes"""
    
    def __init__(self, endpoint: str):
        """
        Create a client.
        
        Args:
            endpoint: Worker endpoint (e.g., "grpc://localhost:50051")
        """
        self._endpoint = endpoint
    
    async def connect(self):
        """Establish connection to worker"""
        ...
    
    async def infer(
        self,
        input_tensor: TensorBuffer,
        parameters: Optional[Dict[str, str]] = None
    ) -> TensorBuffer:
        """
        Submit inference request.
        
        Args:
            input_tensor: Input tensor
            parameters: Optional parameters
            
        Returns:
            Output tensor
        """
        ...
    
    async def health_check(self) -> bool:
        """Check if worker is healthy"""
        ...
    
    async def status(self) -> Dict[str, Any]:
        """Get worker status"""
        ...
    
    async def close(self):
        """Close connection"""
        ...


# ============================================================================
# Integration Helpers
# ============================================================================

def torch_to_buffer(tensor, zero_copy: bool = True) -> TensorBuffer:
    """
    Convert PyTorch tensor to TensorBuffer.
    
    Args:
        tensor: PyTorch tensor
        zero_copy: Use DLPack for zero-copy conversion
        
    Returns:
        TensorBuffer sharing memory with the tensor
    """
    if zero_copy and hasattr(tensor, '__dlpack__'):
        # Use DLPack protocol
        capsule = tensor.__dlpack__()
        return TensorBuffer.from_dlpack(capsule)
    else:
        # Fall back to NumPy conversion
        array = tensor.detach().cpu().numpy()
        return TensorBuffer.from_numpy(array)


def buffer_to_torch(buffer: TensorBuffer, device: Optional[str] = None):
    """
    Convert TensorBuffer to PyTorch tensor.
    
    Args:
        buffer: TensorBuffer
        device: Target device (e.g., 'cuda:0')
        
    Returns:
        PyTorch tensor
    """
    import torch
    
    if hasattr(buffer, '__dlpack__'):
        # Use DLPack protocol
        return torch.from_dlpack(buffer)
    else:
        # Fall back to NumPy conversion
        array = buffer.to_numpy()
        tensor = torch.from_numpy(array)
        if device:
            tensor = tensor.to(device)
        return tensor


# ============================================================================
# Example Usage
# ============================================================================

if __name__ == "__main__":
    # Example: Using model registry
    def load_whisper():
        """Loader function for Whisper model"""
        from transformers import WhisperModel
        return WhisperModel.from_pretrained("openai/whisper-base")
    
    # Get or load model (singleton registry)
    model = get_or_load("whisper-base", load_whisper)
    
    # Example: Zero-copy tensor exchange
    import numpy as np
    
    # Create tensor from NumPy
    array = np.random.randn(1, 3, 224, 224).astype(np.float32)
    tensor = TensorBuffer.from_numpy(array, zero_copy=True)
    
    # Shared memory allocation
    allocator = SharedMemoryAllocator()
    with allocator.tensor(1024 * 1024) as shm_tensor:
        # Use shared memory tensor
        pass  # Automatically freed
    
    # Example: Model worker client
    import asyncio
    
    async def inference_example():
        client = ModelWorkerClient("grpc://localhost:50051")
        await client.connect()
        
        # Submit inference
        input_data = np.random.randn(1, 3, 224, 224).astype(np.float32)
        input_tensor = TensorBuffer.from_numpy(input_data)
        
        output_tensor = await client.infer(input_tensor)
        output_array = output_tensor.to_numpy()
        
        await client.close()
        return output_array
    
    # Run async example
    # result = asyncio.run(inference_example())
