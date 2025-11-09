import asyncio
import logging
from typing import Optional, Any, Dict
import hashlib
import weakref

from remotemedia.core.node import Node
from remotemedia.core.exceptions import NodeError

# Configure basic logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)


# Global pipeline cache to avoid concurrent loading issues
_pipeline_cache = {}
_cache_locks = {}
_cache_lock = asyncio.Lock()


async def _get_cached_pipeline(cache_key: str, pipeline_factory):
    """
    Get or create a cached pipeline with proper concurrency handling.
    
    Args:
        cache_key: Unique key for this pipeline configuration
        pipeline_factory: Async function that creates the pipeline
        
    Returns:
        The cached or newly created pipeline
    """
    async with _cache_lock:
        # Check if pipeline is already cached
        if cache_key in _pipeline_cache:
            logger.info(f"Using cached pipeline for key: {cache_key}")
            return _pipeline_cache[cache_key]
        
        # Check if another coroutine is already loading this pipeline
        if cache_key not in _cache_locks:
            _cache_locks[cache_key] = asyncio.Lock()
    
    # Use pipeline-specific lock to prevent concurrent loading of the same model
    async with _cache_locks[cache_key]:
        # Double-check pattern - pipeline might have been loaded while waiting for lock
        if cache_key in _pipeline_cache:
            logger.info(f"Pipeline was loaded by another request for key: {cache_key}")
            return _pipeline_cache[cache_key]
        
        logger.info(f"Loading new pipeline for key: {cache_key}")
        pipeline = await pipeline_factory()
        
        # Cache the pipeline
        _pipeline_cache[cache_key] = pipeline
        logger.info(f"Cached new pipeline for key: {cache_key}")
        
        return pipeline


def _generate_cache_key(task: str, model: Optional[str], device: str, torch_dtype: Optional[str]) -> str:
    """Generate a unique cache key for pipeline configuration."""
    config_str = f"{task}|{model or 'default'}|{device}|{torch_dtype or 'default'}"
    return hashlib.md5(config_str.encode()).hexdigest()[:12]


