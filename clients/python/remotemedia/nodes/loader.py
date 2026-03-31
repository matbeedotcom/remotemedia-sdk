"""
Python node loader - register nodes from file paths.

This module provides a simple way to register Python nodes by file path,
enabling developers to add custom nodes without modifying the core SDK.

Usage:
    from remotemedia import register_python_node

    # Register all MultiprocessNode subclasses from a file
    register_python_node("./nodes/my_ml_node.py")

    # Register with explicit node type name
    register_python_node("./nodes/custom.py", node_type="MyCustomNode")

    # Register with options
    register_python_node(
        "./nodes/my_tts.py",
        node_type="MyTTS",
        multi_output=True,
        category="tts"
    )
"""

import importlib.util
import inspect
import logging
import os
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Type

logger = logging.getLogger(__name__)

# Global registry of loaded Python nodes
_LOADED_NODES: Dict[str, Dict[str, Any]] = {}


def _load_module_from_path(file_path: str) -> Any:
    """Load a Python module from a file path."""
    path = Path(file_path).resolve()
    
    if not path.exists():
        raise FileNotFoundError(f"Python node file not found: {path}")
    
    if not path.suffix == ".py":
        raise ValueError(f"Expected .py file, got: {path}")
    
    # Create a unique module name based on the file path
    module_name = f"_remotemedia_node_{path.stem}_{hash(str(path)) & 0xFFFFFFFF}"
    
    # Load the module
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Could not load module from: {path}")
    
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    
    return module


def _find_node_classes(module: Any) -> List[Type]:
    """Find all MultiprocessNode subclasses in a module."""
    from remotemedia.core import MultiprocessNode
    
    node_classes = []
    
    for name, obj in inspect.getmembers(module, inspect.isclass):
        # Skip imported classes (only want classes defined in this module)
        if obj.__module__ != module.__name__:
            continue
        
        # Check if it's a MultiprocessNode subclass (but not MultiprocessNode itself)
        if issubclass(obj, MultiprocessNode) and obj is not MultiprocessNode:
            node_classes.append(obj)
    
    return node_classes


def register_python_node(
    file_path: str,
    *,
    node_type: Optional[str] = None,
    node_class: Optional[str] = None,
    multi_output: bool = False,
    category: Optional[str] = None,
    description: Optional[str] = None,
    accepts: Optional[List[str]] = None,
    produces: Optional[List[str]] = None,
) -> Dict[str, Any]:
    """
    Register a Python node from a file path.

    This function loads a Python file and registers the node class(es) found
    within it. The nodes become available for use in pipeline manifests.

    Args:
        file_path: Path to the Python file containing the node class.
        node_type: Override the node type name. Defaults to class name.
        node_class: Specific class name to register (if file has multiple).
        multi_output: Whether the node produces multiple outputs per input.
        category: Category for grouping (e.g., "ml", "audio", "tts").
        description: Human-readable description.
        accepts: List of input data types (e.g., ["audio", "text"]).
        produces: List of output data types (e.g., ["audio", "json"]).

    Returns:
        Dictionary with registration info for each registered node.

    Example:
        # Register all nodes from a file
        register_python_node("./nodes/my_node.py")

        # Register with specific options
        register_python_node(
            "./nodes/ml_node.py",
            node_type="MyMLProcessor",
            multi_output=True,
            category="ml",
            accepts=["audio"],
            produces=["text"]
        )
    """
    path = Path(file_path).resolve()
    logger.info(f"Loading Python node from: {path}")
    
    # Load the module
    module = _load_module_from_path(str(path))
    
    # Find node classes
    classes = _find_node_classes(module)
    
    if not classes:
        raise ValueError(
            f"No MultiprocessNode subclasses found in {path}. "
            "Make sure your class inherits from remotemedia.MultiprocessNode"
        )
    
    # If node_class specified, filter to just that class
    if node_class:
        matching = [c for c in classes if c.__name__ == node_class]
        if not matching:
            available = [c.__name__ for c in classes]
            raise ValueError(
                f"Class '{node_class}' not found in {path}. "
                f"Available classes: {available}"
            )
        classes = matching
    
    # Register each class
    registered = {}
    
    for cls in classes:
        # Determine node type name
        type_name = node_type if node_type and len(classes) == 1 else cls.__name__
        
        # Build registration info
        info = {
            "node_type": type_name,
            "python_class": f"{module.__name__}.{cls.__name__}",
            "class": cls,
            "file_path": str(path),
            "multi_output": multi_output,
            "category": category,
            "description": description or cls.__doc__,
            "accepts": accepts or [],
            "produces": produces or [],
        }
        
        # Store in global registry
        _LOADED_NODES[type_name] = info
        registered[type_name] = info
        
        # Register with the multiprocess node registry (this is what Rust uses)
        try:
            from remotemedia.core.multiprocessing import _NODE_REGISTRY
            _NODE_REGISTRY[type_name] = cls
        except ImportError:
            pass
        
        # Also register with the decorator-based registry if available
        try:
            from .registration import streaming_node
            
            # Apply decorator retroactively
            streaming_node(
                node_type=type_name,
                multi_output=multi_output,
                category=category,
                description=description,
                accepts=accepts,
                produces=produces,
            )(cls)
        except ImportError:
            pass
        
        logger.info(
            f"Registered Python node: {type_name} "
            f"(class={cls.__name__}, multi_output={multi_output})"
        )
    
    return registered


