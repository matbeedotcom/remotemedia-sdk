#!/usr/bin/env python3
"""
VAD-triggered LFM2-Audio Speech-to-Speech Pipeline

This example demonstrates a complete real-time speech-to-speech pipeline:
1. Continuous audio stream input (simulating microphone)
2. VAD detects speech segments
3. VAD-triggered buffer accumulates complete utterances
4. LFM2-Audio generates conversational responses
5. Audio responses are played back or saved

This is the recommended pattern for conversational AI applications.

**TO RUN THIS EXAMPLE:**

1. **Install dependencies:**
   ```bash
   pip install liquid-audio torch torchaudio soundfile librosa
   ```

2. **Run the script:**
   ```bash
   python examples/audio_examples/vad_lfm2_audio_streaming.py
   ```

**Pipeline Architecture:**

```
Audio Stream → VAD → VAD Buffer → LFM2-Audio → Audio Output
                ↓                      ↓
            (metadata)            (text + audio)
```

**Key Features:**
- Real-time speech detection
- Buffering of complete utterances
- Multi-turn conversation support
- Low-latency response generation
"""

import asyncio
import logging
import numpy as np
import soundfile as sf
import os
import sys
from pathlib import Path
from typing import AsyncGenerator, Any, Optional, Dict

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia.nodes.ml import LFM2AudioNode
from remotemedia.nodes.audio import VoiceActivityDetector, AudioTransform

try:
    from remotemedia.core.multiprocessing.data import RuntimeData, numpy_to_audio, audio_to_numpy
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    print("RuntimeData bindings not available. Please build the Rust extension.")
    print("Run: cargo build --release")
    RUNTIME_DATA_AVAILABLE = False
    sys.exit(1)

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# Reduce noise
logging.getLogger("transformers").setLevel(logging.WARNING)
logging.getLogger("torch").setLevel(logging.WARNING)


class VADTriggeredBufferNode:
    """
    Buffer that accumulates audio until a complete speech segment is detected.

    Uses VAD metadata to determine when to trigger processing.
    """

    def __init__(
        self,
        min_speech_duration_s: float = 0.8,
        max_speech_duration_s: float = 10.0,
        silence_duration_s: float = 1.0,
        sample_rate: int = 24000,
    ):
        """
        Initialize VAD-triggered buffer.

        Args:
            min_speech_duration_s: Minimum speech duration before triggering (seconds)
            max_speech_duration_s: Maximum speech duration (force trigger)
            silence_duration_s: Duration of silence to confirm speech end
            sample_rate: Audio sample rate
        """
        self.min_speech_duration_s = min_speech_duration_s
        self.max_speech_duration_s = max_speech_duration_s
        self.silence_duration_s = silence_duration_s
        self.sample_rate = sample_rate

        # State
        self._buffer: list[np.ndarray] = []
        self._speech_frames = 0
        self._silence_frames = 0
        self._total_frames = 0
        self._is_speech_active = False

        # Calculate frame thresholds
        frame_duration_s = 0.03  # 30ms frames (typical VAD frame size)
        self._min_speech_frames = int(min_speech_duration_s / frame_duration_s)
        self._max_speech_frames = int(max_speech_duration_s / frame_duration_s)
        self._silence_frames_threshold = int(silence_duration_s / frame_duration_s)

        logger.info(
            f"VAD Buffer initialized: min_speech={self._min_speech_frames} frames, "
            f"max={self._max_speech_frames} frames, silence={self._silence_frames_threshold} frames"
        )

    async def process(
        self, data_stream: AsyncGenerator[tuple[RuntimeData, Dict[str, Any]], None]
    ) -> AsyncGenerator[RuntimeData, None]:
        """
        Process audio stream with VAD metadata.

        Args:
            data_stream: Stream of (RuntimeData, metadata) tuples

        Yields:
            RuntimeData containing complete speech segments
        """
        async for runtime_data, metadata in data_stream:
            if not runtime_data.is_audio():
                continue

            # Extract audio
            audio_chunk = audio_to_numpy(runtime_data)

            # Check VAD metadata
            has_speech = metadata.get('has_speech', False)

            # Update state
            self._total_frames += 1
            if has_speech:
                self._speech_frames += 1
                self._silence_frames = 0
                self._is_speech_active = True
            else:
                if self._is_speech_active:
                    self._silence_frames += 1

            # Buffer audio
            self._buffer.append(audio_chunk)

            # Check trigger conditions
            should_trigger = False

            # Condition 1: Max duration reached
            if self._total_frames >= self._max_speech_frames:
                logger.info(f"🔊 VAD Buffer: Max duration reached ({self.max_speech_duration_s}s)")
                should_trigger = True

            # Condition 2: Sufficient speech + silence detected
            elif (
                self._speech_frames >= self._min_speech_frames
                and self._silence_frames >= self._silence_frames_threshold
            ):
                speech_duration = self._speech_frames * 0.03
                logger.info(
                    f"🔊 VAD Buffer: Complete utterance detected ({speech_duration:.2f}s speech)"
                )
                should_trigger = True

            # Trigger processing
            if should_trigger and len(self._buffer) > 0:
                # Concatenate buffer
                full_audio = np.concatenate(self._buffer)
                duration_s = len(full_audio) / self.sample_rate

                logger.info(
                    f"📤 VAD Buffer: Outputting {len(full_audio)} samples ({duration_s:.2f}s)"
                )

                # Yield as RuntimeData
                yield numpy_to_audio(full_audio, self.sample_rate, channels=1)

                # Reset buffer
                self._buffer = []
                self._speech_frames = 0
                self._silence_frames = 0
                self._total_frames = 0
                self._is_speech_active = False


