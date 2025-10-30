"""
LFM2-Audio-1.5B Node using RuntimeData API

Speech-to-speech conversational AI node that uses Liquid AI's LFM2-Audio model
for interleaved text and audio generation.

Key features:
- Direct audio-to-audio conversation without intermediate text transcription
- Maintains conversation history across multiple turns
- Supports both text and audio outputs
- Uses RuntimeData API for efficient communication with Rust runtime
- Session-based conversation management
"""

import logging
import numpy as np
import torch
import torchaudio
from typing import AsyncGenerator, Optional, Dict, List, Any, TYPE_CHECKING
import asyncio
from dataclasses import dataclass, field
from datetime import datetime

# Suppress torch dynamo compilation errors (fall back to eager mode)
try:
    import torch._dynamo
    torch._dynamo.config.suppress_errors = True
except (ImportError, AttributeError):
    pass

# Import RuntimeData bindings
if TYPE_CHECKING:
    from remotemedia_runtime.runtime_data import RuntimeData

try:
    from remotemedia_runtime.runtime_data import (
        RuntimeData,
        numpy_to_audio,
        audio_to_numpy,
    )
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


@dataclass
class ConversationState:
    """Manages conversation history for a session."""
    session_id: str
    chat_state: Any  # ChatState from liquid_audio
    created_at: datetime = field(default_factory=datetime.now)
    last_accessed: datetime = field(default_factory=datetime.now)
    turn_count: int = 0

    def update_access(self):
        """Update last accessed timestamp."""
        self.last_accessed = datetime.now()


