"""
Example custom Python node for Docker executor demonstration.

This node demonstrates how to create a simple Python processing node
that runs in a Docker container with iceoryx2 IPC for zero-copy data transfer.
"""

import asyncio
from typing import AsyncGenerator


class EchoNode:
    """
    Simple echo node that passes through audio data with metadata annotation.

    This demonstrates the minimal interface required for a Docker-based
    streaming node in the RemoteMedia SDK.
    """

    def __init__(self):
        """Initialize the echo node."""
        self.processed_count = 0

    async def initialize(self):
        """
        Called once when the container starts.

        Use this for:
        - Loading ML models
        - Initializing resources
        - Setting up connections
        """
        print("[EchoNode] Initializing in Docker container")
        print(f"[EchoNode] Python version: {__import__('sys').version}")

    async def process(self, data: dict) -> AsyncGenerator[dict, None]:
        """
        Process incoming data and yield results.

        Args:
            data: RuntimeData dict with fields:
                - data_type: str ("audio", "text", "image", etc.)
                - For audio: samples (list of floats), sample_rate, channels
                - timestamp: int

        Yields:
            Processed RuntimeData dicts
        """
        self.processed_count += 1

        # Example: Pass through audio with processing count annotation
        if data.get("data_type") == "audio":
            print(f"[EchoNode] Processing audio chunk #{self.processed_count} "
                  f"({len(data.get('samples', []))} samples @ {data.get('sample_rate')}Hz)")

            # Echo the audio back (could apply processing here)
            yield {
                "data_type": "audio",
                "samples": data["samples"],
                "sample_rate": data["sample_rate"],
                "channels": data["channels"],
                "timestamp": data.get("timestamp", 0),
                "metadata": {
                    "processed_by": "EchoNode",
                    "processing_count": self.processed_count,
                    "container": "docker"
                }
            }

        elif data.get("data_type") == "text":
            print(f"[EchoNode] Processing text: {data.get('text', '')[:50]}")

            # Echo text with prefix
            yield {
                "data_type": "text",
                "text": f"[Processed in Docker] {data.get('text', '')}",
                "timestamp": data.get("timestamp", 0)
            }

    async def cleanup(self):
        """
        Called when the container stops.

        Use this for:
        - Releasing resources
        - Closing connections
        - Saving state
        """
        print(f"[EchoNode] Cleaning up (processed {self.processed_count} chunks)")


class AudioAmplifierNode:
    """
    Example node that amplifies audio by a configurable gain factor.

    Demonstrates:
    - Audio processing in Docker
    - Configurable parameters
    - Sample manipulation
    """

    def __init__(self, gain: float = 1.5):
        """
        Initialize amplifier node.

        Args:
            gain: Amplification factor (default: 1.5x)
        """
        self.gain = gain

    async def initialize(self):
        """Initialize the amplifier."""
        print(f"[AudioAmplifierNode] Initializing with gain={self.gain}")

    async def process(self, data: dict) -> AsyncGenerator[dict, None]:
        """
        Amplify audio samples.

        Args:
            data: RuntimeData with audio samples

        Yields:
            Amplified audio data
        """
        if data.get("data_type") == "audio":
            samples = data["samples"]

            # Amplify samples
            amplified = [s * self.gain for s in samples]

            # Clip to prevent overflow
            amplified = [max(-1.0, min(1.0, s)) for s in amplified]

            yield {
                "data_type": "audio",
                "samples": amplified,
                "sample_rate": data["sample_rate"],
                "channels": data["channels"],
                "timestamp": data.get("timestamp", 0),
                "metadata": {
                    "gain_applied": self.gain,
                    "clipped": any(abs(s) > 1.0 for s in samples)
                }
            }

    async def cleanup(self):
        """Cleanup amplifier resources."""
        print("[AudioAmplifierNode] Cleanup complete")


class TextUppercaseNode:
    """
    Example text processing node.

    Demonstrates simple text transformation in Docker container.
    """

    async def initialize(self):
        """Initialize text processor."""
        print("[TextUppercaseNode] Initialized")

    async def process(self, data: dict) -> AsyncGenerator[dict, None]:
        """
        Convert text to uppercase.

        Args:
            data: RuntimeData with text field

        Yields:
            Uppercased text data
        """
        if data.get("data_type") == "text":
            text = data.get("text", "")

            yield {
                "data_type": "text",
                "text": text.upper(),
                "timestamp": data.get("timestamp", 0),
                "metadata": {
                    "transformation": "uppercase",
                    "original_length": len(text)
                }
            }

    async def cleanup(self):
        """Cleanup text processor."""
        print("[TextUppercaseNode] Cleanup complete")


# Export node classes for RemoteMedia SDK discovery
__all__ = ["EchoNode", "AudioAmplifierNode", "TextUppercaseNode"]
