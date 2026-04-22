"""Multiprocessing infrastructure for RemoteMedia nodes."""

from .node import MultiprocessNode, NodeConfig, NodeStatus
from .data import RuntimeData, DataType, AudioMetadata, VideoMetadata

__all__ = [
    "MultiprocessNode",
    "NodeConfig",
    "NodeStatus",
    "RuntimeData",
    "DataType",
    "AudioMetadata",
    "VideoMetadata",
    "register_node",
    "get_node_class",
    "list_registered_nodes",
    "python_requires",
    "get_node_requirements",
]

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


def python_requires(deps: list):
    """
    Decorator to declare Python package requirements for a node.

    These requirements are reported to the Rust runtime via the control
    channel, allowing automatic venv provisioning.

    Can be combined with @register_node:
        @register_node("whisper")
        @python_requires(["torch>=2.0", "openai-whisper"])
        class WhisperNode(MultiprocessNode):
            ...

    Args:
        deps: List of PEP 508 dependency strings (e.g. ["torch>=2.0", "numpy"])
    """
    def decorator(cls):
        cls.__python_requires__ = list(deps)
        return cls
    return decorator


def get_node_requirements(node_type: str) -> list:
    """
    Get Python package requirements for a registered node type.

    Returns:
        List of PEP 508 dependency strings, or empty list if none declared.
    """
    cls = get_node_class(node_type)
    return getattr(cls, '__python_requires__', [])


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
    # Auto-register built-in Python AI nodes on first access
    _auto_register_builtin_nodes()

    if node_type not in _NODE_REGISTRY:
        raise KeyError(f"Node type '{node_type}' not registered. Available: {list(_NODE_REGISTRY.keys())}")
    return _NODE_REGISTRY[node_type]


def list_registered_nodes():
    """
    List all registered node types.

    Returns:
        List of registered node type identifiers
    """
    _auto_register_builtin_nodes()
    return list(_NODE_REGISTRY.keys())


# Track if built-in nodes have been registered
_BUILTIN_REGISTERED = False


def _auto_register_builtin_nodes():
    """
    Auto-register built-in Python AI nodes that inherit from MultiprocessNode.

    This allows them to be used in multiprocess pipelines without explicit registration.
    """
    global _BUILTIN_REGISTERED

    if _BUILTIN_REGISTERED:
        return

    _BUILTIN_REGISTERED = True

    # Register built-in AI nodes
    try:
        from remotemedia.nodes.tts import KokoroTTSNode
        _NODE_REGISTRY["KokoroTTSNode"] = KokoroTTSNode
    except ImportError:
        pass  # Kokoro not installed

    try:
        from remotemedia.nodes.tts_vibevoice import VibeVoiceTTSNode
        _NODE_REGISTRY["VibeVoiceTTSNode"] = VibeVoiceTTSNode
        _NODE_REGISTRY["VibeVoiceNode"] = VibeVoiceTTSNode  # Alias
    except ImportError:
        pass  # VibeVoice not installed

    try:
        from remotemedia.nodes.ml.lfm2_audio import LFM2AudioNode
        _NODE_REGISTRY["LFM2AudioNode"] = LFM2AudioNode
        _NODE_REGISTRY["LFM2Node"] = LFM2AudioNode  # Alias
    except (ImportError, OSError):
        pass  # LFM2 audio or torchaudio ABI missing

    try:
        from remotemedia.nodes.ml.lfm2_text import LFM2TextNode
        _NODE_REGISTRY["LFM2TextNode"] = LFM2TextNode
    except (ImportError, OSError):
        pass  # torch / transformers missing

    # Register test nodes
    try:
        from remotemedia.nodes.test_echo import EchoNode
        _NODE_REGISTRY["EchoNode"] = EchoNode
    except ImportError:
        pass  # Test node not available

    # Log registered nodes
    if _NODE_REGISTRY:
        import logging
        logger = logging.getLogger(__name__)
        logger.info(f"Auto-registered {len(_NODE_REGISTRY)} built-in nodes: {list(_NODE_REGISTRY.keys())}")
