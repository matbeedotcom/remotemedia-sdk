"""Pipeline registry service."""

import logging
from pathlib import Path
from typing import Any, Optional

import yaml

logger = logging.getLogger(__name__)


class PipelineRegistry:
    """Registry for available pipelines."""

    def __init__(self) -> None:
        self._pipelines: dict[str, dict[str, Any]] = {}

    def load_from_directory(self, directory: str) -> None:
        """Load all pipeline manifests from a directory."""
        path = Path(directory)
        if not path.exists():
            logger.warning(f"Pipeline directory not found: {directory}")
            return

        for manifest_file in path.glob("*.yaml"):
            try:
                self.load_from_file(manifest_file)
            except Exception as e:
                logger.error(f"Failed to load {manifest_file}: {e}")

    def load_from_file(self, path: Path) -> None:
        """Load a pipeline manifest from a file."""
        with open(path) as f:
            manifest = yaml.safe_load(f)

        name = manifest.get("name", path.stem)
        self._pipelines[name] = manifest
        logger.info(f"Loaded pipeline: {name}")

    def register(self, name: str, manifest: dict[str, Any]) -> None:
        """Register a pipeline manifest."""
        self._pipelines[name] = manifest

    def get_pipeline(self, name: str) -> Optional[dict[str, Any]]:
        """Get a pipeline by name."""
        return self._pipelines.get(name)

    def list_pipelines(self) -> list[dict[str, Any]]:
        """List all registered pipelines."""
        return [
            {
                "name": name,
                "description": manifest.get("description", ""),
                "version": manifest.get("version", "1.0"),
                "input_type": self._detect_input_type(manifest),
                "output_type": self._detect_output_type(manifest),
                "streaming": self._is_streaming(manifest),
            }
            for name, manifest in self._pipelines.items()
        ]

    def _detect_input_type(self, manifest: dict[str, Any]) -> str:
        """Detect the input type from the first node."""
        nodes = manifest.get("nodes", [])
        if not nodes:
            return "unknown"

        first_node = nodes[0]
        node_type = first_node.get("node_type", "")

        if "Audio" in node_type or "Whisper" in node_type:
            return "audio"
        if "Text" in node_type:
            return "text"
        if "Image" in node_type or "Video" in node_type:
            return "image"

        return "unknown"

    def _detect_output_type(self, manifest: dict[str, Any]) -> str:
        """Detect the output type from the last node."""
        nodes = manifest.get("nodes", [])
        if not nodes:
            return "unknown"

        last_node = nodes[-1]
        node_type = last_node.get("node_type", "")

        if "Audio" in node_type or "TTS" in node_type:
            return "audio"
        if "Text" in node_type or "STT" in node_type or "Whisper" in node_type:
            return "text"
        if "Image" in node_type:
            return "image"

        return "unknown"

    def _is_streaming(self, manifest: dict[str, Any]) -> bool:
        """Check if the pipeline supports streaming."""
        nodes = manifest.get("nodes", [])
        # A pipeline is streaming if any node is marked as streaming
        return any(node.get("streaming", True) for node in nodes)
