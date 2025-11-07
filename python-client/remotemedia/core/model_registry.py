"""
Model Registry for efficient model sharing across nodes.

This module provides a Python interface to the Rust model registry,
enabling multiple nodes to share a single loaded model instance.
"""

from typing import Optional, Callable, TypeVar, Generic, Dict, Any
from dataclasses import dataclass
from enum import Enum
import threading
import time
import logging

logger = logging.getLogger(__name__)

T = TypeVar('T')


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


class ModelHandle(Generic[T]):
    """Handle to a loaded model with automatic reference counting"""
    
    def __init__(self, model: T, model_id: str, registry: 'ModelRegistry'):
        self._model = model
        self._model_id = model_id
        self._registry = registry
    
    @property
    def model(self) -> T:
        """Get the model for inference"""
        return self._model
    
    @property
    def model_id(self) -> str:
        """Get model identifier"""
        return self._model_id
    
    def __enter__(self):
        """Context manager entry"""
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit - releases reference"""
        self._registry.release(self._model_id)
    
    def __del__(self):
        """Destructor - ensures reference is released"""
        if hasattr(self, '_registry'):
            try:
                self._registry.release(self._model_id)
            except Exception as e:
                logger.debug(f"Error releasing model handle: {e}")


class ModelRegistry:
    """
    Process-local model registry (singleton).
    
    This registry maintains a single instance of each model per process,
    enabling multiple nodes to share the same model in memory.
    
    Example:
        >>> registry = ModelRegistry()
        >>> def load_whisper():
        ...     from transformers import WhisperModel
        ...     return WhisperModel.from_pretrained("openai/whisper-base")
        >>> 
        >>> handle = registry.get_or_load("whisper-base", load_whisper)
        >>> model = handle.model
    """
    
    _instance: Optional['ModelRegistry'] = None
    _lock = threading.Lock()
    
    def __new__(cls, config: Optional[RegistryConfig] = None):
        """Ensure singleton instance"""
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self, config: Optional[RegistryConfig] = None):
        """Initialize registry (called once)"""
        if self._initialized:
            return
        
        self._config = config or RegistryConfig()
        self._models: Dict[str, Any] = {}
        self._loading: set = set()
        self._lock = threading.Lock()
        self._loading_lock = threading.Lock()
        self._metrics = RegistryMetrics(0, 0, 0, 0, 0)
        self._initialized = True
        
        logger.info("ModelRegistry initialized")
    
    def get_or_load(
        self,
        key: str,
        loader: Callable[[], T],
        session_id: Optional[str] = None
    ) -> ModelHandle[T]:
        """
        Get existing model or load new one.
        
        This method implements singleton semantics - if a model with the given
        key is already loaded, it returns a handle to the existing instance.
        Otherwise, it calls the loader function to create a new instance.
        
        Args:
            key: Unique model identifier (e.g., "whisper-base@cuda:0")
            loader: Function to load the model if not cached
            session_id: Optional session association (for future use)
            
        Returns:
            ModelHandle with reference to the model
            
        Example:
            >>> def load_model():
            ...     return MyModel()
            >>> handle = registry.get_or_load("my-model", load_model)
            >>> model = handle.model
        """
        # Fast path: check if model is already loaded
        with self._lock:
            if key in self._models:
                self._metrics.cache_hits += 1
                logger.debug(f"Model '{key}' found in cache (hit rate: {self._metrics.hit_rate:.2%})")
                return ModelHandle(self._models[key], key, self)
        
        # Slow path: load the model
        # Check if another thread is loading this model
        while True:
            with self._loading_lock:
                if key not in self._loading:
                    # Mark as loading
                    self._loading.add(key)
                    break
            # Wait a bit and check again
            time.sleep(0.01)
        
        try:
            # Load the model
            logger.info(f"Loading model '{key}'...")
            start_time = time.time()
            model = loader()
            load_time = time.time() - start_time
            
            # Estimate memory usage (basic heuristic)
            memory_bytes = self._estimate_memory(model)
            
            # Record cache miss
            with self._lock:
                self._metrics.cache_misses += 1
                self._metrics.total_models += 1
                self._metrics.total_memory_bytes += memory_bytes
                self._models[key] = model
            
            logger.info(
                f"Model '{key}' loaded successfully in {load_time:.2f}s "
                f"(~{memory_bytes / 1024**2:.1f}MB, hit rate: {self._metrics.hit_rate:.2%})"
            )
            
            return ModelHandle(model, key, self)
            
        finally:
            # Remove from loading set
            with self._loading_lock:
                self._loading.discard(key)
    
    def release(self, key: str):
        """
        Release a model reference (decrement reference count).
        
        Note: In Python, actual cleanup happens via garbage collection.
        This method is primarily for tracking and future eviction logic.
        
        Args:
            key: Model identifier to release
        """
        # In Python, we rely on garbage collection
        # This method is here for API compatibility and future eviction logic
        logger.debug(f"Released reference to model '{key}'")
    
    def list_models(self) -> list[ModelInfo]:
        """
        List all loaded models.
        
        Returns:
            List of ModelInfo objects describing loaded models
        """
        with self._lock:
            return [
                ModelInfo(
                    model_id=key,
                    device="unknown",  # TODO: Extract from model if available
                    memory_bytes=self._estimate_memory(model),
                    reference_count=1  # TODO: Track actual references
                )
                for key, model in self._models.items()
            ]
    
    def metrics(self) -> RegistryMetrics:
        """
        Get registry metrics.
        
        Returns:
            RegistryMetrics with current statistics
        """
        with self._lock:
            return RegistryMetrics(
                total_models=self._metrics.total_models,
                total_memory_bytes=self._metrics.total_memory_bytes,
                cache_hits=self._metrics.cache_hits,
                cache_misses=self._metrics.cache_misses,
                evictions=self._metrics.evictions,
            )
    
    def evict_expired(self) -> tuple[int, int]:
        """
        Force eviction of expired models.
        
        Returns:
            Tuple of (evicted_count, freed_memory_bytes)
        """
        # TODO: Implement TTL-based eviction
        return (0, 0)
    
    def clear(self):
        """Clear all models (for testing)"""
        with self._lock:
            self._models.clear()
            self._metrics = RegistryMetrics(0, 0, 0, 0, 0)
        logger.info("Registry cleared")
    
    def _estimate_memory(self, model: Any) -> int:
        """
        Estimate memory usage of a model.
        
        This is a basic heuristic. For more accurate measurements,
        models should implement a memory_usage() method.
        
        Args:
            model: Model instance
            
        Returns:
            Estimated memory in bytes
        """
        if hasattr(model, 'memory_usage'):
            return model.memory_usage()
        
        if hasattr(model, 'get_memory_footprint'):
            return model.get_memory_footprint()
        
        # Fallback: estimate based on parameters (for PyTorch/HuggingFace models)
        try:
            if hasattr(model, 'parameters'):
                # PyTorch model
                total_params = sum(p.numel() for p in model.parameters())
                # Assume float32 (4 bytes per parameter)
                return total_params * 4
        except Exception:
            pass
        
        # Default estimate: 100MB
        return 100 * 1024 * 1024


# Convenience function for module-level access
def get_or_load(key: str, loader: Callable[[], T]) -> T:
    """
    Get or load a model from the global registry.
    
    This is a convenience function that uses the singleton registry
    instance and returns the model directly (not a handle).
    
    Args:
        key: Unique model identifier
        loader: Function to load the model
        
    Returns:
        The loaded model instance
        
    Example:
        >>> def load_whisper():
        ...     from transformers import WhisperModel
        ...     return WhisperModel.from_pretrained("openai/whisper-base")
        >>> 
        >>> model = get_or_load("whisper-base", load_whisper)
        >>> # Second call returns cached instance
        >>> same_model = get_or_load("whisper-base", load_whisper)
        >>> assert model is same_model  # True
    """
    registry = ModelRegistry()
    handle = registry.get_or_load(key, loader)
    return handle.model


# For backward compatibility
__all__ = [
    'ModelRegistry',
    'ModelHandle',
    'RegistryConfig',
    'RegistryMetrics',
    'ModelInfo',
    'EvictionPolicy',
    'get_or_load',
]

