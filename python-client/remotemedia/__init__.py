"""
RemoteMedia Processing SDK

A Python SDK for building distributed audio/video/data processing pipelines
with transparent remote offloading capabilities.
"""

__version__ = "0.1.0"
__author__ = "RemoteMedia Team"
__email__ = "team@remotemedia.dev"

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
from .webrtc.manager import WebRTCManager

__all__ = [
    # Core classes
    "Pipeline",
    "Node",
    "RemoteExecutorConfig",
    "WebRTCManager",
    # Exceptions
    "RemoteMediaError",
    "PipelineError", 
    "NodeError",
    "RemoteExecutionError",
    "WebRTCError",
    # Version info
    "__version__",
    "__author__",
    "__email__",
] 