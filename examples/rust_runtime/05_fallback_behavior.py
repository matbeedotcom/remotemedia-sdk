#!/usr/bin/env python3
"""
Fallback Behavior Example

This example demonstrates the automatic fallback behavior when the Rust
runtime is unavailable or fails, ensuring your code works everywhere.

Key Features Demonstrated:
- Automatic fallback to Python executor
- Graceful degradation when Rust unavailable
- Explicit runtime control
- Error handling and recovery
"""

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.simple_math import MultiplyNode


async def main():
    """Run the fallback behavior example."""
    print("=" * 70)
    print("RemoteMedia SDK - Fallback Behavior")
    print("=" * 70)
    print()

    # Create pipeline
    pipeline = Pipeline(name="FallbackExample")
    pipeline.add_node(MultiplyNode(factor=3, name="multiply"))

    test_data = [1, 2, 3, 4, 5]

    # Test 1: Automatic runtime selection (default)
    print("1. Automatic runtime selection (use_rust=True, default)")
    print("   - Will try Rust first")
    print("   - Falls back to Python if Rust unavailable")
    print()

    result1 = await pipeline.run(test_data, use_rust=True)
    print(f"   Input:  {test_data}")
    print(f"   Output: {result1}")
    print(f"   [OK] Execution successful (runtime was selected automatically)")
    print()

    # Test 2: Force Python executor
    print("2. Forced Python execution (use_rust=False)")
    print("   - Always uses Python executor")
    print("   - Guaranteed to work on any system")
    print()

    result2 = await pipeline.run(test_data, use_rust=False)
    print(f"   Input:  {test_data}")
    print(f"   Output: {result2}")
    print(f"   [OK] Python execution successful")
    print()

    # Test 3: Verify both produce same results
    print("3. Comparing results...")
    if result1 == result2:
        print("   [OK] Both runtimes produced identical results!")
    else:
        print("   [ERROR] Results differ (this shouldn't happen!)")
        return 1
    print()

    # Show what actually happened
    try:
        import remotemedia_runtime
        print("4. Runtime status:")
        print(f"   [OK] Rust runtime is available")
        print(f"   [OK] Version: {remotemedia_runtime.__version__}")
        print(f"   [OK] Test 1 used: Rust runtime")
        print(f"   [OK] Test 2 used: Python executor (forced)")
    except ImportError:
        print("4. Runtime status:")
        print(f"   [WARN] Rust runtime is not available")
        print(f"   [OK] Test 1 used: Python executor (automatic fallback)")
        print(f"   [OK] Test 2 used: Python executor (forced)")
    print()

    print("=" * 70)
    print("[OK] Fallback behavior works correctly!")
    print("[OK] Your code runs everywhere, with or without Rust runtime")
    print()
    print("Key Takeaway:")
    print("  - use_rust=True (default): Best performance, falls back gracefully")
    print("  - use_rust=False: Explicit Python, guaranteed compatibility")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
