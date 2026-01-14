"""
Tests for Pipeline.serialize() and Node.to_manifest() functionality.

These tests verify that pipelines can be serialized to JSON manifests
conforming to the manifest.v1.json schema for Rust runtime execution.
"""

import json
import pytest
from datetime import datetime

from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import Node
from remotemedia.nodes.calculator import CalculatorNode
from remotemedia.nodes.io_nodes import DataSourceNode, DataSinkNode
from remotemedia.nodes.base import PassThroughNode


class TestNodeManifest:
    """Tests for Node.to_manifest() method."""

    def test_node_to_manifest_basic(self):
        """Test basic node manifest generation."""
        node = PassThroughNode(name="test_node")
        manifest = node.to_manifest()

        assert manifest["id"] == "test_node"
        assert manifest["node_type"] == "PassThroughNode"
        assert "params" in manifest
        assert isinstance(manifest["params"], dict)

    def test_node_to_manifest_with_config(self):
        """Test node manifest includes configuration."""
        node = CalculatorNode(name="calc", operation="add", operand=5)
        manifest = node.to_manifest()

        assert manifest["id"] == "calc"
        assert manifest["node_type"] == "CalculatorNode"
        assert manifest["params"]["operation"] == "add"
        assert manifest["params"]["operand"] == 5

    def test_node_manifest_without_capabilities(self):
        """Test node manifest without capabilities flag."""
        node = PassThroughNode(name="test")
        manifest = node.to_manifest(include_capabilities=False)

        assert "capabilities" not in manifest

    def test_node_get_capabilities_default(self):
        """Test default get_capabilities returns None."""
        node = PassThroughNode(name="test")
        capabilities = node.get_capabilities()

        assert capabilities is None


class TestPipelineSerialize:
    """Tests for Pipeline.serialize() method."""

    def test_serialize_empty_pipeline_fails(self):
        """Test that serializing an empty pipeline fails (needs at least 1 node)."""
        pipeline = Pipeline(name="empty")

        # Should not raise during serialization
        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        # But manifest will have empty nodes list
        assert len(manifest_dict["nodes"]) == 0

    def test_serialize_simple_pipeline(self):
        """Test serializing a simple linear pipeline."""
        pipeline = Pipeline(name="simple-pipeline")
        pipeline.add_node(DataSourceNode(name="input"))
        pipeline.add_node(PassThroughNode(name="passthrough"))
        pipeline.add_node(DataSinkNode(name="output"))

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        # Check version
        assert manifest_dict["version"] == "v1"

        # Check metadata
        assert manifest_dict["metadata"]["name"] == "simple-pipeline"
        assert "created_at" in manifest_dict["metadata"]

        # Check nodes
        assert len(manifest_dict["nodes"]) == 3
        assert manifest_dict["nodes"][0]["id"] == "input_0"
        assert manifest_dict["nodes"][0]["node_type"] == "DataSourceNode"
        assert manifest_dict["nodes"][1]["id"] == "passthrough_1"
        assert manifest_dict["nodes"][2]["id"] == "output_2"

        # Check connections (linear: 0→1, 1→2)
        assert len(manifest_dict["connections"]) == 2
        assert manifest_dict["connections"][0] == {"from": "input_0", "to": "passthrough_1"}
        assert manifest_dict["connections"][1] == {"from": "passthrough_1", "to": "output_2"}

    def test_serialize_with_description(self):
        """Test serializing pipeline with description."""
        pipeline = Pipeline(name="test-pipeline")
        pipeline.add_node(PassThroughNode(name="node1"))

        manifest_json = pipeline.serialize(description="Test pipeline description")
        manifest_dict = json.loads(manifest_json)

        assert manifest_dict["metadata"]["description"] == "Test pipeline description"

    def test_serialize_without_capabilities(self):
        """Test serializing pipeline without capability descriptors."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(PassThroughNode(name="node1"))

        manifest_json = pipeline.serialize(include_capabilities=False)
        manifest_dict = json.loads(manifest_json)

        # Nodes should not have capabilities
        for node in manifest_dict["nodes"]:
            assert "capabilities" not in node

    def test_serialize_timestamp_format(self):
        """Test that created_at timestamp is in ISO 8601 format."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(PassThroughNode(name="node1"))

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        created_at = manifest_dict["metadata"]["created_at"]
        # Should be ISO 8601 format with Z suffix
        assert created_at.endswith("Z")
        # Should be parseable
        datetime.fromisoformat(created_at.replace("Z", "+00:00"))

    def test_serialize_node_id_uniqueness(self):
        """Test that node IDs are unique even with duplicate names."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(PassThroughNode(name="node"))
        pipeline.add_node(PassThroughNode(name="node"))  # Duplicate name
        pipeline.add_node(PassThroughNode(name="node"))  # Duplicate name

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        # IDs should be unique with index suffix
        node_ids = [node["id"] for node in manifest_dict["nodes"]]
        assert node_ids == ["node_0", "node_1", "node_2"]
        assert len(set(node_ids)) == 3  # All unique

    def test_serialize_connection_order(self):
        """Test that connections maintain correct order."""
        pipeline = Pipeline(name="test")
        nodes = [
            DataSourceNode(name="source"),
            PassThroughNode(name="transform1"),
            PassThroughNode(name="transform2"),
            PassThroughNode(name="transform3"),
            DataSinkNode(name="sink")
        ]
        for node in nodes:
            pipeline.add_node(node)

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        # Should have N-1 connections for N nodes
        assert len(manifest_dict["connections"]) == 4

        # Verify sequential connections
        expected_connections = [
            {"from": "source_0", "to": "transform1_1"},
            {"from": "transform1_1", "to": "transform2_2"},
            {"from": "transform2_2", "to": "transform3_3"},
            {"from": "transform3_3", "to": "sink_4"}
        ]
        assert manifest_dict["connections"] == expected_connections


class TestManifestSchema:
    """Tests for manifest schema compliance."""

    def test_manifest_has_required_fields(self):
        """Test that manifest contains all required top-level fields."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(PassThroughNode(name="node1"))

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        # Required fields from schema
        assert "version" in manifest_dict
        assert "metadata" in manifest_dict
        assert "nodes" in manifest_dict
        assert "connections" in manifest_dict

    def test_node_manifest_has_required_fields(self):
        """Test that node manifests contain required fields."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(CalculatorNode(name="calc", operation="add", operand=1))

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        node_manifest = manifest_dict["nodes"][0]

        # Required fields from schema
        assert "id" in node_manifest
        assert "node_type" in node_manifest
        # params is optional but should be present with default {}
        assert "params" in node_manifest

    def test_connection_manifest_format(self):
        """Test connection manifest format."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(PassThroughNode(name="node1"))
        pipeline.add_node(PassThroughNode(name="node2"))

        manifest_json = pipeline.serialize()
        manifest_dict = json.loads(manifest_json)

        connection = manifest_dict["connections"][0]

        # Required fields
        assert "from" in connection
        assert "to" in connection
        # Should NOT have port information (simplified from export_definition)
        assert "output_port" not in connection
        assert "input_port" not in connection

    def test_manifest_json_serializable(self):
        """Test that manifest can be serialized to valid JSON."""
        pipeline = Pipeline(name="test")
        pipeline.add_node(CalculatorNode(name="calc", operation="multiply", operand=2))
        pipeline.add_node(DataSinkNode(name="output"))

        manifest_json = pipeline.serialize()

        # Should be valid JSON
        manifest_dict = json.loads(manifest_json)
        assert isinstance(manifest_dict, dict)

        # Should be re-serializable
        re_serialized = json.dumps(manifest_dict)
        assert isinstance(re_serialized, str)


