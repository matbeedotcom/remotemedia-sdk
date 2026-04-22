"""
End-to-end integration test: client builds a pipeline, server (Rust runtime
via FFI) executes it, client receives results.

Uses node types that are registered in the Rust streaming registry
(CalculatorNode) with the input schema the Rust node expects (operands[2]).
Skips if the Rust runtime is not compiled / installed.
"""

import pytest

from remotemedia import is_rust_runtime_available
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.calculator import CalculatorNode


pytestmark = pytest.mark.skipif(
    not is_rust_runtime_available(),
    reason="Rust runtime FFI not available",
)


@pytest.mark.asyncio
async def test_rust_runtime_executes_manifest_end_to_end():
    """
    Client-side: builds Pipeline with Python Node instance whose class name
    matches a Rust-registered streaming node (CalculatorNode).
    Serializes to manifest JSON, submits to Rust runtime over FFI,
    Rust runtime executes the pipeline, returns output.

    Note: the current FFI `execute_pipeline_with_input` is a unary call —
    it consumes only the first input. Batch processing is client-driven.
    """
    pipeline = Pipeline(
        nodes=[CalculatorNode(name="calc")],
        name="rust-e2e",
    )

    # Rust CalculatorNode contract: {"operation": "...", "operands": [a, b]}
    inputs = [
        {"operation": "add", "operands": [1, 2]},
        {"operation": "multiply", "operands": [3, 4]},
        {"operation": "subtract", "operands": [10, 3]},
    ]

    # Client-side batching: one FFI call per input, since the FFI is unary.
    results = []
    for item in inputs:
        result = await pipeline.run([item], use_rust=True)
        assert isinstance(result, dict), (
            f"expected dict per unary call, got {type(result).__name__}: {result!r}"
        )
        results.append(result["result"])

    assert sorted(results) == sorted([3.0, 12.0, 7.0])


@pytest.mark.asyncio
async def test_rust_runtime_inside_managed_execution():
    """
    managed_execution wraps the Rust FFI execution path:
    init + rust run + cleanup all succeed together.
    """
    pipeline = Pipeline(
        nodes=[CalculatorNode(name="calc")],
        name="rust-managed",
    )

    inputs = [{"operation": "add", "operands": [5, 7]}]

    async with pipeline.managed_execution() as pl:
        assert pl.is_initialized is True
        result = await pl.run(inputs, use_rust=True)

    assert pipeline.is_initialized is False
    # Single-input runs return a single dict (not a 1-element list).
    assert isinstance(result, dict), f"expected dict, got {type(result).__name__}: {result!r}"
    assert result["result"] == 12.0
