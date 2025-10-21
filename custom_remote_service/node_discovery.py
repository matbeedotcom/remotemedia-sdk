"""
Node Discovery Module for RemoteMedia Service.

This module provides automatic discovery and loading of custom nodes
from configured paths, enabling seamless integration of example nodes
and third-party extensions.
"""

import logging
import os
import sys
import importlib.util
import inspect
from pathlib import Path
from typing import Dict, List, Optional, Type, Set
from remotemedia.core.node import Node

logger = logging.getLogger(__name__)


class NodeDiscovery:
    """Discovers and loads custom nodes from configured paths."""

    def __init__(self, search_paths: Optional[List[str]] = None):
        """
        Initialize node discovery.

        Args:
            search_paths: List of directories to search for custom nodes.
                         If None, uses default paths relative to the project root.
        """
        self.search_paths = search_paths or self._get_default_search_paths()
        self.discovered_nodes: Dict[str, Type[Node]] = {}
        self._loaded_modules: Set[str] = set()

    def _get_default_search_paths(self) -> List[str]:
        """Get default search paths for custom nodes."""
        # Get project root (assuming service is in service/src/)
        service_dir = Path(__file__).parent.parent.parent

        paths = [
            str(service_dir / "examples" / "audio_examples"),
            str(service_dir / "custom_remote_service" / "nodes"),
            str(service_dir / "webrtc-example" / "webrtc_examples"),
        ]

        # Filter to existing paths
        return [p for p in paths if os.path.exists(p)]

    def discover_nodes(self) -> Dict[str, Type[Node]]:
        """
        Discover all custom nodes from search paths.

        Returns:
            Dictionary mapping node class names to node classes
        """
        self.discovered_nodes = {}

        for search_path in self.search_paths:
            logger.info(f"Searching for custom nodes in: {search_path}")
            self._discover_from_path(search_path)

        logger.info(f"Discovered {len(self.discovered_nodes)} custom nodes")
        return self.discovered_nodes

    def _discover_from_path(self, path: str) -> None:
        """
        Discover nodes from a specific path.

        Args:
            path: Directory path to search
        """
        if not os.path.exists(path):
            logger.warning(f"Search path does not exist: {path}")
            return

        # Add path to sys.path if not already there
        if path not in sys.path:
            sys.path.insert(0, path)

        # Find all Python files
        for file_path in Path(path).rglob("*.py"):
            if file_path.name.startswith("_"):
                continue

            try:
                self._load_nodes_from_file(file_path)
            except Exception as e:
                logger.warning(f"Failed to load nodes from {file_path}: {e}")

    def _load_nodes_from_file(self, file_path: Path) -> None:
        """
        Load nodes from a Python file.

        Args:
            file_path: Path to Python file
        """
        module_name = file_path.stem

        # Skip if already loaded
        full_module_name = f"custom_nodes.{module_name}"
        if full_module_name in self._loaded_modules:
            return

        try:
            # Load the module
            spec = importlib.util.spec_from_file_location(full_module_name, file_path)
            if spec is None or spec.loader is None:
                return

            module = importlib.util.module_from_spec(spec)
            sys.modules[full_module_name] = module
            spec.loader.exec_module(module)

            self._loaded_modules.add(full_module_name)

            # Find Node subclasses
            for name, obj in inspect.getmembers(module, inspect.isclass):
                if (obj is not Node and
                    issubclass(obj, Node) and
                    obj.__module__ == full_module_name):

                    # Use the class name as the key
                    node_name = obj.__name__
                    self.discovered_nodes[node_name] = obj
                    logger.info(f"Discovered custom node: {node_name} from {file_path.name}")

        except Exception as e:
            logger.debug(f"Could not load module from {file_path}: {e}")

    def register_nodes(
        self,
        node_registry: Dict[str, Type[Node]],
        prefix: str = ""
    ) -> Dict[str, Type[Node]]:
        """
        Register discovered nodes into a node registry.

        Args:
            node_registry: Existing node registry to add to
            prefix: Optional prefix for node names

        Returns:
            Updated node registry
        """
        for node_name, node_class in self.discovered_nodes.items():
            registered_name = f"{prefix}{node_name}" if prefix else node_name

            if registered_name in node_registry:
                logger.warning(
                    f"Node '{registered_name}' already exists in registry, skipping"
                )
                continue

            node_registry[registered_name] = node_class
            logger.debug(f"Registered custom node: {registered_name}")

        return node_registry

    def add_search_path(self, path: str) -> None:
        """
        Add a new search path for node discovery.

        Args:
            path: Directory path to add
        """
        if os.path.exists(path) and path not in self.search_paths:
            self.search_paths.append(path)
            logger.info(f"Added search path: {path}")

    def get_node_info(self, node_name: str) -> Optional[Dict[str, any]]:
        """
        Get information about a discovered node.

        Args:
            node_name: Name of the node class

        Returns:
            Dictionary with node information or None if not found
        """
        node_class = self.discovered_nodes.get(node_name)
        if not node_class:
            return None

        return {
            "name": node_name,
            "module": node_class.__module__,
            "doc": inspect.getdoc(node_class),
            "file": inspect.getfile(node_class),
            "is_streaming": getattr(node_class, "is_streaming", False),
        }