class TestCapabilityDescriptors:
    """Tests for node capability descriptors."""

    def test_node_with_capabilities(self):
        """Test node that returns capability requirements."""
        # Create a custom node with capabilities
        class GPUNode(Node):
            def process(self, data):
                return data

            def get_capabilities(self):
                return {
                    "gpu": {
                        "type": "cuda",
                        "min_memory_gb": 4.0,
                        "required": True
                    },
                    "memory_gb": 8.0
                }

        node = GPUNode(name="gpu_node")
        manifest = node.to_manifest(include_capabilities=True)

        assert "capabilities" in manifest
        assert manifest["capabilities"]["gpu"]["type"] == "cuda"
        assert manifest["capabilities"]["gpu"]["min_memory_gb"] == 4.0
        assert manifest["capabilities"]["gpu"]["required"] is True
        assert manifest["capabilities"]["memory_gb"] == 8.0

    def test_pipeline_with_capable_nodes(self):
        """Test pipeline serialization with nodes that have capabilities."""
        class CPUNode(Node):
            def process(self, data):
                return data

            def get_capabilities(self):
                return {
                    "cpu": {"cores": 4},
                    "memory_gb": 2.0
                }

        pipeline = Pipeline(name="test")
        pipeline.add_node(CPUNode(name="cpu_node"))
        pipeline.add_node(PassThroughNode(name="simple_node"))

        manifest_json = pipeline.serialize(include_capabilities=True)
        manifest_dict = json.loads(manifest_json)

        # First node should have capabilities
        assert "capabilities" in manifest_dict["nodes"][0]
        assert manifest_dict["nodes"][0]["capabilities"]["cpu"]["cores"] == 4

        # Second node should not (returns None from get_capabilities)
        assert "capabilities" not in manifest_dict["nodes"][1]


class TestRemoteNodeSerialization:
    """Tests for remote node serialization."""

    def test_remote_node_host_field(self):
        """Test that remote nodes include host field."""
        from remotemedia.core.node import RemoteExecutorConfig
        from remotemedia.nodes.remote import RemoteExecutionNode

        remote_config = RemoteExecutorConfig(host="gpu-server", port=50051)
        remote_node = RemoteExecutionNode(
            name="remote_calc",
            node_to_execute="CalculatorNode",
            remote_config=remote_config,
            node_config={"operation": "add", "operand": 5}
        )

        manifest = remote_node.to_manifest()

        # Should have host field
        assert "host" in manifest
        assert manifest["host"] == "gpu-server:50051"


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
