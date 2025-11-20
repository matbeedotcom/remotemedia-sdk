"""
Python Node Example using RuntimeData API

This example demonstrates how to use the new RuntimeData API for Python nodes
that communicate directly with the Rust runtime without JSON serialization.

The RuntimeData API provides type-safe, efficient data exchange between Python
nodes and the Rust execution engine.
"""

import numpy as np
from typing import AsyncGenerator, Optional
import asyncio

# Import the RuntimeData bindings from the Rust extension
# (These will be available when the remotemedia Rust extension is built with PyO3)
try:
    from remotemedia.core.multiprocessing.data import RuntimeData, numpy_to_audio, audio_to_numpy
except ImportError:
    print("Note: RuntimeData bindings not yet available. Build the Rust extension first.")
    print("This is a reference implementation showing the API usage.")
    # Define stub classes for demonstration
    class RuntimeData:
        @staticmethod
        def text(text: str): pass
        @staticmethod
        def audio(samples, sample_rate, channels, format, num_samples): pass
        @staticmethod
        def json(data): pass
        @staticmethod
        def binary(data): pass

        def data_type(self) -> str: pass
        def is_text(self) -> bool: pass
        def is_audio(self) -> bool: pass
        def is_json(self) -> bool: pass
        def as_text(self) -> Optional[str]: pass
        def as_audio(self): pass
        def as_json(self): pass
        def as_binary(self): pass


class TextProcessorNode:
    """
    Example node that processes text using RuntimeData.

    This node converts text to uppercase - demonstrates basic RuntimeData usage.
    """

    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.params = params
        print(f"TextProcessorNode initialized: {node_id}")

    async def initialize(self):
        """Initialize the node (called once before processing)"""
        print(f"TextProcessorNode {self.node_id}: Ready to process text")

    async def process(self, data: RuntimeData) -> RuntimeData:
        """
        Process incoming RuntimeData.

        Args:
            data: RuntimeData containing text input

        Returns:
            RuntimeData containing processed text
        """
        # Check if input is text
        if not data.is_text():
            raise ValueError(f"Expected text input, got {data.data_type()}")

        # Extract the text
        text = data.as_text()
        print(f"Processing text: '{text[:50]}...'")

        # Process (convert to uppercase)
        processed_text = text.upper()

        # Return as RuntimeData
        return RuntimeData.text(processed_text)

    async def cleanup(self):
        """Cleanup resources (called when node is done)"""
        print(f"TextProcessorNode {self.node_id}: Cleanup complete")


class AudioProcessorNode:
    """
    Example node that processes audio using RuntimeData with numpy.

    This node applies a simple gain to audio - demonstrates audio RuntimeData usage.
    """

    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.gain = params.get('gain', 1.0)
        print(f"AudioProcessorNode initialized: {node_id}, gain={self.gain}")

    async def initialize(self):
        """Initialize the node"""
        print(f"AudioProcessorNode {self.node_id}: Ready to process audio")

    async def process(self, data: RuntimeData) -> RuntimeData:
        """
        Process audio data.

        Args:
            data: RuntimeData containing audio input

        Returns:
            RuntimeData containing processed audio
        """
        # Check if input is audio
        if not data.is_audio():
            raise ValueError(f"Expected audio input, got {data.data_type()}")

        # Convert RuntimeData to numpy array
        audio_array = audio_to_numpy(data)
        print(f"Processing audio: shape={audio_array.shape}, dtype={audio_array.dtype}")

        # Process: apply gain
        processed_audio = audio_array * self.gain

        # Ensure output is in valid range [-1.0, 1.0]
        processed_audio = np.clip(processed_audio, -1.0, 1.0)

        # Extract audio metadata from original data
        samples_bytes, sample_rate, channels, format_str, num_samples = data.as_audio()

        # Convert back to RuntimeData
        return numpy_to_audio(processed_audio, sample_rate, channels)

    async def cleanup(self):
        """Cleanup resources"""
        print(f"AudioProcessorNode {self.node_id}: Cleanup complete")


