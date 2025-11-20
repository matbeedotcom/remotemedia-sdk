#!/usr/bin/env python3
"""
LFM2-Audio Speech-to-Speech Example

This example demonstrates conversational AI using the LFM2-Audio-1.5B model.
The model accepts audio input and generates both text and audio responses,
enabling natural voice conversations without intermediate text transcription.

Features:
- Audio-to-audio conversation
- Multi-turn conversation history
- Session-based context management
- Interleaved text and audio generation

**TO RUN THIS EXAMPLE:**

1. **Install dependencies:**
   ```bash
   pip install liquid-audio torch torchaudio soundfile
   ```

2. **Prepare test audio:**
   - Record a question as a WAV file (e.g., "question.wav")
   - Or use the provided example

3. **Run the script:**
   ```bash
   python examples/audio_examples/lfm2_audio_s2s.py
   ```

**Model Info:**
- Model: Liquid AI LFM2-Audio-1.5B
- HuggingFace: LiquidAI/LFM2-Audio-1.5B
- Sample Rate: 24kHz
- Output: Interleaved text + audio
"""

import asyncio
import logging
import numpy as np
import soundfile as sf
import os
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia.nodes.ml import LFM2AudioNode

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

# Reduce noise from some loggers
logging.getLogger("transformers").setLevel(logging.WARNING)
logging.getLogger("torch").setLevel(logging.WARNING)


async def create_demo_audio(filepath: str = "demo_question.wav", duration_s: float = 3.0):
    """Create a demo audio file with synthesized speech-like sounds."""
    if os.path.exists(filepath):
        logger.info(f"Demo audio already exists: {filepath}")
        return

    logger.info(f"Creating demo audio: {filepath}")

    sample_rate = 24000
    samples = int(sample_rate * duration_s)
    t = np.linspace(0, duration_s, samples, dtype=np.float32)

    # Create speech-like audio with varying frequencies
    freq_base = 200 + 100 * np.sin(2 * np.pi * 0.5 * t)
    freq_mod = 80 * np.sin(2 * np.pi * 4 * t)
    audio = 0.3 * np.sin(2 * np.pi * (freq_base + freq_mod) * t)

    # Add some noise
    audio += 0.01 * np.random.randn(samples).astype(np.float32)

    # Save
    sf.write(filepath, audio, sample_rate)
    logger.info(f"Created demo audio: {duration_s}s @ {sample_rate}Hz")


async def run_single_turn_conversation():
    """
    Run a single-turn speech-to-speech conversation.
    """
    logger.info("=" * 60)
    logger.info("LFM2-Audio Speech-to-Speech: Single Turn")
    logger.info("=" * 60)

    # Create demo audio if needed
    test_audio_file = "demo_question.wav"
    await create_demo_audio(test_audio_file)

    # Load test audio
    logger.info(f"Loading test audio: {test_audio_file}")
    audio_data, sample_rate = sf.read(test_audio_file, dtype='float32')
    logger.info(f"Loaded audio: {len(audio_data)} samples @ {sample_rate}Hz")

    # Create LFM2-Audio node
    logger.info("Initializing LFM2-Audio node...")
    s2s_node = LFM2AudioNode(
        node_id="lfm2_audio_1",
        system_prompt="You are a helpful AI assistant. Respond naturally to questions.",
        device="cpu",  # Use "cuda" if GPU available
        audio_temperature=1.0,
        audio_top_k=4,
        max_new_tokens=512,
        sample_rate=24000,
        session_timeout_minutes=30,
    )

    # Initialize node
    await s2s_node.initialize()
    logger.info("Node initialized successfully")

    # Convert audio to RuntimeData
    input_audio = numpy_to_audio(audio_data, sample_rate, channels=1)

    # Process
    logger.info("\n🎙️ Processing audio question...")
    session_id = "demo_session_1"

    text_responses = []
    audio_responses = []

    async for response in s2s_node.process(input_audio, session_id=session_id):
        if response.is_text():
            text = response.as_text()
            text_responses.append(text)
            logger.info(f"📝 Text response: {text}")

        elif response.is_audio():
            audio = audio_to_numpy(response)
            audio_responses.append(audio)
            logger.info(f"🔊 Audio response: {len(audio)} samples ({len(audio) / 24000:.2f}s)")

    # Save output audio
    if audio_responses:
        full_audio = np.concatenate(audio_responses)
        output_file = f"response_{session_id}.wav"
        sf.write(output_file, full_audio, 24000)
        logger.info(f"💾 Saved audio response to: {output_file}")

    # Show session info
    logger.info("\n📊 Session Info:")
    session_info = s2s_node.get_session_info(session_id)
    if session_info:
        for key, value in session_info.items():
            logger.info(f"  {key}: {value}")

    # Cleanup
    await s2s_node.cleanup()
    logger.info("\n✅ Single-turn conversation complete!")


