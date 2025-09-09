"""
Core components of the RemoteMedia SDK.

This module contains the fundamental classes and utilities for building
processing pipelines.
"""

from .pipeline import Pipeline
from .node import Node, RemoteExecutorConfig
from .exceptions import (
    RemoteMediaError,
    PipelineError,
    NodeError,
    RemoteExecutionError,
    WebRTCError,
)

__all__ = [
    "Pipeline",
    "Node", 
    "RemoteExecutorConfig",
    "RemoteMediaError",
    "PipelineError",
    "NodeError",
    "RemoteExecutionError",
    "WebRTCError",
] 