async def create_demo_audio_stream(
    filepath: str = "demo_conversation.wav",
    duration_s: int = 20,
) -> str:
    """Create demo audio with multiple speech segments separated by pauses."""
    if os.path.exists(filepath):
        logger.info(f"Demo audio already exists: {filepath}")
        return filepath

    logger.info(f"Creating demo conversation audio: {filepath}")

    sample_rate = 24000
    samples = int(sample_rate * duration_s)
    t = np.linspace(0, duration_s, samples, dtype=np.float32)
    audio = np.zeros(samples, dtype=np.float32)

    # Define speech segments with pauses
    speech_segments = [
        (1.0, 3.5, "Hello, how are you?"),
        (5.0, 8.0, "Tell me about artificial intelligence"),
        (10.0, 13.5, "What can you do to help me?"),
        (16.0, 18.0, "Thank you very much"),
    ]

    for start, end, description in speech_segments:
        mask = (t >= start) & (t < end)
        # Create speech-like audio
        freq_base = 200 + 150 * np.sin(2 * np.pi * 0.5 * t[mask])
        freq_mod = 100 * np.sin(2 * np.pi * 5 * t[mask])
        speech_signal = 0.3 * np.sin(2 * np.pi * (freq_base + freq_mod) * t[mask])
        audio[mask] = speech_signal
        logger.info(f"  Speech: {start:.1f}s-{end:.1f}s ({description})")

    # Add noise
    audio += 0.01 * np.random.randn(samples).astype(np.float32)

    # Save
    sf.write(filepath, audio, sample_rate)
    logger.info(f"Created: {duration_s}s @ {sample_rate}Hz with {len(speech_segments)} segments")

    return filepath


async def audio_chunk_generator(
    audio_file: str, chunk_size_ms: int = 100
) -> AsyncGenerator[np.ndarray, None]:
    """
    Generate audio chunks simulating real-time streaming.

    Args:
        audio_file: Path to audio file
        chunk_size_ms: Size of each chunk in milliseconds

    Yields:
        Audio chunks as numpy arrays
    """
    # Load audio
    audio_data, sample_rate = sf.read(audio_file, dtype='float32')
    logger.info(f"Loaded audio: {len(audio_data)} samples @ {sample_rate}Hz")

    # Calculate chunk size
    chunk_size_samples = int(sample_rate * chunk_size_ms / 1000)
    logger.info(f"Chunk size: {chunk_size_samples} samples ({chunk_size_ms}ms)")

    # Stream chunks
    num_chunks = len(audio_data) // chunk_size_samples
    for i in range(num_chunks):
        start_idx = i * chunk_size_samples
        end_idx = start_idx + chunk_size_samples
        chunk = audio_data[start_idx:end_idx]

        yield chunk

        # Simulate real-time streaming delay
        await asyncio.sleep(chunk_size_ms / 1000.0)

    # Yield remaining samples
    if len(audio_data) % chunk_size_samples != 0:
        remaining = audio_data[num_chunks * chunk_size_samples :]
        yield remaining


