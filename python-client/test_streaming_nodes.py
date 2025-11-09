#!/usr/bin/env python3
"""
Quick test script for streaming nodes.
Tests that nodes yield N>=1 items per process() call.
"""

import asyncio
from remotemedia.nodes.test_nodes import (
    ExpanderNode,
    RangeGeneratorNode,
    TransformAndExpandNode,
    ChainedTransformNode,
    ConditionalExpanderNode,
)


async def test_expander_node():
    """Test ExpanderNode yields multiple items."""
    print("\n=== Testing ExpanderNode ===")
    node = ExpanderNode(expansion_factor=3)

    results = []
    async for item in node.process({"value": 10}):
        results.append(item)

    print(f"Input: {{'value': 10}}")
    print(f"Outputs ({len(results)} items):")
    for i, result in enumerate(results):
        print(f"  {i}: {result}")

    assert len(results) == 3, f"Expected 3 items, got {len(results)}"
    print("[PASS] ExpanderNode yields 3 items")


async def test_range_generator():
    """Test RangeGeneratorNode yields multiple items."""
    print("\n=== Testing RangeGeneratorNode ===")
    node = RangeGeneratorNode()

    results = []
    async for item in node.process({"start": 0, "end": 5, "step": 1}):
        results.append(item)

    print(f"Input: {{'start': 0, 'end': 5, 'step': 1}}")
    print(f"Outputs ({len(results)} items):")
    for i, result in enumerate(results):
        print(f"  {i}: {result}")

    assert len(results) == 5, f"Expected 5 items, got {len(results)}"
    print("[PASS] RangeGeneratorNode yields 5 items")


async def test_transform_and_expand():
    """Test TransformAndExpandNode yields multiple transformations."""
    print("\n=== Testing TransformAndExpandNode ===")
    node = TransformAndExpandNode(transforms=["upper", "lower", "reverse"])

    results = []
    async for item in node.process("Hello"):
        results.append(item)

    print(f"Input: 'Hello'")
    print(f"Outputs ({len(results)} items):")
    for i, result in enumerate(results):
        print(f"  {i}: {result}")

    assert len(results) == 3, f"Expected 3 items, got {len(results)}"
    print("[PASS] TransformAndExpandNode yields 3 transformations")


async def test_chained_transform():
    """Test ChainedTransformNode yields intermediate results."""
    print("\n=== Testing ChainedTransformNode ===")
    node = ChainedTransformNode(emit_intermediates=True)

    results = []
    async for item in node.process({"value": 5}):
        results.append(item)

    print(f"Input: {{'value': 5}}")
    print(f"Outputs ({len(results)} items):")
    for i, result in enumerate(results):
        print(f"  {i}: {result}")

    assert len(results) == 3, f"Expected 3 items (3 stages), got {len(results)}"
    print("[PASS] ChainedTransformNode yields 3 stages")


async def test_conditional_expander():
    """Test ConditionalExpanderNode with variable expansion."""
    print("\n=== Testing ConditionalExpanderNode ===")
    node = ConditionalExpanderNode()

    # Test with value=5 (should yield 5 items)
    results = []
    async for item in node.process({"value": 5}):
        results.append(item)

    print(f"Input: {{'value': 5}}")
    print(f"Outputs ({len(results)} items):")
    for i, result in enumerate(results[:3]):  # Show first 3
        print(f"  {i}: {result}")
    if len(results) > 3:
        print(f"  ... ({len(results) - 3} more)")

    assert len(results) == 5, f"Expected 5 items for value=5, got {len(results)}"
    print("[PASS] ConditionalExpanderNode yields variable N based on input")


async def test_pipeline_simulation():
    """Simulate a pipeline: RangeGenerator -> Expander."""
    print("\n=== Testing Pipeline Simulation ===")
    print("Pipeline: RangeGenerator(0..3) -> ExpanderNode(x3)")

    generator = RangeGeneratorNode()
    expander = ExpanderNode(expansion_factor=3)

    # Stage 1: Generate range
    stage1_results = []
    async for item in generator.process(3):  # Range 0..3
        stage1_results.append(item)

    print(f"\nStage 1 (Generator): {len(stage1_results)} items")
    for item in stage1_results:
        print(f"  {item}")

    # Stage 2: Expand each item
    stage2_results = []
    for input_item in stage1_results:
        async for output_item in expander.process(input_item):
            stage2_results.append(output_item)

    print(f"\nStage 2 (Expander): {len(stage2_results)} items")
    print(f"  (showing first 5 of {len(stage2_results)})")
    for item in stage2_results[:5]:
        print(f"  {item}")

    expected_count = len(stage1_results) * 3  # 3 items × 3 expansion = 9
    assert len(stage2_results) == expected_count, \
        f"Expected {expected_count} items, got {len(stage2_results)}"
    print(f"[PASS] Pipeline yields {expected_count} items (3 -> 9 expansion)")


async def main():
    """Run all tests."""
    print("=" * 60)
    print("Testing Python Streaming Nodes (N>=1 yields per process)")
    print("=" * 60)

    tests = [
        test_expander_node,
        test_range_generator,
        test_transform_and_expand,
        test_chained_transform,
        test_conditional_expander,
        test_pipeline_simulation,
    ]

    passed = 0
    failed = 0

    for test in tests:
        try:
            await test()
            passed += 1
        except Exception as e:
            print(f"[FAIL] {test.__name__}: {e}")
            failed += 1

    print("\n" + "=" * 60)
    print(f"Results: {passed} passed, {failed} failed")
    print("=" * 60)

    return failed == 0


if __name__ == "__main__":
    success = asyncio.run(main())
    exit(0 if success else 1)