def register_node_class(
    cls: Type,
    *,
    node_type: Optional[str] = None,
    multi_output: bool = False,
    category: Optional[str] = None,
    description: Optional[str] = None,
    accepts: Optional[List[str]] = None,
    produces: Optional[List[str]] = None,
) -> Dict[str, Any]:
    """
    Register an existing Python node class directly.

    This is useful when you have a class already defined and don't need
    to load it from a file.

    Args:
        cls: The node class to register (must inherit from MultiprocessNode).
        node_type: Override the node type name. Defaults to class name.
        multi_output: Whether the node produces multiple outputs per input.
        category: Category for grouping (e.g., "ml", "audio", "tts").
        description: Human-readable description.
        accepts: List of input data types (e.g., ["audio", "text"]).
        produces: List of output data types (e.g., ["audio", "json"]).

    Returns:
        Dictionary with registration info.

    Example:
        from remotemedia import register_node_class, MultiprocessNode

        class MyCustomNode(MultiprocessNode):
            async def process(self, data):
                return data

        register_node_class(MyCustomNode, multi_output=True)
    """
    from remotemedia.core import MultiprocessNode
    
    if not issubclass(cls, MultiprocessNode):
        raise TypeError(
            f"Class {cls.__name__} must inherit from MultiprocessNode"
        )
    
    type_name = node_type or cls.__name__
    
    info = {
        "node_type": type_name,
        "python_class": f"{cls.__module__}.{cls.__name__}",
        "class": cls,
        "file_path": None,
        "multi_output": multi_output,
        "category": category,
        "description": description or cls.__doc__,
        "accepts": accepts or [],
        "produces": produces or [],
    }
    
    # Store in global registry
    _LOADED_NODES[type_name] = info
    
    # Register with the multiprocess node registry (this is what Rust uses)
    try:
        from remotemedia.core.multiprocessing import _NODE_REGISTRY
        _NODE_REGISTRY[type_name] = cls
    except ImportError:
        pass
    
    # Also register with the decorator-based registry
    try:
        from .registration import streaming_node
        streaming_node(
            node_type=type_name,
            multi_output=multi_output,
            category=category,
            description=description,
            accepts=accepts,
            produces=produces,
        )(cls)
    except ImportError:
        pass
    
    logger.info(
        f"Registered node class: {type_name} "
        f"(class={cls.__name__}, multi_output={multi_output})"
    )
    
    return info


def get_loaded_nodes() -> Dict[str, Dict[str, Any]]:
    """Get all nodes that have been loaded via register_python_node()."""
    return _LOADED_NODES.copy()


def get_node_class(node_type: str) -> Optional[Type]:
    """Get the class for a registered node type."""
    info = _LOADED_NODES.get(node_type)
    return info["class"] if info else None


def clear_loaded_nodes():
    """Clear all loaded nodes (mainly for testing)."""
    _LOADED_NODES.clear()


def register_python_nodes_from_config(config_path: str) -> Dict[str, Any]:
    """
    Register multiple Python nodes from a YAML/JSON config file.

    Config format:
        python_nodes:
          - path: ./nodes/my_ml_node.py
          - path: ./nodes/custom_tts.py
            node_type: MyTTS
            multi_output: true
            category: tts

    Args:
        config_path: Path to YAML or JSON config file.

    Returns:
        Dictionary with all registration info.
    """
    import json
    from pathlib import Path
    
    config_file = Path(config_path)
    
    if not config_file.exists():
        raise FileNotFoundError(f"Config file not found: {config_file}")
    
    # Load config
    if config_file.suffix in (".yaml", ".yml"):
        import yaml
        with open(config_file) as f:
            config = yaml.safe_load(f)
    elif config_file.suffix == ".json":
        with open(config_file) as f:
            config = json.load(f)
    else:
        raise ValueError(f"Unsupported config format: {config_file.suffix}")
    
    # Get base directory for relative paths
    base_dir = config_file.parent
    
    # Register each node
    all_registered = {}
    
    for node_config in config.get("python_nodes", []):
        if isinstance(node_config, str):
            # Simple string path
            node_path = base_dir / node_config
            registered = register_python_node(str(node_path))
        else:
            # Dict with options
            node_path = base_dir / node_config.pop("path")
            registered = register_python_node(str(node_path), **node_config)
        
        all_registered.update(registered)
    
    return all_registered


__all__ = [
    "register_python_node",
    "register_node_class",
    "get_loaded_nodes",
    "get_node_class",
    "clear_loaded_nodes",
    "register_python_nodes_from_config",
]
