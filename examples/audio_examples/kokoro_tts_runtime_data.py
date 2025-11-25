"""
Kokoro TTS Node using RuntimeData API

This is an updated version of KokoroTTSNode that uses the new RuntimeData API
for efficient, type-safe communication with the Rust runtime.

Key improvements:
- Direct RuntimeData communication (no JSON serialization)
- Efficient numpy array conversion to audio RuntimeData
- Type-safe input/output handling
- Seamless integration with the streaming node architecture
"""

import logging
import numpy as np
from typing import AsyncGenerator, Optional
import asyncio

# Import RuntimeData bindings
try:
    from remotemedia.core.multiprocessing.data import RuntimeData, numpy_to_audio, audio_to_numpy
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    logging.warning("[KokoroTTSNode] RuntimeData bindings not available. Using fallback implementation.")

logger = logging.getLogger(__name__)


class KokoroTTSNode:
    """
    Text-to-speech synthesis using Kokoro TTS with RuntimeData API.

    This node accepts text via RuntimeData.Text and yields audio chunks via
    RuntimeData.Audio, enabling efficient streaming synthesis.

    Kokoro is an open-weight TTS model with 82 million parameters that delivers
    comparable quality to larger models while being significantly faster and more
    cost-efficient.
    """

    def __init__(
        self,
        node_id: str,
        lang_code: str = 'a',
        voice: str = 'af_heart',
        speed: float = 1.0,
        split_pattern: str = r'\n+',
        sample_rate: int = 24000,
        stream_chunks: bool = True,
        **kwargs
    ):
        """
        Initialize Kokoro TTS node with RuntimeData support.

        Args:
            node_id: Unique identifier for this node instance
            lang_code: Language code ('a' for American English, 'b' for British English,
                      'e' for Spanish, 'f' for French, 'h' for Hindi, 'i' for Italian,
                      'j' for Japanese, 'p' for Brazilian Portuguese, 'z' for Mandarin Chinese)
            voice: Voice identifier (e.g., 'af_heart')
            speed: Speech speed multiplier (default: 1.0)
            split_pattern: Regex pattern for splitting text (default: r'\n+')
            sample_rate: Output sample rate (default: 24000)
            stream_chunks: Whether to stream audio chunks as they're generated (default: True)
        """
        self.node_id = node_id
        self.lang_code = lang_code
        self.voice = voice
        self.speed = speed
        self.split_pattern = split_pattern
        self.sample_rate = sample_rate
        self.stream_chunks = stream_chunks
        self.is_streaming = stream_chunks

        self._pipeline = None
        self._initialized = False

    async def initialize(self) -> None:
        """Initialize the Kokoro TTS pipeline."""
        if self._initialized:
            return

        try:
            # Import Kokoro here to avoid import errors if not installed
            from kokoro import KPipeline

            logger.info(f"Initializing Kokoro TTS with lang_code='{self.lang_code}', voice='{self.voice}'")

            # Initialize the pipeline in a thread to avoid blocking
            self._pipeline = await asyncio.to_thread(
                lambda: KPipeline(lang_code=self.lang_code)
            )

            self._initialized = True
            logger.info("Kokoro TTS pipeline initialized successfully")

        except ImportError as e:
            raise ImportError(
                "Kokoro TTS is not installed. Install with: pip install kokoro>=0.9.4 soundfile"
            ) from e
        except Exception as e:
            logger.error(f"Failed to initialize Kokoro TTS: {e}")
            raise

    async def cleanup(self) -> None:
        """Clean up the TTS pipeline."""
        if self._pipeline is not None:
            self._pipeline = None
            self._initialized = False
            logger.info("Kokoro TTS pipeline cleaned up")

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """
        Process text input and generate speech audio using RuntimeData.

        Args:
            data: RuntimeData containing text to synthesize (RuntimeData.Text)

        Yields:
            RuntimeData.Audio chunks containing synthesized speech

        Raises:
            ValueError: If input is not RuntimeData.Text
            RuntimeError: If TTS synthesis fails
        """
        if not self._initialized:
            await self.initialize()

        # Validate input type
        if not data.is_text():
            raise ValueError(
                f"KokoroTTSNode expects RuntimeData.Text input, got {data.data_type()}"
            )

        # Extract text from RuntimeData
        text = data.as_text()

        if not text or not text.strip():
            logger.warning("Empty text input, skipping synthesis")
            return

        logger.info(f"🎙️ Kokoro TTS: Starting synthesis for text: '{text[:100]}{'...' if len(text) > 100 else ''}'")

        # Synthesize audio and yield chunks
        chunk_count = 0
        total_audio_duration = 0.0

        async for audio_data in self._synthesize_streaming(text):
            chunk_count += 1

            # audio_data is already RuntimeData.Audio from _synthesize_streaming

            # Calculate duration for logging
            audio_array = audio_to_numpy(audio_data)
            chunk_duration = len(audio_array) / self.sample_rate
            total_audio_duration += chunk_duration

            logger.info(
                f"🎙️ Kokoro TTS: Streaming chunk {chunk_count}, "
                f"duration={chunk_duration:.2f}s, total={total_audio_duration:.2f}s"
            )

            yield audio_data

        logger.info(
            f"🎙️ Kokoro TTS: Completed synthesis - {chunk_count} chunks, "
            f"total={total_audio_duration:.2f}s"
        )

    async def _synthesize_streaming(self, text: str) -> AsyncGenerator[RuntimeData, None]:
        """
        Generate audio chunks for the given text using RuntimeData.

        Args:
            text: Text to synthesize

        Yields:
            RuntimeData.Audio containing audio chunks
        """
        try:
            logger.info(
                f"🎙️ Kokoro TTS: Starting streaming synthesis for: "
                f"'{text[:50]}{'...' if len(text) > 50 else ''}'"
            )

            # Run synthesis in a thread to avoid blocking
            generator = await asyncio.to_thread(
                self._create_generator, text
            )

            # Process each generated chunk
            chunk_count = 0
            total_samples = 0

            for i, (graphemes, phonemes, audio) in enumerate(generator):
                chunk_count += 1

                # Ensure audio is a numpy array
                if not isinstance(audio, np.ndarray):
                    audio = np.array(audio, dtype=np.float32)

                # Ensure audio is properly shaped (1D for mono, 2D for multi-channel)
                if audio.ndim == 1:
                    # Mono audio - keep as 1D
                    pass
                elif audio.ndim == 2:
                    # Multi-channel - keep as is
                    pass
                else:
                    # Unexpected shape - flatten
                    audio = audio.flatten()

                chunk_samples = len(audio) if audio.ndim == 1 else audio.shape[0]
                total_samples += chunk_samples
                chunk_duration = chunk_samples / self.sample_rate
                total_duration = total_samples / self.sample_rate

                logger.info(
                    f"🎙️ Kokoro TTS: Chunk {chunk_count}: "
                    f"'{graphemes[:30]}{'...' if len(graphemes) > 30 else ''}' "
                    f"-> {chunk_duration:.2f}s ({chunk_samples} samples) | Total: {total_duration:.2f}s"
                )

                # Convert numpy array to RuntimeData.Audio
                # Assume mono output (channels=1) - adjust if Kokoro outputs stereo
                audio_data = numpy_to_audio(audio, self.sample_rate, channels=1)

                yield audio_data

            logger.info(
                f"🎙️ Kokoro TTS: Synthesis complete - {chunk_count} chunks, "
                f"{total_duration:.2f}s total audio"
            )

        except Exception as e:
            logger.error(f"Error during Kokoro TTS synthesis: {e}")
            raise RuntimeError(f"TTS synthesis failed: {e}") from e

    def _create_generator(self, text: str):
        """Create the Kokoro generator (runs in thread)."""
        return self._pipeline(
            text,
            voice=self.voice,
            speed=self.speed,
            split_pattern=self.split_pattern
        )

    def get_config(self) -> dict:
        """Get node configuration."""
        return {
            "node_id": self.node_id,
            "node_type": "KokoroTTSNode",
            "lang_code": self.lang_code,
            "voice": self.voice,
            "speed": self.speed,
            "split_pattern": self.split_pattern,
            "sample_rate": self.sample_rate,
            "stream_chunks": self.stream_chunks,
        }