class LFM2AudioNode:
    """
    Speech-to-speech conversation node using LFM2-Audio-1.5B.

    This node accepts audio via RuntimeData.Audio and yields both text and audio
    responses, enabling natural conversational AI without intermediate transcription.

    The model generates interleaved text and audio tokens, providing both textual
    responses and corresponding speech audio.
    """

    def __init__(
        self,
        node_id: str,
        hf_repo: str = "LiquidAI/LFM2-Audio-1.5B",
        system_prompt: str = "Respond with interleaved text and audio.",
        device: Optional[str] = None,
        audio_temperature: float = 1.0,
        audio_top_k: int = 4,
        max_new_tokens: int = 512,
        sample_rate: int = 24000,
        session_timeout_minutes: int = 30,
        text_only: bool = False,
        **kwargs
    ):
        """
        Initialize LFM2-Audio node with RuntimeData support.

        Args:
            node_id: Unique identifier for this node instance
            hf_repo: HuggingFace repository for the model (default: LiquidAI/LFM2-Audio-1.5B)
            system_prompt: System prompt for the conversation
            device: Device to run the model on (cuda/cpu, auto-detected if None)
            audio_temperature: Temperature for audio generation (default: 1.0)
            text_only: If True, only output text (no audio synthesis)
            audio_top_k: Top-k sampling for audio tokens (default: 4)
            max_new_tokens: Maximum number of tokens to generate (default: 512)
            sample_rate: Output sample rate (default: 24000)
            session_timeout_minutes: Minutes before session expires (default: 30)
        """
        self.node_id = node_id
        self.hf_repo = hf_repo
        self.system_prompt = system_prompt
        self.audio_temperature = audio_temperature
        self.audio_top_k = audio_top_k
        self.max_new_tokens = max_new_tokens
        self.sample_rate = sample_rate
        self.session_timeout_minutes = session_timeout_minutes
        self.text_only = text_only
        self.is_streaming = True

        logger.info(f"LFM2AudioNode initialized: text_only={self.text_only}, system_prompt='{self.system_prompt}'")

        # Auto-detect device if not specified
        if device is None:
            if torch.cuda.is_available():
                self.device = "cuda"
            else:
                self.device = "cpu"
        else:
            self.device = device

        self._processor = None
        self._model = None
        self._initialized = False

        # Session management
        self._sessions: Dict[str, ConversationState] = {}
        self._cleanup_task = None

    async def initialize(self) -> None:
        """Initialize the LFM2-Audio model and processor."""
        if self._initialized:
            return

        try:
            # Import liquid_audio here to avoid import errors if not installed
            from liquid_audio import LFM2AudioModel, LFM2AudioProcessor

            logger.info(f"Initializing LFM2-Audio from '{self.hf_repo}' on device '{self.device}'")

            # Load processor and model
            # Run in thread to avoid blocking
            # Note: LFM2AudioProcessor uses "cuda" by default, so we need to override it
            def load_processor():
                # Patch the default device in liquid_audio if using CPU
                import os
                # Force CPU-only mode if CUDA isn't available
                if self.device == "cpu" and not torch.cuda.is_available():
                    # Set environment variable to disable CUDA
                    os.environ["CUDA_VISIBLE_DEVICES"] = ""

                # liquid_audio's from_pretrained() accepts a device parameter
                try:
                    processor = LFM2AudioProcessor.from_pretrained(
                        self.hf_repo,
                        device=self.device
                    )
                except TypeError:
                    # Fallback if device parameter not supported
                    processor = LFM2AudioProcessor.from_pretrained(self.hf_repo)
                    if self.device == "cpu":
                        processor = processor.to("cpu")
                return processor

            self._processor = await asyncio.to_thread(load_processor)
            self._processor = self._processor.eval()

            # Load model with explicit attn_implementation to avoid flash_attn issues
            def load_model():
                try:
                    # Try to use SDPA (scaled_dot_product_attention) instead of flash_attn
                    model = LFM2AudioModel.from_pretrained(
                        self.hf_repo,
                        attn_implementation="sdpa"
                    )
                except (TypeError, ValueError):
                    # Fallback if parameter not supported
                    model = LFM2AudioModel.from_pretrained(self.hf_repo)
                return model

            self._model = await asyncio.to_thread(load_model)
            self._model = self._model.eval()

            # Move to device
            if self.device == "cuda":
                self._model = self._model.cuda()
            else:
                self._model = self._model.to("cpu")

            self._initialized = True
            logger.info("LFM2-Audio model initialized successfully")

            # Start session cleanup task
            self._cleanup_task = asyncio.create_task(self._cleanup_expired_sessions())

        except ImportError as e:
            raise ImportError(
                "liquid_audio is not installed. Install with: pip install liquid-audio"
            ) from e
        except Exception as e:
            logger.error(f"Failed to initialize LFM2-Audio: {e}")
            raise

    async def cleanup(self) -> None:
        """Clean up the model and sessions."""
        if self._cleanup_task:
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass

        if self._model is not None:
            self._model = None
            self._processor = None
            self._initialized = False
            self._sessions.clear()
            logger.info("LFM2-Audio model cleaned up")

    async def _cleanup_expired_sessions(self):
        """Background task to clean up expired sessions."""
        while True:
            try:
                await asyncio.sleep(60)  # Check every minute
                now = datetime.now()
                expired_sessions = []

                for session_id, state in self._sessions.items():
                    elapsed = (now - state.last_accessed).total_seconds() / 60
                    if elapsed > self.session_timeout_minutes:
                        expired_sessions.append(session_id)

                for session_id in expired_sessions:
                    logger.info(f"Removing expired session: {session_id}")
                    del self._sessions[session_id]

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Error in session cleanup: {e}")

    def _get_or_create_session(self, session_id: str) -> ConversationState:
        """Get existing session or create a new one."""
        from liquid_audio import ChatState

        if session_id not in self._sessions:
            logger.info(f"Creating new conversation session: {session_id}")
            chat = ChatState(self._processor)

            # Add system prompt
            chat.new_turn("system")
            chat.add_text(self.system_prompt)
            chat.end_turn()

            self._sessions[session_id] = ConversationState(
                session_id=session_id,
                chat_state=chat
            )
        else:
            self._sessions[session_id].update_access()

        return self._sessions[session_id]

    def _generate_sync(
        self,
        chat_state: Any,
        session_state: ConversationState
    ) -> tuple[List[torch.Tensor], List[torch.Tensor], List[Any]]:
        """
        Synchronously generate text and audio tokens (thread-safe, PyTorch isolated).

        Runs PyTorch operations in a thread to avoid heap corruption issues.

        Args:
            chat_state: ChatState object
            session_state: Conversation state

        Returns:
            Tuple of (text_tokens, audio_tokens, modality_flags)
        """
        from liquid_audio import LFMModality

        logger.info(f"Generating response for session {session_state.session_id} (turn {session_state.turn_count})")

        text_out: List[torch.Tensor] = []
        audio_out: List[torch.Tensor] = []
        modality_out: List[Any] = []

        # Use generate_sequential for text-only mode (ASR)
        # Use generate_interleaved for conversational mode with audio
        if self.text_only:
            logger.info("Using generate_sequential for text-only mode")
            for t in self._model.generate_sequential(
                **chat_state,
                max_new_tokens=self.max_new_tokens,
                text_temperature=None,  # Use defaults
                text_top_k=None
            ):
                if t.numel() == 1:
                    # Text token
                    text_out.append(t)
                    modality_out.append(LFMModality.TEXT)
                else:
                    # Audio token (shouldn't happen in sequential mode, but handle it)
                    audio_out.append(t)
                    modality_out.append(LFMModality.AUDIO_OUT)
        else:
            logger.info("Using generate_interleaved for full S2S mode")
            for t in self._model.generate_interleaved(
                **chat_state,
                max_new_tokens=self.max_new_tokens,
                audio_temperature=self.audio_temperature,
                audio_top_k=self.audio_top_k
            ):
                if t.numel() == 1:
                    # Text token
                    text_out.append(t)
                    modality_out.append(LFMModality.TEXT)
                else:
                    # Audio token
                    audio_out.append(t)
                    modality_out.append(LFMModality.AUDIO_OUT)

        return text_out, audio_out, modality_out

    async def process(
        self,
        data: RuntimeData
    ) -> AsyncGenerator[RuntimeData, None]:
        """
        Process audio input and generate speech+text response.

        Args:
            data: RuntimeData containing audio to process (RuntimeData.Audio)
                  Metadata can include:
                  - "session_id": str - Session identifier for conversation history (default: "default")
                  - "reset": bool - If True, resets the conversation session

        Yields:
            RuntimeData.Text for text responses
            RuntimeData.Audio for audio responses

        Raises:
            ValueError: If input is not RuntimeData.Audio
            RuntimeError: If generation fails
        """
        # CRITICAL: Extract all data from RuntimeData BEFORE any await/yield
        # PyO3 RuntimeData objects can't be accessed after async suspension points
        logger.info("LFM2-Audio Node processing input data")

        # Validate input type
        if not data.is_audio():
            logger.error(f"Invalid input type: expected RuntimeData.Audio, got {data.data_type()}")
            raise ValueError(
                f"LFM2AudioNode expects RuntimeData.Audio input, got {data.data_type()}"
            )
        logger.info("Input is valid RuntimeData.Audio")

        # Extract audio from RuntimeData using as_audio() method instead of audio_to_numpy()
        # to avoid PyO3 FFI issues in async context
        samples_bytes, sample_rate, channels, format_str, num_samples = data.as_audio()
        # Convert bytes to numpy array
        audio_array = np.frombuffer(samples_bytes, dtype=np.float32)
        logger.info(f"Received audio: {len(audio_array)} samples, {sample_rate}Hz, {channels} channels")

        # Extract session_id from RuntimeData
        session_id = data.session_id if hasattr(data, 'session_id') and data.session_id else "default"
        logger.info(f"Using session_id from RuntimeData: {session_id}")
        metadata = {}  # Reserved for future use

        # Handle metadata commands
        if metadata:
            if metadata.get("reset"):
                if session_id in self._sessions:
                    logger.info(f"Resetting session: {session_id}")
                    del self._sessions[session_id]

                logger.info(f"Session {session_id} has been reset.")
                return
        logger.info(f"Using session ID: {session_id}")

        # Get or create session
        session_state = self._get_or_create_session(session_id)
        chat = session_state.chat_state

        # Start new user turn
        chat.new_turn("user")

        # Convert numpy array to torch tensor and add to chat
        # LFM2-Audio expects audio as (channels, samples) tensor at 24kHz
        wav = torch.from_numpy(audio_array).float()
        if wav.dim() == 1:
            wav = wav.unsqueeze(0)  # Add channel dimension

        chat.add_audio(wav, self.sample_rate)
        chat.end_turn()

        # Start assistant turn
        chat.new_turn("assistant")
        session_state.turn_count += 1

        try:
            # Generate response in thread (PyTorch-safe)
            text_tokens, audio_tokens, modality_flags = await asyncio.to_thread(
                self._generate_sync, chat, session_state
            )

            # Stream text tokens
            if text_tokens:
                full_text = ""
                for token in text_tokens:
                    decoded = self._processor.text.decode(token)
                    full_text += decoded

                logger.info(f"Generated text: {full_text}")
                yield RuntimeData.text(full_text)

            # Detokenize and stream audio
            if audio_tokens and len(audio_tokens) > 1:
                # Remove the last "end-of-audio" code
                mimi_codes = torch.stack(audio_tokens[:-1], 1).unsqueeze(0)

                with torch.no_grad():
                    waveform = self._processor.mimi.decode(mimi_codes)[0]

                # Convert to numpy and yield as RuntimeData
                audio_np = waveform.cpu().numpy()

                # Mimi returns audio at 24kHz
                # If multi-channel, take first channel
                if audio_np.ndim == 2:
                    audio_np = audio_np[0]

                logger.info(f"Generated audio: {len(audio_np)} samples ({len(audio_np) / 24000:.2f}s)")

                # Convert to RuntimeData.Audio
                audio_runtime_data = numpy_to_audio(audio_np, 24000, channels=1)
                logger.info(f"Yielding audio RuntimeData with {len(audio_np)} samples")
                yield audio_runtime_data

            # Append to chat history
            if text_tokens or audio_tokens:
                from liquid_audio import LFMModality
                chat.append(
                    text=torch.stack(text_tokens, 1) if text_tokens else None,
                    audio_out=torch.stack(audio_tokens, 1) if audio_tokens else None,
                    modality_flag=torch.tensor(modality_flags) if modality_flags else None,
                )
            chat.end_turn()

        except Exception as e:
            logger.error(f"Error during LFM2-Audio generation: {e}")
            raise RuntimeError(f"Speech-to-speech generation failed: {e}") from e

    def get_config(self) -> dict:
        """Get node configuration."""
        return {
            "node_id": self.node_id,
            "node_type": "LFM2AudioNode",
            "hf_repo": self.hf_repo,
            "system_prompt": self.system_prompt,
            "device": self.device,
            "audio_temperature": self.audio_temperature,
            "audio_top_k": self.audio_top_k,
            "max_new_tokens": self.max_new_tokens,
            "sample_rate": self.sample_rate,
            "session_timeout_minutes": self.session_timeout_minutes,
            "active_sessions": len(self._sessions),
        }

    def get_session_info(self, session_id: str) -> Optional[Dict[str, Any]]:
        """Get information about a specific session."""
        if session_id not in self._sessions:
            return None

        state = self._sessions[session_id]
        return {
            "session_id": state.session_id,
            "turn_count": state.turn_count,
            "created_at": state.created_at.isoformat(),
            "last_accessed": state.last_accessed.isoformat(),
        }

    def list_sessions(self) -> List[Dict[str, Any]]:
        """List all active sessions."""
        return [self.get_session_info(sid) for sid in self._sessions.keys()]


