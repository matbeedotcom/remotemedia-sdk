"""Multiprocessing infrastructure for RemoteMedia nodes."""

from .node import MultiprocessNode, NodeConfig, NodeStatus
from .data import RuntimeData, DataType, AudioMetadata, VideoMetadata
from .audio_pressure import AudioPressureMixin

__all__ = [
    "MultiprocessNode",
    "NodeConfig",
    "NodeStatus",
    "RuntimeData",
    "DataType",
    "AudioMetadata",
    "VideoMetadata",
    "AudioPressureMixin",
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

    import logging as _logging
    _logger = _logging.getLogger(__name__)

    # Per-venv auto-registration: any ML / TTS module may raise on
    # import for reasons other than ImportError (API rename →
    # AttributeError, native-lib ABI skew → OSError, transformers
    # version mismatch → RuntimeError, ...). Catching ONLY ImportError
    # made these failures silent — the caller ended up with an empty
    # registry and a bare KeyError. Catch everything, log what
    # happened, keep going.
    def _try_register(module_path, aliases):
        """Import `module_path.name` as `_NODE_REGISTRY[name]` for each alias.
        `aliases` is a list of (symbol_name, registry_key) pairs."""
        try:
            mod = __import__(module_path, fromlist=[a[0] for a in aliases])
        except BaseException as exc:  # noqa: BLE001
            _logger.warning(
                "auto-register skipped %s (%s): %s",
                module_path, type(exc).__name__, exc,
            )
            return
        for symbol, key in aliases:
            cls = getattr(mod, symbol, None)
            if cls is not None:
                _NODE_REGISTRY[key] = cls

    _try_register("remotemedia.nodes.tts", [("KokoroTTSNode", "KokoroTTSNode")])
    _try_register(
        "remotemedia.nodes.tts_vibevoice",
        [("VibeVoiceTTSNode", "VibeVoiceTTSNode"), ("VibeVoiceTTSNode", "VibeVoiceNode")],
    )
    _try_register(
        "remotemedia.nodes.tts_cosyvoice3",
        [("CosyVoice3TTSNode", "CosyVoice3TTSNode")],
    )
    _try_register(
        "remotemedia.nodes.tts_voxtral",
        [("VoxtralTTSNode", "VoxtralTTSNode")],
    )
    _try_register(
        "remotemedia.nodes.ml.lfm2_audio",
        [("LFM2AudioNode", "LFM2AudioNode"), ("LFM2AudioNode", "LFM2Node")],
    )
    _try_register(
        "remotemedia.nodes.ml.lfm2_audio_mlx",
        [
            ("LFM2AudioMlxNode", "LFM2AudioMlxNode"),
            ("LFM2AudioMlxNode", "LFM2AudioMLX"),
        ],
    )
    _try_register(
        "remotemedia.nodes.ml.lfm2_text",
        [("LFM2TextNode", "LFM2TextNode")],
    )
    _try_register(
        "remotemedia.nodes.ml.whisper_stt",
        [("WhisperSTTNode", "WhisperSTTNode")],
    )
    _try_register("remotemedia.nodes.test_echo", [("EchoNode", "EchoNode")])

    if _NODE_REGISTRY:
        _logger.info(
            "Auto-registered %d built-in nodes: %s",
            len(_NODE_REGISTRY), list(_NODE_REGISTRY.keys()),
        )
    else:
        _logger.warning(
            "Auto-register produced an empty _NODE_REGISTRY; every builtin "
            "module failed to import. See the WARN lines above for why."
        )
