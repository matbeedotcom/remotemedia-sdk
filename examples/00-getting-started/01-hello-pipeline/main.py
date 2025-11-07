#!/usr/bin/env python3
"""
Hello Pipeline - Your First RemoteMedia SDK Example

This is the simplest possible RemoteMedia pipeline that demonstrates:
1. Loading a pipeline from a YAML manifest
2. Sending text data through the pipeline
3. Receiving and printing the output

No audio processing, no complex setup - just the basics!
"""

import asyncio
from remotemedia.core.pipeline import Pipeline


async def main():
    """Run a simple text processing pipeline"""

    print("=" * 60)
    print("🎯 Hello Pipeline - Your First RemoteMedia Example")
    print("=" * 60)
    print()

    # Step 1: Load the pipeline from YAML manifest
    print("Step 1: Loading pipeline from pipeline.yaml...")
    pipeline = await Pipeline.from_yaml_file("pipeline.yaml")
    print("✅ Pipeline loaded successfully!")
    print()

    # Step 2: Prepare input data
    print("Step 2: Preparing input data...")
    input_text = "Hello, RemoteMedia SDK!"
    print(f"   Input: '{input_text}'")
    print()

    # Step 3: Run the pipeline
    print("Step 3: Running the pipeline...")
    result = await pipeline.run({
        "text": input_text
    })
    print("✅ Pipeline execution complete!")
    print()

    # Step 4: Display the result
    print("Step 4: Results:")
    print("-" * 60)
    if "processed_text" in result:
        print(f"   Output: '{result['processed_text']}'")
    else:
        print(f"   Raw result: {result}")
    print("-" * 60)
    print()

    # Success message
    print("=" * 60)
    print("🎉 Congratulations! You've run your first RemoteMedia pipeline!")
    print("=" * 60)
    print()
    print("What you just did:")
    print("  ✅ Loaded a pipeline from YAML configuration")
    print("  ✅ Sent data through a processing node")
    print("  ✅ Received and displayed the output")
    print()
    print("Next steps:")
    print("  📚 Try ../02-basic-audio/ for audio processing")
    print("  🔬 Experiment by changing the input text above")
    print("  📖 Read the README.md to understand how it works")
    print()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n\n⚠️  Pipeline interrupted by user")
    except Exception as e:
        print(f"\n\n❌ Error running pipeline: {e}")
        print("\n💡 Troubleshooting:")
        print("   1. Make sure you've installed: pip install remotemedia")
        print("   2. Check that pipeline.yaml exists in this directory")
        print("   3. See README.md for detailed setup instructions")
        import traceback
        traceback.print_exc()