# Example usage
async def main():
    """
    Example demonstrating LFM2AudioNode with RuntimeData API
    """
    if not RUNTIME_DATA_AVAILABLE:
        print("RuntimeData bindings not available. Please build the Rust extension.")
        print("Run: cargo build --release")
        return

    print("=" * 60)
    print("LFM2-Audio Node with RuntimeData API")
    print("=" * 60)

    # Create speech-to-speech node
    s2s_node = LFM2AudioNode(
        node_id="lfm2_audio_1",
        system_prompt="You are a helpful AI assistant. Respond naturally to questions.",
        device="cpu",  # Use "cuda" if available
        audio_temperature=1.0,
        audio_top_k=4,
        max_new_tokens=512,
        sample_rate=24000,
    )

    # Initialize
    await s2s_node.initialize()

    # Load test audio
    # You would need to provide your own test audio file
    test_audio_path = "test_question.wav"
    if not os.path.exists(test_audio_path):
        print(f"Test audio file not found: {test_audio_path}")
        print("Please provide a test audio file with a question.")
        await s2s_node.cleanup()
        return

    import soundfile as sf
    audio_data, sr = sf.read(test_audio_path)

    # Resample if necessary
    if sr != s2s_node.sample_rate:
        import resampy
        audio_data = resampy.resample(audio_data, sr, s2s_node.sample_rate)

    # Create RuntimeData input
    input_audio = numpy_to_audio(audio_data.astype(np.float32), s2s_node.sample_rate, channels=1)

    # Process
    print("\nProcessing audio question...")
    session_id = "test_session_1"

    async for response in s2s_node.process(input_audio, session_id=session_id):
        if response.is_text():
            text = response.as_text()
            print(f"\nText response: {text}")
        elif response.is_audio():
            audio = audio_to_numpy(response)
            print(f"Audio response: {len(audio)} samples ({len(audio) / 24000:.2f}s)")
            # Save audio
            sf.write(f"response_{session_id}.wav", audio, 24000)
            print(f"Saved to response_{session_id}.wav")

    # Show session info
    print("\nSession info:")
    print(s2s_node.get_session_info(session_id))

    # Cleanup
    await s2s_node.cleanup()

    print("\n" + "=" * 60)


if __name__ == "__main__":
    import os
    asyncio.run(main())
