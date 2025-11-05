#!/usr/bin/env python3
"""
Example: Pipeline initialization with progress tracking.

This example demonstrates how to use the Pipeline builder with
initialization progress tracking to monitor the startup of
multiple AI model nodes.

Usage:
    python init_with_progress.py
"""

import sys
import time
from remotemedia.core.multiprocessing import Pipeline, InitProgress, InitStatus


def show_progress(progress: InitProgress):
    """
    Progress callback to display initialization status.

    Args:
        progress: Initialization progress information
    """
    # Color codes for terminal output
    BLUE = '\033[94m'
    GREEN = '\033[92m'
    YELLOW = '\033[93m'
    RED = '\033[91m'
    RESET = '\033[0m'

    # Choose color based on status
    if progress.status == InitStatus.STARTING:
        color = BLUE
        icon = "🔵"
    elif progress.status == InitStatus.LOADING_MODEL:
        color = YELLOW
        icon = "⚙️"
    elif progress.status == InitStatus.CONNECTING:
        color = YELLOW
        icon = "🔗"
    elif progress.status == InitStatus.READY:
        color = GREEN
        icon = "✅"
    elif progress.status == InitStatus.FAILED:
        color = RED
        icon = "❌"
    else:
        color = RESET
        icon = "⚪"

    # Format progress bar
    bar_width = 30
    filled = int(bar_width * progress.progress)
    bar = "█" * filled + "░" * (bar_width - filled)

    # Display progress
    print(
        f"{icon} {color}[{progress.node_id:15}]{RESET} "
        f"[{bar}] {progress.progress * 100:5.1f}% - {progress.message}"
    )


def main():
    """
    Main example function demonstrating pipeline initialization
    with progress tracking.
    """
    print("=" * 80)
    print("Pipeline Initialization with Progress Tracking Example")
    print("=" * 80)
    print()

    # Build a speech-to-speech pipeline with 3 AI model nodes
    print("Building pipeline with 3 AI model nodes...")
    pipeline = (Pipeline("speech_to_speech_demo")
        # Voice Activity Detection node
        .add_node(
            "vad",
            "SileroVAD",
            config={
                "model_path": "./models/silero_vad.onnx",
                "threshold": 0.5,
                "sample_rate": 16000
            }
        )

        # Speech-to-Speech model node
        .add_node(
            "s2s",
            "LFM2Audio",
            config={
                "model_path": "./models/lfm2_audio.onnx",
                "max_length": 1024,
                "temperature": 0.8
            },
            dependencies=["vad"]  # Depends on VAD output
        )

        # Text-to-Speech node
        .add_node(
            "tts",
            "VibeVoice",
            config={
                "model_path": "./models/vibe_voice.onnx",
                "voice_id": "en_us_male_1",
                "speed": 1.0
            },
            dependencies=["s2s"]  # Depends on S2S output
        )

        # Connect the nodes
        .connect("vad", "s2s", channel_name="audio_segments")
        .connect("s2s", "tts", channel_name="text_output")

        # Configure initialization timeout
        .set_config("init_timeout_secs", 60)  # 60 second timeout
        .set_config("max_processes", 10)
    )

    print(f"Pipeline built with {len(pipeline.get_nodes())} nodes")
    print(f"Connections: {len(pipeline.get_connections())}")
    print()

    # Initialize the pipeline with progress tracking
    print("Initializing pipeline (this may take up to 60 seconds)...")
    print()

    start_time = time.time()

    try:
        pipeline.initialize(
            timeout_secs=60,
            progress_callback=show_progress
        )

        elapsed = time.time() - start_time
        print()
        print(f"✅ Pipeline initialized successfully in {elapsed:.2f} seconds")
        print()

        # Get final progress for all nodes
        session = pipeline.get_session()
        if session:
            all_progress = session.get_init_progress()
            print(f"All nodes ready: {len(all_progress)} nodes")
            for p in all_progress:
                assert p.status == InitStatus.READY, f"Node {p.node_id} not ready!"

        # Run the pipeline
        print("Starting pipeline execution...")
        pipeline.run()

        # Simulate some processing time
        print("Pipeline running... (press Ctrl+C to stop)")
        try:
            while True:
                time.sleep(1)
        except KeyboardInterrupt:
            print("\nStopping pipeline...")

    except RuntimeError as e:
        print(f"\n❌ Pipeline initialization failed: {e}")
        sys.exit(1)

    finally:
        # Cleanup
        print("Terminating pipeline...")
        pipeline.terminate()
        print("✅ Pipeline terminated successfully")


if __name__ == "__main__":
    main()
