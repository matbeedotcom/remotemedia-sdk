"""
RemoteMedia Processing SDK

A Python SDK for building distributed audio/video/data processing pipelines
with transparent remote offloading capabilities.
"""

import logging
import warnings

__version__ = "0.1.0"
__author__ = "Mathieu Gosbee"
__email__ = "mail@matbee.com"

# Runtime detection (Phase 8 - T130-T132)
_rust_runtime_available = False
_rust_runtime = None

logger = logging.getLogger(__name__)

def try_load_rust_runtime():
    """
    Attempt to load the Rust runtime module.
    
    Returns:
        tuple: (success: bool, module: Optional[Module], error: Optional[str])
    
    Example:
        >>> success, runtime, error = try_load_rust_runtime()
        >>> if success:
        ...     print(f"Rust runtime v{runtime.__version__} loaded")
        ... else:
        ...     print(f"Rust runtime unavailable: {error}")
    """
    try:
        import remotemedia_runtime
        return (True, remotemedia_runtime, None)
    except ImportError as e:
        return (False, None, f"Module not found: {e}")
    except Exception as e:
        return (False, None, f"Failed to load: {e}")

def is_rust_runtime_available():
    """
    Check if the Rust runtime is available.
    
    Returns:
        bool: True if Rust runtime can be imported, False otherwise
    
    Example:
        >>> if is_rust_runtime_available():
        ...     print("Using Rust acceleration")
        ... else:
        ...     print("Using Python fallback")
    """
    global _rust_runtime_available, _rust_runtime
    
    if _rust_runtime is not None:
        return _rust_runtime_available
    
    success, runtime, error = try_load_rust_runtime()
    _rust_runtime_available = success
    _rust_runtime = runtime
    
    if success:
        logger.info(f"Rust runtime v{runtime.__version__} loaded successfully")
    else:
        logger.debug(f"Rust runtime not available: {error}")
        warnings.warn(
            f"Rust runtime unavailable, falling back to Python execution. "
            f"Install remotemedia-runtime for 50-100x performance improvement. "
            f"Reason: {error}",
            UserWarning,
            stacklevel=2
        )
    
    return _rust_runtime_available

def get_rust_runtime():
    """
    Get the Rust runtime module if available.
    
    Returns:
        Module or None: The remotemedia_runtime module if available
    
    Example:
        >>> runtime = get_rust_runtime()
        >>> if runtime:
        ...     print(f"Runtime version: {runtime.__version__}")
    """
    global _rust_runtime
    
    if _rust_runtime is None:
        is_rust_runtime_available()  # Trigger loading
    
    return _rust_runtime

# Check runtime availability on module import (silent)
try:
    _rust_runtime_available, _rust_runtime, _ = try_load_rust_runtime()
except Exception:
    pass  # Silent failure, warnings will be shown when used

# Core imports
from .core.pipeline import Pipeline
from .core.node import Node, RemoteExecutorConfig
from .core.exceptions import (
    RemoteMediaError,
    PipelineError,
    NodeError,
    RemoteExecutionError,
    WebRTCError,
)

# Convenience imports
from .nodes import *  # noqa: F401, F403

# Optional import: WebRTCManager might require aiortc for some features
try:
    from .webrtc.manager import WebRTCManager
    _has_webrtc = True
except ImportError:
    WebRTCManager = None
    _has_webrtc = False

__all__ = [
    # Core classes
    "Pipeline",
    "Node",
    "RemoteExecutorConfig",
    # Exceptions
    "RemoteMediaError",
    "PipelineError", 
    "NodeError",
    "RemoteExecutionError",
    "WebRTCError",
    # Runtime detection (Phase 8)
    "is_rust_runtime_available",
    "get_rust_runtime",
    "try_load_rust_runtime",
    # Version info
    "__version__",
    "__author__",
    "__email__",
]

# Add optional WebRTC support if available
if _has_webrtc:
    __all__.append("WebRTCManager") 