# Example usage
async def main():
    """
    Example demonstrating KokoroTTSNode with RuntimeData API
    """
    if not RUNTIME_DATA_AVAILABLE:
        print("RuntimeData bindings not available. Please build the Rust extension.")
        print("Run: cargo build --release")
        return

    print("=" * 60)
    print("Kokoro TTS Node with RuntimeData API")
    print("=" * 60)

    # Create TTS node
    tts_node = KokoroTTSNode(
        node_id="kokoro_tts_1",
        lang_code="a",  # American English
        voice="af_heart",
        speed=1.0,
        sample_rate=24000,
        stream_chunks=True
    )

    # Initialize
    await tts_node.initialize()

    # Create input text as RuntimeData
    input_text = RuntimeData.text(
        "Hello! This is a demonstration of the Kokoro text-to-speech system "
        "using the new RuntimeData API for efficient communication with the Rust runtime."
    )

    # Process and collect audio chunks
    print("\nSynthesizing audio...")
    audio_chunks = []

    async for audio_chunk in tts_node.process(input_text):
        # Convert to numpy for inspection
        audio_array = audio_to_numpy(audio_chunk)
        audio_chunks.append(audio_array)
        print(f"  Received chunk: {len(audio_array)} samples")

    # Cleanup
    await tts_node.cleanup()

    # Summary
    total_samples = sum(len(chunk) for chunk in audio_chunks)
    duration = total_samples / tts_node.sample_rate

    print(f"\nSynthesis complete:")
    print(f"  Total chunks: {len(audio_chunks)}")
    print(f"  Total samples: {total_samples}")
    print(f"  Duration: {duration:.2f} seconds")
    print(f"  Sample rate: {tts_node.sample_rate} Hz")

    print("\n" + "=" * 60)


if __name__ == "__main__":
    asyncio.run(main())
