"""
Test nodes for integration testing with gRPC execution.

These nodes are designed to test streaming behavior where process() yields N>=1 items.
"""

from typing import AsyncGenerator

def extract_data(data):
    """Extract actual data from RuntimeData wrapper if present."""
    # Check if this is a RuntimeData wrapper
    if hasattr(data, 'as_json') and hasattr(data, 'is_json'):
        if data.is_json():
            return data.as_json()
        elif hasattr(data, 'is_text') and hasattr(data, 'as_text'):
            if data.is_text():
                return data.as_text()
    # Otherwise return as-is
    return data


class MultiplyNode:
    """
    Simple non-streaming node that multiplies input by a factor.
    Yields exactly 1 item per process() call.
    """

    def __init__(self, factor: float = 2.0, node_id: str = None, **kwargs):
        self.factor = factor
        self.node_id = node_id

    async def process(self, data):
        """Process single input, yield single output."""
        if isinstance(data, dict) and "value" in data:
            result = {"value": data["value"] * self.factor}
        elif isinstance(data, (int, float)):
            result = data * self.factor
        else:
            result = data

        yield result


class ExpanderNode:
    """
    Streaming node that expands single input into multiple outputs.
    Yields N items per process() call, where N is configurable.
    """

    def __init__(self, expansion_factor: int = 3, node_id: str = None, **kwargs):
        self.expansion_factor = expansion_factor
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Expand single input into multiple outputs."""
        # Extract actual data from RuntimeData wrapper if present
        actual_data = extract_data(data)

        if isinstance(actual_data, dict) and "value" in actual_data:
            base_value = actual_data["value"]
            for i in range(self.expansion_factor):
                yield {"value": base_value + i, "index": i}
        elif isinstance(actual_data, (int, float)):
            for i in range(self.expansion_factor):
                yield actual_data + i
        else:
            # String or other type - duplicate it
            for i in range(self.expansion_factor):
                yield f"{actual_data}_{i}"


class FilterNode:
    """
    Streaming node that filters inputs based on a predicate.
    May yield 0 or 1 items per process() call.
    """

    def __init__(self, min_value: float = 0.0, node_id: str = None, **kwargs):
        self.min_value = min_value
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Filter out values below threshold."""
        # Extract actual data from RuntimeData wrapper if present
        actual_data = extract_data(data)

        value = actual_data.get("value", 0) if isinstance(actual_data, dict) else actual_data

        if isinstance(value, (int, float)) and value >= self.min_value:
            yield actual_data
        # Otherwise yield nothing (filter out)


class BatcherNode:
    """
    Stateful streaming node that batches inputs.
    Accumulates inputs and yields batches of configurable size.
    """

    def __init__(self, batch_size: int = 3, node_id: str = None, **kwargs):
        self.batch_size = batch_size
        self.buffer = []
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Accumulate inputs and yield batches."""
        self.buffer.append(data)

        # Yield batch when full
        if len(self.buffer) >= self.batch_size:
            yield {"batch": self.buffer[:self.batch_size], "size": self.batch_size}
            self.buffer = self.buffer[self.batch_size:]
        # Otherwise don't yield yet


class RangeGeneratorNode:
    """
    Generator node that produces a range of values.
    Yields multiple items based on input specification.
    """

    def __init__(self, node_id: str = None, **kwargs):
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Generate range of values based on input."""
        # Extract actual data from RuntimeData wrapper if present
        actual_data = extract_data(data)

        if isinstance(actual_data, dict):
            start = actual_data.get("start", 0)
            end = actual_data.get("end", 10)
            step = actual_data.get("step", 1)
        else:
            # Treat input as end value
            start, end, step = 0, int(actual_data), 1

        for value in range(start, end, step):
            yield {"value": value}


class TransformAndExpandNode:
    """
    Complex streaming node that transforms and expands.
    Each input yields multiple transformed outputs.
    """

    def __init__(self, transforms: list = None, node_id: str = None, **kwargs):
        self.transforms = transforms or ["upper", "lower", "reverse"]
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Apply multiple transformations to input."""
        # Extract actual data from RuntimeData wrapper if present
        actual_data = extract_data(data)

        text = str(actual_data)

        for transform in self.transforms:
            if transform == "upper":
                yield {"text": text.upper(), "transform": "upper"}
            elif transform == "lower":
                yield {"text": text.lower(), "transform": "lower"}
            elif transform == "reverse":
                yield {"text": text[::-1], "transform": "reverse"}
            elif transform == "double":
                yield {"text": text * 2, "transform": "double"}


class CounterNode:
    """
    Stateful node that counts process() calls.
    Yields the current count with each call.
    """

    def __init__(self, initial_count: int = 0, node_id: str = None, **kwargs):
        self.count = initial_count
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Increment counter and yield current count."""
        self.count += 1
        yield {
            "count": self.count,
            "input": data,
        }


class ConditionalExpanderNode:
    """
    Node that conditionally expands based on input value.
    Different inputs yield different numbers of outputs.
    """

    def __init__(self, node_id: str = None, **kwargs):
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Expand based on input value."""
        # Extract actual data from RuntimeData wrapper if present
        actual_data = extract_data(data)

        if isinstance(actual_data, dict):
            value = actual_data.get("value", 0)
        else:
            value = actual_data if isinstance(actual_data, (int, float)) else 0

        # Yield N items based on value
        expansion_count = int(abs(value)) if abs(value) <= 10 else 10

        for i in range(expansion_count):
            yield {"original": value, "index": i, "count": expansion_count}


class ChainedTransformNode:
    """
    Node that applies a chain of transformations.
    Each input goes through multiple stages, yielding intermediate results.
    """

    def __init__(self, emit_intermediates: bool = True, node_id: str = None, **kwargs):
        self.emit_intermediates = emit_intermediates
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Apply transformation chain."""
        # Extract actual data from RuntimeData wrapper if present
        actual_data = extract_data(data)

        if isinstance(actual_data, dict) and "value" in actual_data:
            value = actual_data["value"]
        elif isinstance(actual_data, (int, float)):
            value = actual_data
        else:
            value = 0

        # Stage 1: Double it
        stage1 = value * 2
        if self.emit_intermediates:
            yield {"stage": 1, "value": stage1}

        # Stage 2: Add 10
        stage2 = stage1 + 10
        if self.emit_intermediates:
            yield {"stage": 2, "value": stage2}

        # Stage 3: Square it
        stage3 = stage2 ** 2
        yield {"stage": 3, "value": stage3, "final": True}


class ErrorProneNode:
    """
    Node for testing error handling.
    May raise exceptions based on input.
    """

    def __init__(self, error_on_value: float = -1.0, node_id: str = None, **kwargs):
        self.error_on_value = error_on_value
        self.node_id = node_id

    async def process(self, data) -> AsyncGenerator:
        """Process data, raising error on specific value."""
        value = data.get("value", 0) if isinstance(data, dict) else data

        if value == self.error_on_value:
            raise ValueError(f"Error triggered by value: {value}")

        yield {"value": value, "status": "ok"}