async def vad_process_wrapper(
    vad_node: VoiceActivityDetector, audio_stream: AsyncGenerator[np.ndarray, None]
) -> AsyncGenerator[tuple[RuntimeData, Dict[str, Any]], None]:
    """
    Wrapper to convert audio chunks to RuntimeData and add VAD metadata.

    Args:
        vad_node: VAD node instance
        audio_stream: Stream of audio chunks

    Yields:
        (RuntimeData, metadata) tuples
    """
    async for audio_chunk in audio_stream:
        # Convert to RuntimeData
        runtime_data = numpy_to_audio(audio_chunk, 24000, channels=1)

        # Simple energy-based VAD (placeholder - real VAD would be more sophisticated)
        energy = np.sum(audio_chunk**2) / len(audio_chunk)
        has_speech = energy > 0.02  # Energy threshold

        metadata = {
            'has_speech': has_speech,
            'energy': float(energy),
        }

        yield runtime_data, metadata


async def main():
    """Main entry point."""
    print("\n")
    print("╔═══════════════════════════════════════════════════════════╗")
    print("║  VAD + LFM2-Audio Speech-to-Speech Pipeline Demo         ║")
    print("╚═══════════════════════════════════════════════════════════╝")
    print("\n")

    # Create demo audio
    demo_audio_file = await create_demo_audio_stream()

    # Initialize nodes
    logger.info("\n🔧 Initializing pipeline nodes...")

    # VAD node (simplified for demo)
    vad_node = VoiceActivityDetector(
        frame_duration_ms=30,
        energy_threshold=0.02,
        speech_threshold=0.3,
        filter_mode=False,
        include_metadata=True,
    )

    # VAD-triggered buffer
    vad_buffer = VADTriggeredBufferNode(
        min_speech_duration_s=1.0,
        max_speech_duration_s=10.0,
        silence_duration_s=1.5,
        sample_rate=24000,
    )

    # LFM2-Audio node
    logger.info("Initializing LFM2-Audio node...")
    lfm2_audio = LFM2AudioNode(
        node_id="lfm2_audio_vad",
        system_prompt="You are a helpful AI assistant. Respond naturally to user questions.",
        device="cpu",  # Use "cuda" if available
        audio_temperature=1.0,
        audio_top_k=4,
        max_new_tokens=512,
        sample_rate=24000,
    )

    await lfm2_audio.initialize()

    # Process pipeline
    logger.info("\n🎙️ Starting VAD-based speech-to-speech pipeline...")
    session_id = "vad_demo_session"

    # Create audio stream
    audio_stream = audio_chunk_generator(demo_audio_file, chunk_size_ms=100)

    # VAD processing
    vad_stream = vad_process_wrapper(vad_node, audio_stream)

    # VAD buffering
    buffered_stream = vad_buffer.process(vad_stream)

    # Process through LFM2-Audio
    response_count = 0
    async for speech_segment in buffered_stream:
        response_count += 1
        logger.info(f"\n🗣️ Processing speech segment {response_count}...")

        text_responses = []
        audio_responses = []

        async for response in lfm2_audio.process(speech_segment, session_id=session_id):
            if response.is_text():
                text = response.as_text()
                text_responses.append(text)
                logger.info(f"📝 AI: {text}")

            elif response.is_audio():
                audio = audio_to_numpy(response)
                audio_responses.append(audio)
                duration_s = len(audio) / 24000
                logger.info(f"🔊 Audio: {len(audio)} samples ({duration_s:.2f}s)")

        # Save response audio
        if audio_responses:
            full_audio = np.concatenate(audio_responses)
            output_file = f"vad_response_{response_count}.wav"
            sf.write(output_file, full_audio, 24000)
            logger.info(f"💾 Saved: {output_file}")

    # Show session info
    logger.info("\n📊 Final Session Info:")
    session_info = lfm2_audio.get_session_info(session_id)
    if session_info:
        for key, value in session_info.items():
            logger.info(f"  {key}: {value}")

    # Cleanup
    await lfm2_audio.cleanup()

    print("\n" + "=" * 60)
    print(f"✅ Pipeline complete! Processed {response_count} speech segments")
    print("=" * 60)


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("\n🛑 Interrupted by user")
    except Exception as e:
        logger.error(f"❌ Error: {e}", exc_info=True)
        sys.exit(1)
