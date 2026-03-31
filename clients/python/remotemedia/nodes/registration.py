"""
Node registration system for Python nodes.

This module provides a decorator-based registration system that allows Python nodes
to register themselves with metadata. The registered information can be:
1. Used by the Rust runtime to create factories dynamically
2. Exported to a manifest file for static registration
3. Used for documentation generation

Usage:
    from remotemedia.nodes.registration import streaming_node

    @streaming_node(
        node_type="KokoroTTSNode",
        multi_output=True,
        accepts=["text"],
        produces=["audio"],
        description="Text-to-speech using Kokoro TTS"
    )
    class KokoroTTSNode(MultiprocessNode):
        async def process(self, data):
            # ... implementation
"""

from typing import Dict, List, Optional, Type, Any, Callable
from dataclasses import dataclass, field
import json
import logging

logger = logging.getLogger(__name__)

# Global registry of Python streaming nodes
_REGISTERED_NODES: Dict[str, "NodeRegistration"] = {}


@dataclass
class NodeRegistration:
    """Registration information for a Python streaming node."""
    
    # The node type name used in manifests (e.g., "KokoroTTSNode")
    node_type: str
    
    # The Python class that implements the node
    python_class: Type
    
    # Full Python import path (e.g., "remotemedia.nodes.tts.KokoroTTSNode")
    python_path: str
    
    # Whether this node can produce multiple outputs per input
    multi_output: bool = False
    
    # Description of the node
    description: Optional[str] = None
    
    # Category for grouping (e.g., "ml", "audio", "tts")
    category: Optional[str] = None
    
    # Input data types this node accepts
    accepts: List[str] = field(default_factory=list)
    
    # Output data types this node produces
    produces: List[str] = field(default_factory=list)
    
    # Additional metadata
    metadata: Dict[str, Any] = field(default_factory=dict)
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "node_type": self.node_type,
            "python_class": self.python_path,
            "multi_output": self.multi_output,
            "description": self.description,
            "category": self.category,
            "accepts": self.accepts,
            "produces": self.produces,
            "metadata": self.metadata,
        }
    
    def to_rust_config(self) -> str:
        """Generate Rust registration code."""
        lines = [f'register_python_node(']
        lines.append(f'    PythonNodeConfig::new("{self.node_type}")')
        lines.append(f'        .with_python_class("{self.python_path}")')
        
        if self.multi_output:
            lines.append('        .with_multi_output(true)')
        
        if self.description:
            lines.append(f'        .with_description("{self.description}")')
        
        if self.category:
            lines.append(f'        .with_category("{self.category}")')
        
        if self.accepts:
            accepts_str = ', '.join(f'"{a}"' for a in self.accepts)
            lines.append(f'        .accepts([{accepts_str}])')
        
        if self.produces:
            produces_str = ', '.join(f'"{p}"' for p in self.produces)
            lines.append(f'        .produces([{produces_str}])')
        
        lines.append(');')
        return '\n'.join(lines)


def streaming_node(
    node_type: Optional[str] = None,
    *,
    multi_output: bool = False,
    description: Optional[str] = None,
    category: Optional[str] = None,
    accepts: Optional[List[str]] = None,
    produces: Optional[List[str]] = None,
    **metadata: Any,
) -> Callable[[Type], Type]:
    """
    Decorator to register a Python class as a streaming node.
    
    Args:
        node_type: The node type name used in manifests. Defaults to class name.
        multi_output: Whether this node can produce multiple outputs per input.
        description: Human-readable description of the node.
        category: Category for grouping (e.g., "ml", "audio", "tts").
        accepts: List of input data types (e.g., ["audio", "text"]).
        produces: List of output data types (e.g., ["audio", "json"]).
        **metadata: Additional metadata to store with the registration.
    
    Returns:
        The decorated class, unchanged.
    
    Example:
        @streaming_node(
            node_type="KokoroTTSNode",
            multi_output=True,
            category="tts",
            accepts=["text"],
            produces=["audio"],
            description="Text-to-speech using Kokoro TTS"
        )
        class KokoroTTSNode(MultiprocessNode):
            ...
    """
    def decorator(cls: Type) -> Type:
        # Determine node type name
        name = node_type or cls.__name__
        
        # Build full Python import path
        module = cls.__module__
        python_path = f"{module}.{cls.__name__}"
        
        # Create registration
        registration = NodeRegistration(
            node_type=name,
            python_class=cls,
            python_path=python_path,
            multi_output=multi_output,
            description=description or cls.__doc__,
            category=category,
            accepts=accepts or [],
            produces=produces or [],
            metadata=metadata,
        )
        
        # Store in global registry
        _REGISTERED_NODES[name] = registration
        
        # Attach registration to class for introspection
        cls._node_registration = registration
        
        logger.debug(f"Registered streaming node: {name} ({python_path})")
        
        return cls
    
    return decorator


