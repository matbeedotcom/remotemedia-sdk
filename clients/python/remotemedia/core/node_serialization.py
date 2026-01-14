"""
Node instance serialization for multiprocess execution.

Feature 011 - User Story 3: Instance Serialization for IPC

This module provides serialization/deserialization of Node instances using
cloudpickle, with proper lifecycle management (cleanup before pickle,
initialize after unpickle).
"""

import logging
import cloudpickle
from typing import Optional

logger = logging.getLogger(__name__)

# Size limit for serialized nodes (~100MB)
MAX_SERIALIZED_SIZE_BYTES = 100 * 1024 * 1024  # 100 MB


class SerializationError(Exception):
    """
    Raised when Node instance cannot be serialized.

    T045: Exception class for serialization failures with helpful context.
    """

    def __init__(self, node_name: str, reason: str, suggestion: Optional[str] = None):
        """
        Create a serialization error with context.

        Args:
            node_name: Name of the node that failed to serialize
            reason: Description of why serialization failed
            suggestion: Optional suggestion for fixing the issue
        """
        message = f"Cannot serialize Node '{node_name}': {reason}"
        if suggestion:
            message += f"\nSuggestion: {suggestion}"

        super().__init__(message)
        self.node_name = node_name
        self.reason = reason
        self.suggestion = suggestion


def serialize_node_for_ipc(node: 'Node') -> bytes:
    """
    Serialize a Node instance for IPC transfer to subprocess.

    T038-T040, T044, T046-T047: Complete serialization workflow:
    1. Validate node is ready for serialization
    2. Call node.cleanup() to release resources
    3. Serialize with cloudpickle
    4. Validate size limit
    5. Return bytes for IPC transfer

    Args:
        node: Node instance to serialize

    Returns:
        bytes: cloudpickle-serialized node data

    Raises:
        SerializationError: If serialization fails or size limit exceeded

    Example:
        >>> from remotemedia.nodes import PassThroughNode
        >>> node = PassThroughNode(name="test")
        >>> serialized = serialize_node_for_ipc(node)
        >>> # Transfer via IPC...
        >>> restored = deserialize_node_from_ipc(serialized)
    """
    from remotemedia.core.node import Node

    if not isinstance(node, Node):
        raise TypeError(f"Expected Node instance, got {type(node).__name__}")

    node_name = node.name

    try:
        # T046: Validate node state before serialization
        if node._is_initialized:
            logger.warning(
                f"Node '{node_name}' is initialized before serialization. "
                f"Calling cleanup() to release resources."
            )

        # T039: Call cleanup() before serialization
        try:
            node.cleanup()
            logger.debug(f"Called cleanup() on node '{node_name}' before serialization")
        except Exception as e:
            logger.warning(f"cleanup() failed for node '{node_name}': {e}")
            # Continue anyway - cleanup might fail if already clean

        # T038, T040: Serialize with cloudpickle
        logger.debug(f"Serializing node '{node_name}' with cloudpickle")
        serialized_bytes = cloudpickle.dumps(node)

        # T047: Check size limit
        size_mb = len(serialized_bytes) / (1024 * 1024)
        logger.info(f"Serialized node '{node_name}': {size_mb:.2f} MB")

        if len(serialized_bytes) > MAX_SERIALIZED_SIZE_BYTES:
            raise SerializationError(
                node_name,
                f"Serialized size ({size_mb:.2f} MB) exceeds limit ({MAX_SERIALIZED_SIZE_BYTES / (1024*1024):.0f} MB)",
                "Reduce node state size or implement custom __getstate__/__setstate__ methods"
            )

        return serialized_bytes

    except SerializationError:
        # Re-raise our custom errors
        raise

    except Exception as e:
        # T044: Wrap other exceptions with helpful context
        error_type = type(e).__name__
        error_msg = str(e)

        # Try to identify problematic attributes
        suggestion = (
            "Ensure node.cleanup() releases all non-serializable resources. "
            "Implement __getstate__/__setstate__ methods to handle complex state."
        )

        # Check for common serialization issues
        if "cannot pickle" in error_msg.lower():
            suggestion = (
                "The node contains non-serializable objects (file handles, locks, connections, etc.). "
                "Implement __getstate__() to exclude non-serializable attributes."
            )
        elif "circular reference" in error_msg.lower():
            suggestion = "The node has circular references. Implement __getstate__() to break the cycle."

        raise SerializationError(
            node_name,
            f"{error_type}: {error_msg}",
            suggestion
        ) from e


def deserialize_node_from_ipc(serialized_bytes: bytes) -> 'Node':
    """
    Deserialize a Node instance from IPC transfer.

    T042-T043: Complete deserialization workflow:
    1. Deserialize with cloudpickle
    2. Call node.initialize() to recreate resources
    3. Return ready-to-use node

    Args:
        serialized_bytes: cloudpickle-serialized node data

    Returns:
        Node: Deserialized and initialized node instance

    Raises:
        SerializationError: If deserialization fails
        RuntimeError: If initialize() fails

    Example:
        >>> serialized = serialize_node_for_ipc(node)
        >>> # ... transfer via IPC ...
        >>> restored_node = deserialize_node_from_ipc(serialized)
        >>> # Node is ready to use
    """
    try:
        # T042: Deserialize with cloudpickle
        logger.debug(f"Deserializing node from {len(serialized_bytes)} bytes")
        node = cloudpickle.loads(serialized_bytes)

        # Validate it's a Node
        from remotemedia.core.node import Node
        if not isinstance(node, Node):
            raise TypeError(f"Deserialized object is not a Node, got {type(node).__name__}")

        logger.debug(f"Deserialized node: {node.name}")

        # T043: Call initialize() after deserialization
        try:
            node.initialize()
            logger.debug(f"Initialized node '{node.name}' after deserialization")
        except Exception as e:
            logger.error(f"Failed to initialize node '{node.name}' after deserialization: {e}")
            raise RuntimeError(
                f"Node '{node.name}' deserialized successfully but initialize() failed: {e}"
            ) from e

        return node

    except (SerializationError, RuntimeError):
        # Re-raise our custom errors
        raise

    except Exception as e:
        # Wrap other exceptions
        raise SerializationError(
            "unknown",
            f"Deserialization failed: {type(e).__name__}: {e}",
            "Ensure Python version and dependencies match between processes"
        ) from e


def validate_node_serializable(node: 'Node') -> bool:
    """
    Validate that a node is ready for serialization.

    Checks for common non-serializable attributes and warns if found.

    Args:
        node: Node to validate

    Returns:
        bool: True if node appears serializable

    Raises:
        ValueError: If node has known non-serializable state
    """
    warnings = []

    # Check if initialized (should call cleanup first)
    if hasattr(node, '_is_initialized') and node._is_initialized:
        warnings.append("Node is initialized (call cleanup() first)")

    # Check for common non-serializable attributes
    for attr_name in ['_lock', '_thread', '_file_handle', '_connection', '_socket']:
        if hasattr(node, attr_name):
            attr_value = getattr(node, attr_name)
            if attr_value is not None:
                warnings.append(f"Node has non-serializable attribute: {attr_name}")

    if warnings:
        logger.warning(f"Node '{node.name}' may not be serializable: {', '.join(warnings)}")
        return False

    return True


__all__ = [
    'SerializationError',
    'serialize_node_for_ipc',
    'deserialize_node_from_ipc',
    'validate_node_serializable',
    'MAX_SERIALIZED_SIZE_BYTES',
]
