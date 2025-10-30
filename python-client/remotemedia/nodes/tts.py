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
from typing import AsyncGenerator, Optional, TYPE_CHECKING
import asyncio

# Import RuntimeData bindings
if TYPE_CHECKING:
    from remotemedia_runtime.runtime_data import RuntimeData

try:
    from remotemedia_runtime.runtime_data import RuntimeData, numpy_to_audio, audio_to_numpy
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    RuntimeData = None  # type: ignore
    numpy_to_audio = None  # type: ignore
    audio_to_numpy = None  # type: ignore
    logging.warning("RuntimeData bindings not available. Using fallback implementation.")

logger = logging.getLogger(__name__)

# Configure logger to output to console
if not logger.handlers:
    console_handler = logging.StreamHandler()
    console_handler.setLevel(logging.INFO)
    formatter = logging.Formatter('%(levelname)s:%(name)s:%(message)s')
    console_handler.setFormatter(formatter)
    logger.addHandler(console_handler)
    logger.setLevel(logging.INFO)


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
        split_pattern: str = r'[.!?,;:\n]+',
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

            # Initialize the pipeline synchronously
            # WORKAROUND: Kokoro's PyTorch operations cause heap corruption when called
            # from Rust/PyO3 event loop on Windows. We initialize here which is safe.
            self._pipeline = KPipeline(lang_code=self.lang_code)

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

    def _synthesize_chunk_sync(self, generator_iter) -> tuple:
        """
        Get the next chunk from Kokoro generator synchronously (thread-safe).

        This runs in a thread to isolate PyTorch operations.

        Args:
            generator_iter: Iterator from Kokoro pipeline

        Returns:
            Tuple of (graphemes, phonemes, audio_array) or None if exhausted
        """
        try:
            graphemes, phonemes, audio = next(generator_iter)

            # Ensure audio is a numpy array
            if not isinstance(audio, np.ndarray):
                audio = np.array(audio, dtype=np.float32)

            # Ensure audio is properly shaped (1D for mono)
            if audio.ndim == 1:
                pass  # Mono audio - keep as 1D
            elif audio.ndim == 2:
                # If stereo/multi-channel, take first channel
                audio = audio[:, 0] if audio.shape[1] > 0 else audio.flatten()
            else:
                audio = audio.flatten()

            chunk_samples = len(audio)
            chunk_duration = chunk_samples / self.sample_rate
            logger.info(
                f"🎙️ Kokoro chunk: '{graphemes[:30]}...' "
                f"-> {chunk_duration:.2f}s ({chunk_samples} samples)"
            )

            return (graphemes, phonemes, audio)
        except StopIteration:
            return None

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """
        Process text input and generate speech audio chunks incrementally.

        ARCHITECTURE: To avoid heap corruption with PyTorch on Windows, we run EACH
        PyTorch operation (next() call on Kokoro generator) in a thread pool. This
        allows true streaming: chunks are yielded as soon as Kokoro generates them.

        Args:
            data: RuntimeData containing text to synthesize (RuntimeData.Text)

        Yields:
            RuntimeData.Audio containing synthesized speech chunks

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

        logger.info(f"🎙️ Kokoro TTS: Starting synthesis for: '{text[:100]}{'...' if len(text) > 100 else ''}'")

        # Create the Kokoro generator (this is safe, doesn't run PyTorch yet)
        generator = self._create_generator(text)

        # Now iterate the generator, running EACH next() call in a thread
        import asyncio
        chunk_idx = 0
        total_samples = 0

        while True:
            # Get next chunk from generator in thread (PyTorch-safe)
            chunk_data = await asyncio.to_thread(self._synthesize_chunk_sync, generator)

            if chunk_data is None:
                # Generator exhausted
                break

            chunk_idx += 1
            graphemes, phonemes, audio = chunk_data

            # Convert to RuntimeData.Audio
            audio_runtime_data = numpy_to_audio(audio, self.sample_rate, channels=1)

            total_samples += len(audio)
            chunk_duration = len(audio) / self.sample_rate
            total_duration = total_samples / self.sample_rate

            logger.info(
                f"🎙️ Kokoro TTS: Yielding chunk {chunk_idx} "
                f"({chunk_duration:.2f}s) | Total: {total_duration:.2f}s"
            )

            # Yield immediately!
            yield audio_runtime_data

        logger.info(f"🎙️ Kokoro TTS: Streaming complete - {chunk_idx} chunks, {total_samples} samples ({total_duration:.2f}s)")

    async def _synthesize_streaming(self, text: str) -> AsyncGenerator[RuntimeData, None]:
        """
        Generate audio chunks for the given text.

        Args:
            text: Text to synthesize

        Yields:
            RuntimeData.Audio containing synthesized speech chunks
        """
        try:
            logger.info(
                f"🎙️ Kokoro TTS: Starting streaming synthesis for: "
                f"'{text[:50]}{'...' if len(text) > 50 else ''}'"
            )

            # WORKAROUND FOR WINDOWS HEAP CORRUPTION:
            # Calling Kokoro's generator directly from async context causes heap corruption
            # on Windows when called through Rust/PyO3. Solution: Call Kokoro synchronously
            # OUTSIDE the async generator, collect all results, then yield them.

            logger.info("Synthesizing ALL audio synchronously (Windows workaround)")

            # Call Kokoro synchronously and collect all chunks
            generator = self._create_generator(text)
            all_audio_chunks = []

            for i, (graphemes, phonemes, audio) in enumerate(generator):
                # Ensure audio is a numpy array
                if not isinstance(audio, np.ndarray):
                    audio = np.array(audio, dtype=np.float32)

                # Ensure audio is properly shaped (1D for mono, 2D for multi-channel)
                if audio.ndim == 1:
                    pass  # Mono audio - keep as 1D
                elif audio.ndim == 2:
                    pass  # Multi-channel - keep as is
                else:
                    audio = audio.flatten()  # Unexpected shape - flatten

                all_audio_chunks.append((graphemes, phonemes, audio))

            logger.info(f"Kokoro generated {len(all_audio_chunks)} chunks, now yielding them")

            # Now yield the pre-generated chunks through the async generator
            chunk_count = 0
            total_samples = 0

            for graphemes, phonemes, audio in all_audio_chunks:
                chunk_count += 1

                chunk_samples = len(audio) if audio.ndim == 1 else audio.shape[0]
                total_samples += chunk_samples
                chunk_duration = chunk_samples / self.sample_rate
                total_duration = total_samples / self.sample_rate

                logger.info(
                    f"🎙️ Kokoro TTS: Chunk {chunk_count}: "
                    f"'{graphemes[:30]}{'...' if len(graphemes) > 30 else ''}' "
                    f"-> {chunk_duration:.2f}s ({chunk_samples} samples) | Total: {total_duration:.2f}s"
                )

                # Convert numpy audio to RuntimeData.Audio BEFORE yielding
                # This is safe now that we've fixed the event loop close issue
                audio_runtime_data = numpy_to_audio(audio, self.sample_rate, channels=1)

                yield audio_runtime_data

            logger.info(
                f"🎙️ Kokoro TTS: Synthesis complete - {chunk_count} chunks, "
                f"{total_duration:.2f}s total audio"
            )
            return

            # # Original streaming code - causes heap corruption on Windows
            # generator = self._create_generator(text)
            #
            # # Process each generated chunk
            # chunk_count = 0
            # total_samples = 0
            #
            # for i, (graphemes, phonemes, audio) in enumerate(generator):
            #     chunk_count += 1
            #
            #     # Ensure audio is a numpy array
            #     if not isinstance(audio, np.ndarray):
            #         audio = np.array(audio, dtype=np.float32)
            #
            #     # Ensure audio is properly shaped (1D for mono, 2D for multi-channel)
            #     if audio.ndim == 1:
            #         # Mono audio - keep as 1D
            #         pass
            #     elif audio.ndim == 2:
            #         # Multi-channel - keep as is
            #         pass
            #     else:
            #         # Unexpected shape - flatten
            #         audio = audio.flatten()
            #
            #     chunk_samples = len(audio) if audio.ndim == 1 else audio.shape[0]
            #     total_samples += chunk_samples
            #     chunk_duration = chunk_samples / self.sample_rate
            #     total_duration = total_samples / self.sample_rate
            #
            #     logger.info(
            #         f"🎙️ Kokoro TTS: Chunk {chunk_count}: "
            #         f"'{graphemes[:30]}{'...' if len(graphemes) > 30 else ''}' "
            #         f"-> {chunk_duration:.2f}s ({chunk_samples} samples) | Total: {total_duration:.2f}s"
            #     )
            #
            #     # Convert numpy array to RuntimeData.Audio
            #     # Assume mono output (channels=1) - adjust if Kokoro outputs stereo
            #     audio_data = numpy_to_audio(audio, self.sample_rate, channels=1)
            #
            #     yield audio_data
            #
            # logger.info(
            #     f"🎙️ Kokoro TTS: Synthesis complete - {chunk_count} chunks, "
            #     f"{total_duration:.2f}s total audio"
            # )

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
