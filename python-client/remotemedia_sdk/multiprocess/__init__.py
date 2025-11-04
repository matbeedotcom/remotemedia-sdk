"""
Multiprocess support for RemoteMedia SDK.

This module enables running Python nodes in separate processes with
independent GILs for true concurrent execution.
"""

from typing import TYPE_CHECKING

__all__ = [
    "MultiprocessNode",
    "RuntimeData",
    "DataType",
    "AudioMetadata",
    "VideoMetadata",
    "Publisher",
    "Subscriber",
    "Session",
    "Pipeline",
    "register_node",
]

# Lazy imports to avoid circular dependencies
def _lazy_import():
    """Lazy import of module components."""
    global MultiprocessNode, RuntimeData, DataType, AudioMetadata, VideoMetadata
    global Publisher, Subscriber, Session, Pipeline

    from .node import MultiprocessNode
    from .data import RuntimeData, DataType, AudioMetadata, VideoMetadata
    from .channel import Publisher, Subscriber
    # These will be implemented in later phases
    # from .session import Session
    # from .pipeline import Pipeline

# Type checking imports
if TYPE_CHECKING:
    from .node import MultiprocessNode
    from .data import RuntimeData, DataType, AudioMetadata, VideoMetadata
    from .channel import Publisher, Subscriber
    # from .session import Session
    # from .pipeline import Pipeline
else:
    # Perform lazy import on first access
    MultiprocessNode = None
    RuntimeData = None
    DataType = None
    AudioMetadata = None
    VideoMetadata = None

# Registry for node types
_NODE_REGISTRY = {}


def register_node(node_type: str):
    """
    Decorator to register a node class for multiprocess execution.

    Args:
        node_type: Unique identifier for the node type

    Example:
        @register_node("audio_processor")
        class AudioProcessorNode(MultiprocessNode):
            ...
    """
    def decorator(cls):
        _NODE_REGISTRY[node_type] = cls
        cls._node_type = node_type
        return cls
    return decorator


def get_node_class(node_type: str):
    """
    Get a registered node class by its type identifier.

    Args:
        node_type: The node type identifier

    Returns:
        The node class

    Raises:
        KeyError: If node type is not registered
    """
    if node_type not in _NODE_REGISTRY:
        raise KeyError(f"Node type '{node_type}' not registered")
    return _NODE_REGISTRY[node_type]