class StreamingTTSNode:
    """
    Example streaming TTS node that yields audio chunks.

    This demonstrates how a TTS node like KokoroTTS would work with RuntimeData.
    """

    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.voice = params.get('voice', 'default')
        self.sample_rate = params.get('sample_rate', 24000)
        self.is_streaming = True  # Indicates this node streams output
        print(f"StreamingTTSNode initialized: {node_id}, voice={self.voice}")

    async def initialize(self):
        """Initialize the TTS engine"""
        print(f"StreamingTTSNode {self.node_id}: Loading TTS model...")
        # In real implementation, load the TTS model here
        await asyncio.sleep(0.1)  # Simulate loading time
        print(f"StreamingTTSNode {self.node_id}: Ready")

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """
        Process text and yield audio chunks.

        Args:
            data: RuntimeData containing text to synthesize

        Yields:
            RuntimeData containing audio chunks
        """
        # Extract text
        if not data.is_text():
            raise ValueError(f"Expected text input, got {data.data_type()}")

        text = data.as_text()
        print(f"Synthesizing: '{text[:50]}...'")

        # Simulate TTS synthesis by generating audio chunks
        # In real implementation, this would call the actual TTS engine
        num_chunks = 5
        samples_per_chunk = 4800  # 200ms at 24kHz

        for i in range(num_chunks):
            # Generate dummy audio data (in real implementation, call TTS engine)
            audio_chunk = np.random.randn(samples_per_chunk).astype(np.float32) * 0.1

            print(f"Yielding chunk {i+1}/{num_chunks}: {len(audio_chunk)} samples")

            # Convert numpy array to RuntimeData
            audio_data = numpy_to_audio(audio_chunk, self.sample_rate, channels=1)

            yield audio_data

            # Simulate processing time
            await asyncio.sleep(0.05)

        print(f"Synthesis complete: {num_chunks} chunks generated")

    async def cleanup(self):
        """Cleanup TTS resources"""
        print(f"StreamingTTSNode {self.node_id}: Cleanup complete")


class JSONProcessorNode:
    """
    Example node that processes JSON data using RuntimeData.

    Demonstrates working with structured JSON data.
    """

    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.params = params

    async def initialize(self):
        print(f"JSONProcessorNode {self.node_id}: Ready")

    async def process(self, data: RuntimeData) -> RuntimeData:
        """
        Process JSON data.

        Args:
            data: RuntimeData containing JSON input

        Returns:
            RuntimeData containing processed JSON
        """
        if not data.is_json():
            raise ValueError(f"Expected JSON input, got {data.data_type()}")

        # Extract JSON as Python dict/list
        json_data = data.as_json()
        print(f"Processing JSON: {json_data}")

        # Process: add metadata
        result = {
            "original": json_data,
            "processed_by": self.node_id,
            "timestamp": "2025-10-29T00:00:00Z"
        }

        # Return as RuntimeData
        return RuntimeData.json(result)

    async def cleanup(self):
        print(f"JSONProcessorNode {self.node_id}: Cleanup complete")


# Example usage
async def main():
    """Demonstrate the RuntimeData API with various node types"""

    print("=" * 60)
    print("Python RuntimeData API Examples")
    print("=" * 60)

    # Example 1: Text Processing
    print("\n--- Example 1: Text Processing ---")
    text_node = TextProcessorNode("text_proc_1", {})
    await text_node.initialize()

    input_text = RuntimeData.text("hello world from python!")
    output_text = await text_node.process(input_text)
    print(f"Output: {output_text.as_text()}")

    await text_node.cleanup()

    # Example 2: Audio Processing
    print("\n--- Example 2: Audio Processing ---")
    audio_node = AudioProcessorNode("audio_proc_1", {"gain": 1.5})
    await audio_node.initialize()

    # Create sample audio data
    sample_audio = np.random.randn(4800).astype(np.float32) * 0.3
    input_audio = numpy_to_audio(sample_audio, sample_rate=24000, channels=1)

    output_audio = await audio_node.process(input_audio)
    output_array = audio_to_numpy(output_audio)
    print(f"Output audio shape: {output_array.shape}")

    await audio_node.cleanup()

    # Example 3: Streaming TTS
    print("\n--- Example 3: Streaming TTS ---")
    tts_node = StreamingTTSNode("tts_1", {"voice": "en-US", "sample_rate": 24000})
    await tts_node.initialize()

    input_text = RuntimeData.text("This is a test of streaming text-to-speech synthesis.")

    chunk_count = 0
    async for audio_chunk in tts_node.process(input_text):
        chunk_count += 1
        audio_array = audio_to_numpy(audio_chunk)
        print(f"  Received chunk {chunk_count}: {len(audio_array)} samples")

    await tts_node.cleanup()

    # Example 4: JSON Processing
    print("\n--- Example 4: JSON Processing ---")
    json_node = JSONProcessorNode("json_proc_1", {})
    await json_node.initialize()

    input_json = RuntimeData.json({"message": "Hello", "count": 42})
    output_json = await json_node.process(input_json)
    print(f"Output: {output_json.as_json()}")

    await json_node.cleanup()

    print("\n" + "=" * 60)
    print("All examples completed successfully!")
    print("=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