async def run_multi_turn_conversation():
    """
    Run a multi-turn speech-to-speech conversation.
    Demonstrates conversation history management.
    """
    logger.info("\n" + "=" * 60)
    logger.info("LFM2-Audio Speech-to-Speech: Multi-Turn Conversation")
    logger.info("=" * 60)

    # Create demo audio files for multiple turns
    turn1_file = "turn1_question.wav"
    turn2_file = "turn2_question.wav"

    await create_demo_audio(turn1_file, duration_s=2.0)
    await create_demo_audio(turn2_file, duration_s=2.5)

    # Create LFM2-Audio node
    logger.info("Initializing LFM2-Audio node...")
    s2s_node = LFM2AudioNode(
        node_id="lfm2_audio_multi",
        system_prompt="You are a helpful AI assistant having a conversation. Maintain context across turns.",
        device="cpu",
        audio_temperature=1.0,
        audio_top_k=4,
        max_new_tokens=512,
        sample_rate=24000,
    )

    await s2s_node.initialize()
    session_id = "multi_turn_session"

    # Turn 1
    logger.info("\n🗣️ Turn 1: User asks first question")
    audio1, sr1 = sf.read(turn1_file, dtype='float32')
    input1 = numpy_to_audio(audio1, sr1, channels=1)

    response1_text = []
    response1_audio = []

    async for response in s2s_node.process(input1, session_id=session_id):
        if response.is_text():
            text = response.as_text()
            response1_text.append(text)
            logger.info(f"  📝 Text: {text}")
        elif response.is_audio():
            audio = audio_to_numpy(response)
            response1_audio.append(audio)

    if response1_audio:
        full_audio1 = np.concatenate(response1_audio)
        sf.write(f"response_turn1.wav", full_audio1, 24000)
        logger.info(f"  💾 Saved: response_turn1.wav")

    # Turn 2
    logger.info("\n🗣️ Turn 2: User asks follow-up question")
    audio2, sr2 = sf.read(turn2_file, dtype='float32')
    input2 = numpy_to_audio(audio2, sr2, channels=1)

    response2_text = []
    response2_audio = []

    async for response in s2s_node.process(input2, session_id=session_id):
        if response.is_text():
            text = response.as_text()
            response2_text.append(text)
            logger.info(f"  📝 Text: {text}")
        elif response.is_audio():
            audio = audio_to_numpy(response)
            response2_audio.append(audio)

    if response2_audio:
        full_audio2 = np.concatenate(response2_audio)
        sf.write(f"response_turn2.wav", full_audio2, 24000)
        logger.info(f"  💾 Saved: response_turn2.wav")

    # Show final session info
    logger.info("\n📊 Final Session State:")
    session_info = s2s_node.get_session_info(session_id)
    if session_info:
        for key, value in session_info.items():
            logger.info(f"  {key}: {value}")

    # Cleanup
    await s2s_node.cleanup()
    logger.info("\n✅ Multi-turn conversation complete!")


async def main():
    """Main entry point."""
    print("\n")
    print("╔═══════════════════════════════════════════════════════════╗")
    print("║   LFM2-Audio Speech-to-Speech Demonstration              ║")
    print("║   Model: LiquidAI/LFM2-Audio-1.5B                        ║")
    print("╚═══════════════════════════════════════════════════════════╝")
    print("\n")

    try:
        # Run single-turn conversation
        await run_single_turn_conversation()

        # Run multi-turn conversation
        await run_multi_turn_conversation()

        print("\n" + "=" * 60)
        print("🎉 All demonstrations completed successfully!")
        print("=" * 60)

    except Exception as e:
        logger.error(f"Error during demonstration: {e}", exc_info=True)
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