def get_registered_nodes() -> Dict[str, NodeRegistration]:
    """Get all registered Python streaming nodes."""
    return _REGISTERED_NODES.copy()


def get_node_registration(node_type: str) -> Optional[NodeRegistration]:
    """Get registration for a specific node type."""
    return _REGISTERED_NODES.get(node_type)


def clear_registry():
    """Clear all registered nodes (mainly for testing)."""
    _REGISTERED_NODES.clear()


def export_to_json(path: Optional[str] = None) -> str:
    """
    Export all registered nodes to JSON format.
    
    Args:
        path: Optional file path to write JSON to.
    
    Returns:
        JSON string of all registered nodes.
    """
    data = {
        "nodes": [reg.to_dict() for reg in _REGISTERED_NODES.values()]
    }
    json_str = json.dumps(data, indent=2)
    
    if path:
        with open(path, 'w') as f:
            f.write(json_str)
    
    return json_str


def export_to_rust(path: Optional[str] = None) -> str:
    """
    Export all registered nodes as Rust registration code.
    
    Args:
        path: Optional file path to write Rust code to.
    
    Returns:
        Rust code string for registering all nodes.
    """
    lines = [
        "// Auto-generated Python node registrations",
        "// Do not edit manually - regenerate with: python -m remotemedia.nodes.registration",
        "",
        "use crate::registry::{register_python_node, PythonNodeConfig};",
        "",
        "pub fn register_discovered_python_nodes() {",
    ]
    
    for reg in _REGISTERED_NODES.values():
        lines.append("")
        lines.append("    " + reg.to_rust_config().replace("\n", "\n    "))
    
    lines.append("}")
    
    rust_code = "\n".join(lines)
    
    if path:
        with open(path, 'w') as f:
            f.write(rust_code)
    
    return rust_code


def discover_and_register():
    """
    Import all node modules to trigger registration.
    
    This function imports all known node modules, which causes
    their @streaming_node decorators to run and register the nodes.
    """
    # Import modules that contain streaming nodes
    from . import tts  # noqa: F401
    from . import tts_vibevoice  # noqa: F401
    from . import transcription  # noqa: F401
    from . import test_nodes  # noqa: F401
    from . import simple_pytorch_test  # noqa: F401
    
    # Try optional modules
    try:
        from .ml import lfm2_audio  # noqa: F401
    except ImportError:
        pass
    
    logger.info(f"Discovered {len(_REGISTERED_NODES)} Python streaming nodes")


# Export for convenience
__all__ = [
    "streaming_node",
    "NodeRegistration",
    "get_registered_nodes",
    "get_node_registration",
    "clear_registry",
    "export_to_json",
    "export_to_rust",
    "discover_and_register",
]


if __name__ == "__main__":
    """CLI for exporting node registrations."""
    import argparse
    
    parser = argparse.ArgumentParser(description="Export Python node registrations")
    parser.add_argument("--json", "-j", help="Export to JSON file")
    parser.add_argument("--rust", "-r", help="Export to Rust file")
    parser.add_argument("--list", "-l", action="store_true", help="List registered nodes")
    
    args = parser.parse_args()
    
    # Discover all nodes
    discover_and_register()
    
    if args.list:
        print(f"Registered Python streaming nodes ({len(_REGISTERED_NODES)}):")
        for name, reg in sorted(_REGISTERED_NODES.items()):
            print(f"  - {name}: {reg.python_path}")
            if reg.description:
                print(f"    {reg.description}")
    
    if args.json:
        export_to_json(args.json)
        print(f"Exported JSON to: {args.json}")
    
    if args.rust:
        export_to_rust(args.rust)
        print(f"Exported Rust code to: {args.rust}")
    
    if not any([args.list, args.json, args.rust]):
        # Default: print JSON to stdout
        print(export_to_json())