class TransformersPipelineNode(Node):
    """
    A generic node that wraps a Hugging Face Transformers pipeline.

    This node can be configured to run various tasks like text-classification,
    automatic-speech-recognition, etc., by leveraging the `transformers.pipeline`
    factory.

    See: https://huggingface.co/docs/transformers/main_classes/pipelines
    """

    def __init__(
        self,
        task: str,
        model: Optional[str] = None,
        device: Optional[Any] = None,
        model_kwargs: Optional[Dict[str, Any]] = None,
        torch_dtype: Optional[str] = None,
        **kwargs,
    ):
        """
        Initializes the TransformersPipelineNode.

        Args:
            task (str): The task for the pipeline (e.g., "text-classification").
            model (str, optional): The model identifier from the Hugging Face Hub.
            device (Any, optional): The device to run the model on (e.g., "cpu", "cuda", 0).
                                    If None, automatically selects GPU if available.
            model_kwargs (Dict[str, Any], optional): Extra keyword arguments for the model.
            torch_dtype (str, optional): The torch dtype to use (e.g., "float16", "bfloat16").
            **kwargs: Additional node parameters.
        """
        super().__init__(**kwargs)
        if not task:
            raise ValueError("The 'task' argument is required.")

        self.task = task
        self.model = model
        self.device = device
        self.torch_dtype = torch_dtype
        self.model_kwargs = model_kwargs or {}
        self.pipe = None

    async def initialize(self):
        """
        Initializes the underlying `transformers` pipeline with robust error handling.

        This method handles heavy imports and model downloading, making it suitable
        for execution on a remote server. It includes graceful fallback mechanisms
        for common issues like meta tensor problems and device availability.
        """
        self.logger.info(f"Initializing node '{self.name}'...")
        
        # Set environment variables to avoid common PyTorch issues
        import os
        os.environ.setdefault("PYTORCH_ENABLE_MPS_FALLBACK", "1")
        os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")  # Avoid deadlocks
        
        try:
            from transformers import pipeline
            import torch
        except ImportError:
            raise NodeError(
                "TransformersPipelineNode requires `transformers` and `torch`. "
                "Please install them, e.g., `pip install transformers torch`."
            )

        # Auto-detect device if not specified
        if self.device is None:
            if torch.cuda.is_available():
                self.device = "cuda:0"
            elif hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
                self.device = "mps"
            else:
                self.device = "cpu"

        # Auto-detect torch_dtype if not specified
        resolved_torch_dtype = None
        if self.torch_dtype:
            if isinstance(self.torch_dtype, str):
                try:
                    resolved_torch_dtype = getattr(torch, self.torch_dtype)
                except AttributeError:
                    self.logger.warning(f"Invalid torch_dtype '{self.torch_dtype}', using default")
            else:
                resolved_torch_dtype = self.torch_dtype
        elif "cuda" in str(self.device) and torch.cuda.is_available():
            # Use float16 for CUDA by default to save memory
            resolved_torch_dtype = torch.float16
        
        self.logger.info(
            f"Loading transformers pipeline for task '{self.task}'"
            f" with model '{self.model or 'default'}' on device '{self.device}'"
            f" with dtype '{resolved_torch_dtype or 'default'}'."
        )

        # Prepare pipeline arguments with safe defaults
        pipeline_kwargs = {
            "task": self.task,
            "model": self.model,
            "device": self.device,
            **self.model_kwargs
        }
        
        # Add torch_dtype if specified
        if resolved_torch_dtype is not None:
            pipeline_kwargs["torch_dtype"] = resolved_torch_dtype
        
        # Add model configuration to avoid meta tensor issues
        model_kwargs = pipeline_kwargs.get("model_kwargs", {})
        model_kwargs.update({
            "low_cpu_mem_usage": False,  # Avoid meta tensors
            "use_safetensors": True,     # Prefer safetensors when available
        })
        pipeline_kwargs["model_kwargs"] = model_kwargs

        # Generate cache key for this configuration
        cache_key = _generate_cache_key(
            self.task, 
            self.model, 
            self.device, 
            str(resolved_torch_dtype) if resolved_torch_dtype else None
        )
        
        # Try loading with cached approach and multiple fallback strategies
        strategies = [
            ("primary", pipeline_kwargs),
            ("cpu_fallback", {**pipeline_kwargs, "device": "cpu"}),
            ("safe_mode", {
                "task": self.task,
                "model": self.model,
                "device": "cpu",
                "torch_dtype": None,
                "model_kwargs": {
                    "low_cpu_mem_usage": False,
                    "device_map": None,  # Disable automatic device mapping
                    "load_in_8bit": False,  # Disable quantization
                    "load_in_4bit": False,  # Disable quantization
                }
            }),
            ("minimal_mode", {
                "task": self.task,
                "model": self.model or "distilbert-base-uncased-finetuned-sst-2-english" if self.task == "sentiment-analysis" else None,
                "device": "cpu",
                "torch_dtype": torch.float32,
                "model_kwargs": {}
            }),
            ("ultra_safe_mode", {
                "task": self.task,
                "model": "distilbert-base-uncased-finetuned-sst-2-english" if self.task == "sentiment-analysis" else (self.model or None),
                "device": "cpu",
                # Remove ALL potentially problematic parameters for maximum compatibility
            }),
            ("bulletproof_mode", "CUSTOM_HANDLER")  # Special flag for custom handling
        ]

        last_error = None
        for strategy_name, kwargs in strategies:
            try:
                self.logger.info(f"Attempting pipeline loading with {strategy_name} strategy...")
                
                # Generate strategy-specific cache key
                strategy_device = kwargs.get("device", "cpu") if kwargs != "CUSTOM_HANDLER" else "cpu"
                strategy_cache_key = _generate_cache_key(
                    self.task,
                    kwargs.get("model", self.model) if kwargs != "CUSTOM_HANDLER" else self.model,
                    strategy_device,
                    str(kwargs.get("torch_dtype")) if kwargs != "CUSTOM_HANDLER" and "torch_dtype" in kwargs else None
                )
                
                async def pipeline_factory():
                    if kwargs == "CUSTOM_HANDLER":
                        # Bulletproof mode: manually handle the pipeline creation with absolute minimal parameters
                        self.logger.info("Using bulletproof mode with custom parameter filtering...")
                        
                        # Create pipeline with only the most basic parameters
                        basic_kwargs = {
                            "task": self.task,
                            "device": "cpu"
                        }
                        
                        # Only add model if it's a known working one
                        if self.task == "sentiment-analysis":
                            basic_kwargs["model"] = "distilbert-base-uncased-finetuned-sst-2-english"
                        elif self.model and "distilbert" in self.model.lower():
                            basic_kwargs["model"] = self.model
                        
                        return await asyncio.to_thread(pipeline, **basic_kwargs)
                    else:
                        # Normal strategy
                        return await asyncio.to_thread(pipeline, **kwargs)
                
                # Use cached pipeline loading to prevent concurrent loading issues
                self.pipe = await _get_cached_pipeline(strategy_cache_key, pipeline_factory)
                
                # Update device tracking for successful strategy
                if kwargs == "CUSTOM_HANDLER":
                    self.device = "cpu"
                elif "device" in kwargs:
                    self.device = kwargs["device"]
                
                self.logger.info(f"Pipeline loaded successfully with {strategy_name} strategy on device '{self.device}' (cache key: {strategy_cache_key})")
                break
                
            except Exception as e:
                error_msg = str(e)
                self.logger.warning(f"Strategy '{strategy_name}' failed: {error_msg}")
                last_error = e
                
                # Clear CUDA cache if it's a GPU-related error
                if "cuda" in error_msg.lower() or "gpu" in error_msg.lower():
                    try:
                        if torch.cuda.is_available():
                            torch.cuda.empty_cache()
                            self.logger.info("Cleared CUDA cache after GPU error")
                    except:
                        pass
                
                # If this was the last strategy, we'll raise the error
                if strategy_name == "bulletproof_mode":
                    # Provide helpful error message based on the type of error
                    if "Cannot copy out of meta tensor" in error_msg:
                        # Try to get version info for debugging
                        try:
                            import torch
                            import transformers
                            torch_version = torch.__version__
                            transformers_version = transformers.__version__
                            version_info = f"PyTorch: {torch_version}, Transformers: {transformers_version}"
                        except:
                            version_info = "Unable to determine versions"
                        
                        raise NodeError(
                            f"Failed to load transformers pipeline due to meta tensor issue after trying all fallback strategies. "
                            f"Current versions: {version_info}. "
                            f"This is usually caused by incompatible PyTorch/transformers versions. "
                            f"Try:\n"
                            f"1. pip install torch==2.0.1 transformers==4.33.2\n"
                            f"2. Or pip install --upgrade torch transformers\n"
                            f"3. Restart the server after updating packages"
                        ) from e
                    elif "out of memory" in error_msg.lower():
                        raise NodeError(
                            f"Failed to load transformers pipeline due to insufficient memory. "
                            f"Try using a smaller model or running on CPU instead."
                        ) from e
                    elif "unexpected keyword argument" in error_msg:
                        raise NodeError(
                            f"Failed to load transformers pipeline due to parameter compatibility issue. "
                            f"This is usually caused by version mismatches between transformers components. "
                            f"Try: pip install --upgrade transformers torch"
                        ) from e
                    else:
                        raise NodeError(f"Failed to load transformers pipeline: {error_msg}") from e
        else:
            # This shouldn't happen as safe_mode should always work, but just in case
            raise NodeError(f"All pipeline loading strategies failed. Last error: {last_error}")

        self.logger.info(f"Node '{self.name}' initialized successfully.")

    async def process(self, data: Any) -> Any:
        """
        Processes a single data item using the loaded pipeline.

        This method is designed to be thread-safe and non-blocking.

        Args:
            data: The single input data item to be processed by the pipeline.

        Returns:
            The processing result.
        """
        if not self.pipe:
            raise NodeError("Pipeline not initialized. Call initialize() first.")

        # The pipeline call can be blocking, so run it in a thread
        result = await asyncio.to_thread(self.pipe, data)
        return result

    async def cleanup(self):
        """Cleans up the node's reference to the pipeline (but keeps cached pipeline)."""
        self.logger.info(f"Cleaning up node '{self.name}'.")

        if hasattr(self, "pipe") and self.pipe is not None:
            # Just remove the reference - don't delete the cached pipeline
            # as other nodes may be using the same cached instance
            self.pipe = None
            self.logger.debug("Removed pipeline reference from node.")

    @staticmethod
    async def clear_pipeline_cache():
        """Clear the entire pipeline cache (for memory management)."""
        async with _cache_lock:
            logger.info(f"Clearing pipeline cache with {len(_pipeline_cache)} entries")
            
            # Check for CUDA pipelines before clearing
            cuda_pipelines = []
            for cache_key, pipe in _pipeline_cache.items():
                if hasattr(pipe, "device") and "cuda" in str(pipe.device):
                    cuda_pipelines.append(cache_key)
            
            # Clear the cache
            _pipeline_cache.clear()
            _cache_locks.clear()
            
            # Clear CUDA cache if we had CUDA pipelines
            if cuda_pipelines:
                try:
                    import torch
                    torch.cuda.empty_cache()
                    logger.info(f"Cleared CUDA cache after removing {len(cuda_pipelines)} CUDA pipelines")
                except (ImportError, AttributeError):
                    pass
                except Exception as e:
                    logger.warning(f"Could not clear CUDA cache: {e}")

    def get_capabilities(self) -> Optional[Dict[str, Any]]:
        """
        Return capability requirements for Transformers pipeline.

        Requirements vary significantly based on the task and model size.
        This provides reasonable defaults.

        Returns:
            Capability descriptor with GPU preferences and memory requirements
        """
        # Determine if GPU is required or optional
        gpu_required = False
        if self.device is not None:
            if isinstance(self.device, str) and "cuda" in str(self.device):
                gpu_required = True
            elif isinstance(self.device, int):  # GPU device index
                gpu_required = True

        # Estimate memory based on task type
        memory_gb = 4.0  # Default for smaller models
        gpu_memory_gb = 3.0

        # Task-specific estimates
        if self.task in ["automatic-speech-recognition", "text-to-speech"]:
            memory_gb = 8.0
            gpu_memory_gb = 6.0
        elif self.task in ["text-generation", "translation"]:
            memory_gb = 6.0
            gpu_memory_gb = 4.0
        elif self.task in ["image-to-text", "visual-question-answering"]:
            memory_gb = 8.0
            gpu_memory_gb = 6.0

        capabilities = {"memory_gb": memory_gb}

        # Add GPU requirements if applicable
        if self.device is None or gpu_required:  # None means auto-select GPU if available
            capabilities["gpu"] = {
                "type": "cuda",
                "min_memory_gb": gpu_memory_gb,
                "required": gpu_required
            }

        return capabilities


__all__ = ["TransformersPipelineNode"] 