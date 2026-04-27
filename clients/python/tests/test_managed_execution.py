"""
Integration test for Pipeline.managed_execution().

Verifies that the async context manager:
  - initializes the pipeline on enter
  - yields the pipeline for streaming use
  - cleans up (is_initialized == False) on normal exit
  - cleans up on exception as well
"""

import asyncio
import pytest

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.base import PassThroughNode
from remotemedia.nodes.calculator import CalculatorNode


async def _feed(items):
    for item in items:
        yield item


@pytest.mark.asyncio
async def test_managed_execution_initializes_and_streams():
    pipeline = Pipeline(
        nodes=[
            PassThroughNode(name="pass"),
            CalculatorNode(name="calc"),
        ],
        name="managed-test",
    )

    assert pipeline.is_initialized is False

    inputs = [
        {"operation": "add", "args": [1, 2]},
        {"operation": "multiply", "args": [3, 4]},
        {"operation": "subtract", "args": [10, 3]},
    ]

    results = []
    async with pipeline.managed_execution() as pl:
        assert pl is pipeline
        assert pipeline.is_initialized is True

        async for out in pipeline.process(_feed(inputs)):
            results.append(out)

    # Cleanup must run on exit
    assert pipeline.is_initialized is False

    # Each calc result is a dict with 'result'
    assert len(results) == len(inputs)
    values = [r["result"] for r in results]
    assert sorted(values) == sorted([3, 12, 7])


@pytest.mark.asyncio
async def test_managed_execution_cleans_up_on_exception():
    pipeline = Pipeline(
        nodes=[PassThroughNode(name="pass")],
        name="managed-error-test",
    )

    with pytest.raises(RuntimeError, match="boom"):
        async with pipeline.managed_execution():
            assert pipeline.is_initialized is True
            raise RuntimeError("boom")

    assert pipeline.is_initialized is False


@pytest.mark.asyncio
async def test_managed_execution_empty_pipeline_raises():
    pipeline = Pipeline(name="empty")
    with pytest.raises(Exception):  # PipelineError wraps the empty check
        async with pipeline.managed_execution():
            pass
    assert pipeline.is_initialized is False
