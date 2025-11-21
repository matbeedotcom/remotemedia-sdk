#!/usr/bin/env python3
"""
Comprehensive tests for Feature 011: Python Instance Execution

Covers all user stories with integration tests.
T048-T053: User Story 3 test tasks
"""

import sys
import pytest
from pathlib import Path

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent.parent))


class TestNodeSerialization:
    """T048-T053: User Story 3 serialization tests."""

    def test_node_serialization_roundtrip(self):
        """T048: Test Node serialization → deserialization → verify state."""
        from remotemedia.core.node import Node
        from remotemedia.core.node_serialization import (
            serialize_node_for_ipc,
            deserialize_node_from_ipc
        )

        class StatefulNode(Node):
            def __init__(self, value=0, data="test", **kwargs):
                super().__init__(**kwargs)
                self.value = value
                self.data = data
                self.processed = []

            def process(self, input_data):
                self.processed.append(input_data)
                return f"{input_data}_{self.value}"

        # Create with state
        original = StatefulNode(name="state", value=42, data="important")
        original.processed = ["item1", "item2"]

        # Serialize
        serialized = serialize_node_for_ipc(original)
        assert isinstance(serialized, bytes)
        assert len(serialized) > 0

        # Deserialize
        restored = deserialize_node_from_ipc(serialized)

        # Verify all state preserved
        assert restored.name == "state"
        assert restored.value == 42
        assert restored.data == "important"
        assert restored.processed == ["item1", "item2"]

        # Verify functional
        result = restored.process("new")
        assert result == "new_42"
        assert "new" in restored.processed

    def test_cleanup_called_before_serialization(self):
        """T051: Test cleanup() is called before serialization."""
        from remotemedia.core.node import Node
        from remotemedia.core.node_serialization import serialize_node_for_ipc

        cleanup_called = []

        class TrackingNode(Node):
            def __init__(self, **kwargs):
                super().__init__(**kwargs)
                self.resource = "loaded"

            def cleanup(self):
                cleanup_called.append(True)
                self.resource = None
                super().cleanup()

            def process(self, data):
                return data

        node = TrackingNode(name="tracker")
        node.initialize()

        assert node._is_initialized == True
        assert len(cleanup_called) == 0

        # Serialize - should call cleanup
        serialize_node_for_ipc(node)

        assert len(cleanup_called) == 1, "cleanup() should have been called"
        assert node._is_initialized == False
        assert node.resource is None

    def test_initialize_called_after_deserialization(self):
        """T052: Test initialize() is called after deserialization."""
        from remotemedia.core.node import Node
        from remotemedia.core.node_serialization import (
            serialize_node_for_ipc,
            deserialize_node_from_ipc
        )

        class InitNode(Node):
            def __init__(self, **kwargs):
                super().__init__(**kwargs)
                self.init_count = 0  # Track in instance, not closure

            def initialize(self):
                self.init_count += 1
                super().initialize()

            def process(self, data):
                return data

        node = InitNode(name="init")
        assert node.init_count == 0

        serialized = serialize_node_for_ipc(node)

        # Deserialize - should call initialize
        restored = deserialize_node_from_ipc(serialized)

        # initialize() called during deserialize_node_from_ipc
        assert restored.init_count == 1, f"initialize() should have been called once, got {restored.init_count}"
        assert restored._is_initialized == True

    def test_serialization_error_non_serializable(self):
        """T050: Test serialization error for non-serializable attributes."""
        from remotemedia.core.node import Node
        from remotemedia.core.node_serialization import (
            serialize_node_for_ipc,
            SerializationError
        )
        import threading

        class NonSerializableNode(Node):
            def __init__(self, **kwargs):
                super().__init__(**kwargs)

            def process(self, data):
                return data

            def __getstate__(self):
                state = super().__getstate__()
                # Force non-serializable
                state['lock'] = threading.Lock()
                return state

        node = NonSerializableNode(name="bad")

        # Should raise SerializationError
        with pytest.raises(SerializationError) as exc_info:
            serialize_node_for_ipc(node)

        error = exc_info.value
        assert "bad" in str(error).lower()
        assert "pickle" in str(error).lower() or "lock" in str(error).lower()

    def test_serialization_error_message_quality(self):
        """T053: Test helpful error messages on serialization failure."""
        from remotemedia.core.node import Node
        from remotemedia.core.node_serialization import (
            serialize_node_for_ipc,
            SerializationError
        )
        import threading

        class ErrorNode(Node):
            def __init__(self, **kwargs):
                super().__init__(**kwargs)

            def process(self, data):
                return data

            def __getstate__(self):
                state = super().__getstate__()
                state['_lock'] = threading.Lock()
                return state

        node = ErrorNode(name="error_test")

        try:
            serialize_node_for_ipc(node)
            assert False, "Should have raised SerializationError"
        except SerializationError as e:
            error_msg = str(e)

            # Validate error message quality (SC-005)
            assert "error_test" in error_msg or "ErrorNode" in error_msg, "Should mention node name"
            assert "Suggestion:" in error_msg or "Implement" in error_msg, "Should have suggestion"
            assert len(error_msg) > 50, "Error message should be informative"

            # Check it has the attributes we need
            assert hasattr(e, 'node_name')
            assert hasattr(e, 'reason')
            assert hasattr(e, 'suggestion')

    def test_multiprocess_execution_with_instance(self):
        """T049: Test multiprocess execution with Node instance (framework)."""
        from remotemedia.core.node import Node
        from remotemedia.core.node_serialization import (
            serialize_node_for_ipc,
            deserialize_node_from_ipc
        )

        # This test validates the serialization mechanism
        # Actual multiprocess execution requires PipelineRunner integration
        class MPNode(Node):
            def __init__(self, config_value="default", **kwargs):
                super().__init__(**kwargs)
                self.config_value = config_value
                self.local_state = []

            def process(self, data):
                self.local_state.append(data)
                return f"{data}_{self.config_value}"

        # Simulate multiprocess workflow
        # Parent process
        node_parent = MPNode(name="mp", config_value="from_parent")
        node_parent.local_state = ["parent_data"]

        # Serialize for IPC
        serialized = serialize_node_for_ipc(node_parent)

        # Child process (simulated)
        node_child = deserialize_node_from_ipc(serialized)

        # Verify state transferred
        assert node_child.config_value == "from_parent"
        assert node_child.local_state == ["parent_data"]

        # Verify functional in "child"
        result = node_child.process("child_input")
        assert result == "child_input_from_parent"
        assert "child_input" in node_child.local_state


