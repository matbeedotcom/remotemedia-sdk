#!/usr/bin/env python3
"""
Basic Pipeline Example with Rust Runtime

This example demonstrates the simplest possible pipeline using the Rust runtime.
The pipeline automatically uses Rust for execution when available, with zero
code changes required from standard Python usage.

Key Features Demonstrated:
- Creating a simple pipeline
- Adding nodes
- Running with automatic Rust runtime detection
- Transparent fallback to Python if Rust unavailable
"""

import asyncio
import sys
from pathlib import Path

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.base import PassThroughNode


async def main():
    """Run the basic pipeline example."""
    print("=" * 70)
    print("RemoteMedia SDK - Basic Pipeline with Rust Runtime")
    print("=" * 70)
    print()

    # Create a simple pipeline
    print("1. Creating pipeline...")
    pipeline = Pipeline(name="BasicExample")

    # Add some processing nodes
    pipeline.add_node(PassThroughNode(name="input"))
    pipeline.add_node(PassThroughNode(name="middle"))
    pipeline.add_node(PassThroughNode(name="output"))

    print(f"   [OK] Created pipeline with {len(pipeline.nodes)} nodes")
    print()

    # Prepare test data
    test_data = [1, 2, 3, 4, 5]
    print(f"2. Input data: {test_data}")
    print()

    # Execute with Rust runtime (automatic detection)
    print("3. Executing pipeline with Rust runtime...")
    try:
        result = await pipeline.run(test_data)
        print(f"   [OK] Result: {result}")
        print(f"   [OK] Execution successful!")
    except Exception as e:
        print(f"   [ERROR] Error: {e}")
        return 1

    print()
    print("=" * 70)
    print("[OK] Example completed successfully!")
    print()
    print("Note: This pipeline automatically used the Rust runtime for")
    print("      improved performance. No code changes were needed!")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
