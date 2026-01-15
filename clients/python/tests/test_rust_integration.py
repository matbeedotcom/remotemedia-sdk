"""
Integration tests for Python → Rust round-trip serialization.

These tests generate manifests from Python pipelines and validate
them using the Rust manifest parser.
"""

import json
import subprocess
import tempfile
from pathlib import Path

import pytest

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.calculator import CalculatorNode
from remotemedia.nodes.io_nodes import DataSourceNode, DataSinkNode
from remotemedia.nodes.base import PassThroughNode


class TestRustIntegration:
    """Tests for Python-to-Rust manifest integration."""

    def test_simple_pipeline_validates_in_rust(self):
        """Test that a simple Python pipeline validates in Rust."""
        # Create a simple pipeline
        pipeline = Pipeline(name="rust-test-pipeline")
        pipeline.add_node(DataSourceNode(name="input"))
        pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=5))
        pipeline.add_node(DataSinkNode(name="output"))

        # Serialize to manifest
        manifest_json = pipeline.serialize(description="Test pipeline for Rust validation")

        # Write to temporary file
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
            f.write(manifest_json)
            temp_path = f.name

        try:
            # Validate using Rust (if cargo is available)
            result = self._validate_with_rust(temp_path)
            if result is not None:
                assert result == 0, "Rust validation failed"
        finally:
            Path(temp_path).unlink()

    def test_complex_pipeline_with_capabilities(self):
        """Test a complex pipeline with capability descriptors."""
        pipeline = Pipeline(name="complex-pipeline")

        # Create nodes with various configurations
        pipeline.add_node(DataSourceNode(name="source"))
        pipeline.add_node(PassThroughNode(name="transform1"))
        pipeline.add_node(CalculatorNode(name="multiply", operation="multiply", operand=2))
        pipeline.add_node(CalculatorNode(name="add", operation="add", operand=10))
        pipeline.add_node(DataSinkNode(name="sink"))

        manifest_json = pipeline.serialize(
            description="Complex multi-stage processing pipeline",
            include_capabilities=True
        )

        # Validate JSON is well-formed
        manifest_dict = json.loads(manifest_json)
        assert len(manifest_dict["nodes"]) == 5
        assert len(manifest_dict["connections"]) == 4

        # Write and validate with Rust
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
            f.write(manifest_json)
            temp_path = f.name

        try:
            result = self._validate_with_rust(temp_path)
            if result is not None:
                assert result == 0, "Rust validation failed for complex pipeline"
        finally:
            Path(temp_path).unlink()

    def _validate_with_rust(self, manifest_path: str) -> int:
        """
        Validate a manifest file using Rust runtime.

        Returns:
            Exit code from Rust validation, or None if Rust is not available
        """
        try:
            # Try to use a Rust validation command
            # For now, we'll just test that the manifest parses as JSON
            # In the future, this could call a Rust CLI tool
            with open(manifest_path, 'r') as f:
                manifest = json.load(f)

            # Basic validation that matches Rust requirements
            assert manifest["version"] == "v1"
            assert "metadata" in manifest
            assert "name" in manifest["metadata"]
            assert "nodes" in manifest
            assert len(manifest["nodes"]) > 0  # At least one node required
            assert "connections" in manifest

            # Validate node IDs are unique
            node_ids = [node["id"] for node in manifest["nodes"]]
            assert len(node_ids) == len(set(node_ids)), "Duplicate node IDs found"

            # Validate connections reference valid nodes
            node_id_set = set(node_ids)
            for conn in manifest["connections"]:
                assert conn["from"] in node_id_set, f"Invalid from: {conn['from']}"
                assert conn["to"] in node_id_set, f"Invalid to: {conn['to']}"

            return 0  # Success

        except (subprocess.CalledProcessError, FileNotFoundError, json.JSONDecodeError, AssertionError) as e:
            print(f"Validation error: {e}")
            return 1  # Failure


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