class TestMixedPipelines:
    """Additional tests for mixed pipelines (US2)."""

    def test_mixed_pipeline_with_state(self):
        """T036: Test mixed pipeline with stateful instances."""
        from remotemedia.core.node import Node
        from remotemedia.runtime_wrapper import _convert_mixed_list_to_manifest
        import json

        class CustomNode(Node):
            def __init__(self, multiplier=1, **kwargs):
                super().__init__(multiplier=multiplier, **kwargs)  # Pass to config
                self.multiplier = multiplier

            def process(self, data):
                return int(data) * self.multiplier

        # Create mixed pipeline
        mixed = [
            CustomNode(name="custom1", multiplier=2),
            {"node_type": "PassThrough", "params": {}},
            CustomNode(name="custom2", multiplier=3),
        ]

        # Convert to manifest
        manifest_json = _convert_mixed_list_to_manifest(mixed)
        manifest = json.loads(manifest_json)

        # Verify structure
        assert len(manifest['nodes']) == 3
        assert manifest['nodes'][0]['node_type'] == 'CustomNode'
        # multiplier should be in params now (passed via kwargs to config)
        assert 'params' in manifest['nodes'][0]
        assert manifest['nodes'][0]['params'].get('multiplier') == 2
        assert manifest['nodes'][2]['params'].get('multiplier') == 3

    def test_invalid_mixed_type_rejection(self):
        """T037: Test invalid mixed type raises TypeError."""
        from remotemedia import execute_pipeline
        from remotemedia.nodes import PassThroughNode

        # Create list with invalid types
        invalid = [
            PassThroughNode(name="valid"),
            "invalid_string",  # Invalid
            123,  # Invalid
        ]

        # Should raise TypeError
        with pytest.raises(TypeError) as exc_info:
            # Won't execute, but should fail validation in wrapper
            import asyncio
            try:
                asyncio.run(execute_pipeline(invalid))
            except TypeError:
                raise  # Re-raise for pytest.raises

        error = str(exc_info.value)
        assert "invalid" in error.lower() or "str" in error


class TestBackwardCompatibility:
    """Additional backward compatibility tests."""

    def test_existing_manifest_json_unchanged(self):
        """Verify manifest JSON format unchanged."""
        from remotemedia.core.pipeline import Pipeline
        from remotemedia.nodes import PassThroughNode
        import json

        # Create pipeline
        pipeline = Pipeline(name="test")
        pipeline.add_node(PassThroughNode(name="pass"))

        # Serialize
        manifest_json = pipeline.serialize()
        manifest = json.loads(manifest_json)

        # Verify expected structure (unchanged)
        assert manifest['version'] == 'v1'
        assert 'metadata' in manifest
        assert 'nodes' in manifest
        assert 'connections' in manifest
        assert isinstance(manifest['nodes'], list)
        assert isinstance(manifest['connections'], list)

    def test_node_pickle_support(self):
        """Test that Node base class __getstate__/__setstate__ work."""
        from remotemedia.core.node import Node
        import cloudpickle

        class SimpleNode(Node):
            def __init__(self, value=0, **kwargs):
                super().__init__(**kwargs)
                self.value = value

            def process(self, data):
                return data + self.value

        # Create and initialize
        node = SimpleNode(name="simple", value=10)
        node.initialize()

        assert node._is_initialized == True
        assert node.state is not None

        # Pickle
        pickled = cloudpickle.dumps(node)

        # Unpickle
        restored = cloudpickle.loads(pickled)

        # Verify state manager was recreated
        assert restored.state is not None
        assert restored.value == 10
        assert restored.name == "simple"
        # Note: _is_initialized is False after pickle (as intended)


if __name__ == "__main__":
    # Run with pytest
    sys.exit(pytest.main([__file__, "-v"]))
