"""
LFM2-Audio-1.5B Node using RuntimeData API and MultiprocessNode base

Speech-to-speech conversational AI node that uses Liquid AI's LFM2-Audio model
for interleaved text and audio generation, with multiprocess execution support.

Inherits from MultiprocessNode to enable concurrent execution in separate processes.

Key features:
- Multiprocess execution support (concurrent with other AI models)
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
from typing import AsyncGenerator, Optional, Dict, List, Any, TYPE_CHECKING, Union
from liquid_audio import ChatState, LFMModality
# Import liquid_audio here to avoid import errors if not installed
from liquid_audio import LFM2AudioModel, LFM2AudioProcessor
import asyncio
from dataclasses import dataclass, field
from datetime import datetime
try:
    import resampy
except ImportError:
    resampy = None
    logging.warning("resampy not installed. Audio resampling will use torchaudio instead.")

# Suppress torch dynamo compilation errors (fall back to eager mode)
try:
    import torch._dynamo
    torch._dynamo.config.suppress_errors = True
except (ImportError, AttributeError):
    pass

# Import RuntimeData bindings
if TYPE_CHECKING:
    from remotemedia.core.multiprocessing.data import RuntimeData

try:
    from remotemedia.core.multiprocessing.data import (
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
    logging.warning("[LFM2AudioNode] RuntimeData bindings not available. Using fallback implementation.")

# Import MultiprocessNode base class from core
from remotemedia.core import MultiprocessNode, NodeConfig

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


class LFM2AudioNode(MultiprocessNode):
    """
    Speech-to-speech conversation node using LFM2-Audio-1.5B with multiprocess support.

    Inherits from MultiprocessNode to enable concurrent execution in separate processes.
    This node accepts audio via RuntimeData.Audio and yields both text and audio
    responses, enabling natural conversational AI without intermediate transcription.

    The model generates interleaved text and audio tokens, providing both textual
    responses and corresponding speech audio.
    """

    def __init__(
        self,
        node_id: str = None,
        hf_repo: str = "LiquidAI/LFM2-Audio-1.5B",
        system_prompt: str = "Respond with interleaved text and audio.",
        device: Optional[str] = None,
        audio_temperature: float = 1.0,
        audio_top_k: int = 4,
        max_new_tokens: int = 4096,
        sample_rate: int = 24000,
        session_timeout_minutes: int = 30,
        text_only: bool = False,
        config: Union[NodeConfig, Dict[str, Any]] = None,
        **kwargs
    ):
        """
        Initialize LFM2-Audio node with RuntimeData and multiprocess support.

        Args:
            node_id: Unique identifier for this node instance
            hf_repo: HuggingFace repository for the model
            system_prompt: System prompt for the conversation
            device: Device to run the model on (cuda/cpu, auto-detected if None)
            audio_temperature: Temperature for audio generation
            audio_top_k: Top-k sampling for audio tokens
            max_new_tokens: Maximum number of tokens to generate
            sample_rate: Output sample rate
            session_timeout_minutes: Minutes before session expires
            text_only: If True, only output text (no audio synthesis)
            config: NodeConfig for multiprocess mode (if available)
            **kwargs: Additional parameters
        """
        # Initialize MultiprocessNode base if config provided
        if config is not None:
            super().__init__(config, **kwargs)
            # Extract params from config
            if isinstance(config, NodeConfig):
                params = config.params
            else:
                params = config.get('params', {})

            # Override with params from config
            hf_repo = params.get('hf_repo', hf_repo)
            system_prompt = params.get('system_prompt', system_prompt)
            device = params.get('device', device)
            audio_temperature = params.get('audio_temperature', audio_temperature)
            audio_top_k = params.get('audio_top_k', audio_top_k)
            max_new_tokens = params.get('max_new_tokens', max_new_tokens)
            sample_rate = params.get('sample_rate', sample_rate)
            session_timeout_minutes = params.get('session_timeout_minutes', session_timeout_minutes)
            text_only = params.get('text_only', text_only)
        else:
            # Standalone mode without multiprocess
            self.node_id = node_id or "lfm2_audio"
            self.node_type = "LFM2AudioNode"
            self.logger = logging.getLogger(__name__)

        # LFM2-specific configuration
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
            logger.info(f"Initializing LFM2-Audio from '{self.hf_repo}' on device '{self.device}'")

            # Load processor directly (no thread isolation)
            import os
            if self.device == "cpu" and not torch.cuda.is_available():
                os.environ["CUDA_VISIBLE_DEVICES"] = ""

            try:
                self._processor = LFM2AudioProcessor.from_pretrained(
                    self.hf_repo,
                    device=self.device
                )
            except TypeError:
                self._processor = LFM2AudioProcessor.from_pretrained(self.hf_repo)
                if self.device == "cpu":
                    self._processor = self._processor.to("cpu")

            self._processor = self._processor.eval()

            # Load model directly
            try:
                self._model = LFM2AudioModel.from_pretrained(
                    self.hf_repo,
                    attn_implementation="flash_attn"
                )
            except (TypeError, ValueError):
                self._model = LFM2AudioModel.from_pretrained(self.hf_repo)

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

    async def _get_or_create_session(self, session_id: str) -> ConversationState:
        """Get existing session or create a new one."""
        if session_id not in self._sessions:
            logger.info(f"Creating new conversation session: {session_id}")

            if self._processor is None:
                logger.error("ERROR: Processor not initialized. Call initialize() first.")
                raise RuntimeError("Processor not initialized")

            # Create ChatState directly (no thread isolation)
            chat = ChatState(self._processor)

            # Add system prompt
            chat.new_turn("system")
            chat.add_text(self.system_prompt)
            chat.end_turn()

            self._sessions[session_id] = ConversationState(
                session_id=session_id,
                chat_state=chat
            )
            logger.info(f"Session {session_id} created and stored successfully")
        else:
            logger.debug(f"Using existing session: {session_id}")
            self._sessions[session_id].update_access()

        return self._sessions[session_id]

    async def process(
        self,
        data: RuntimeData
    ) -> Union[RuntimeData, AsyncGenerator[RuntimeData, None], None]:
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
        try:
            # CRITICAL: Extract all data from RuntimeData BEFORE any await/yield
            # PyO3 RuntimeData objects can't be accessed after async suspension points
            logger.info("LFM2-Audio Node processing input data")
            # Validate input type
            if not data.is_audio():
                logger.error(f"Invalid input type: expected RuntimeData.Audio, got {data.data_type()}")
                # Yield an error message instead of raising
                yield RuntimeData.text(f"ERROR: Expected audio input, got {data.data_type()}")
                return
            logger.debug("Input is valid RuntimeData.Audio")

            # Extract audio from RuntimeData using as_audio() method instead of audio_to_numpy()
            # to avoid PyO3 FFI issues in async context
            samples_bytes, input_sample_rate, channels, format_str, num_samples = data.as_audio()
            
            audio_array_echo = np.frombuffer(samples_bytes, dtype=np.float32)
            audio_runtime_data_echo = numpy_to_audio(audio_array_echo, self.sample_rate, channels=1)
            yield audio_runtime_data_echo
            # Convert bytes to numpy array
            audio_array = np.frombuffer(samples_bytes, dtype=np.float32)

            logger.debug(f"Received audio: {len(audio_array)} samples, {input_sample_rate}Hz, {channels} channels")

            # Resample audio if necessary (model expects 24kHz)
            if input_sample_rate != self.sample_rate:
                raise ValueError(f"Input audio sample rate {input_sample_rate} does not match expected {self.sample_rate}")

            # Extract session_id from RuntimeData
            session_id = data.session_id if hasattr(data, 'session_id') and data.session_id else "default"
            logger.debug(f"Using session_id from RuntimeData: {session_id}")
            metadata = {}  # Reserved for future use

            # Handle metadata commands
            if metadata:
                if metadata.get("reset"):
                    if session_id in self._sessions:
                        logger.info(f"Resetting session: {session_id}")
                        del self._sessions[session_id]

                    logger.info(f"Session {session_id} has been reset.")
                    return
            logger.debug(f"Using session ID: {session_id}")

            # Get or create session
            logger.debug(f"Retrieving or creating session: {session_id}")
            session_state = await self._get_or_create_session(session_id)
            logger.debug(f"Retrieved session state for session_id: {session_id}")
            logger.debug(f"Session turn count: {session_state.turn_count}")

            logger.debug("Getting chat_state from session...")
            chat = session_state.chat_state
            logger.debug("Successfully got chat_state")

            # Start new user turn directly (no thread isolation)
            logger.debug("Starting new user turn...")
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
            logger.debug("User and assistant turns added successfully")

            logger.info(f"Starting streaming generation for session {session_id}, turn {session_state.turn_count}")

            # Stream tokens as they are generated - simplified
            from liquid_audio import LFMModality
            text_tokens_for_history = []
            audio_tokens_for_history = []
            modality_flags_for_history = []

            # Create generator directly (no thread isolation)
            logger.debug("Creating token generator...")
            if self.text_only:
                logger.debug("Generating in text-only mode")
                token_generator = self._model.generate_sequential(
                    **chat,
                    max_new_tokens=self.max_new_tokens,
                    text_temperature=None,
                    text_top_k=None
                )
            else:
                logger.debug(f"Generating in full S2S mode with max_new_tokens={self.max_new_tokens}")
                token_generator = self._model.generate_interleaved(
                    **chat,
                    max_new_tokens=self.max_new_tokens,
                    audio_temperature=self.audio_temperature,
                    audio_top_k=self.audio_top_k
                )

            # Process and yield tokens as they're generated
            logger.debug("Starting to stream tokens...")
            token_idx = 0
            audio_token_batch = []  # Batch audio tokens for decoding
            audio_batch_size = 5  # Decode 5 audio tokens at once for stability

            while True:
                # Get next token from generator directly
                try:
                    token = next(token_generator)
                except StopIteration:
                    logger.info(f"Token generation complete: {token_idx} tokens processed")
                    break

                token_idx += 1

                # Yield control to event loop periodically
                if token_idx % 10 == 0:
                    await asyncio.sleep(0)

                # Get token info for logging (safe to do on CPU-side metadata)
                token_numel = token.numel()
                logger.debug(f"Processing token {token_idx}, numel: {token_numel}")

                if token_numel == 1:
                    # Text token - flush any pending audio tokens FIRST before yielding text
                    # This ensures audio tokens are always consecutive in time
                    if audio_token_batch and len(audio_token_batch) >= 1:
                        try:
                            logger.debug(f"Flushing {len(audio_token_batch)} pending audio tokens before text token")

                            # Decode audio batch directly (no thread isolation)
                            cloned_tokens = [t.clone().detach() for t in audio_token_batch]

                            # Stack tokens: first stack creates [batch_size, codebook_size]
                            # Then transpose to get [codebook_size, batch_size]
                            batch_tensor = torch.stack(cloned_tokens, dim=0).T
                            # Reshape for Mimi: [1, codebook_size, batch_size]
                            mimi_codes = batch_tensor.unsqueeze(0)

                            with torch.no_grad():
                                waveform = self._processor.mimi.decode(mimi_codes)[0]

                            audio_np = waveform.cpu().numpy()
                            logger.debug(f"Decoded audio batch: shape={audio_np.shape}, dtype={audio_np.dtype}")

                            if audio_np.ndim == 2:
                                audio_np = audio_np[0]

                            logger.debug(f"Yielding audio batch: {len(audio_np)} samples")
                            audio_runtime_data = numpy_to_audio(audio_np, self.sample_rate, channels=1)
                            yield audio_runtime_data
                            logger.debug(f"Successfully yielded audio batch")

                            audio_token_batch.clear()

                        except RuntimeError as e:
                            if "CUDA error" in str(e) or "CUBLAS" in str(e):
                                logger.warning(f"CUDA error decoding audio batch, skipping: {str(e)[:100]}")
                                audio_token_batch.clear()
                            else:
                                logger.error(f"Failed to decode audio batch: {e}", exc_info=True)
                                raise
                        except Exception as e:
                            logger.error(f"Unexpected error decoding audio batch: {e}", exc_info=True)
                            audio_token_batch.clear()

                    # Now yield the text token
                    text_tokens_for_history.append(token)
                    modality_flags_for_history.append(LFMModality.TEXT)

                    decoded_text = self._processor.text.decode(token)
                    if decoded_text:  # Only yield non-empty text
                        logger.debug(f"Yielding text token {token_idx}: '{decoded_text}'")
                        yield RuntimeData.text(decoded_text)
                        logger.debug(f"Successfully yielded text token {token_idx}, continuing to next token...")
                        await asyncio.sleep(0)  # Ensure proper async behavior
                    else:
                        logger.debug(f"Skipping empty text token {token_idx}")
                else:
                    # Audio token - batch for decoding
                    audio_tokens_for_history.append(token)
                    modality_flags_for_history.append(LFMModality.AUDIO_OUT)
                    audio_token_batch.append(token)

                    # Decode batch when we reach batch_size
                    should_decode_batch = len(audio_token_batch) >= audio_batch_size

                    if should_decode_batch and audio_token_batch:
                        try:
                            # Decode regular batch (not final) directly (no thread isolation)
                            tokens_to_decode = audio_token_batch
                            logger.debug(f"Decoding audio batch: {len(tokens_to_decode)} tokens")

                            if len(tokens_to_decode) < 1:
                                raise ValueError(f"Cannot decode empty token list")

                            # Clone tokens
                            cloned_tokens = [t.clone().detach() for t in tokens_to_decode]

                            # Stack tokens: first stack creates [batch_size, codebook_size]
                            # Then transpose to get [codebook_size, batch_size]
                            batch_tensor = torch.stack(cloned_tokens, dim=0).T
                            # Reshape for Mimi: [1, codebook_size, batch_size]
                            mimi_codes = batch_tensor.unsqueeze(0)

                            with torch.no_grad():
                                waveform = self._processor.mimi.decode(mimi_codes)[0]

                            audio_np = waveform.cpu().numpy()
                            logger.debug(f"Decoded audio batch: shape={audio_np.shape}, dtype={audio_np.dtype}")

                            if audio_np.ndim == 2:
                                audio_np = audio_np[0]

                            logger.info(f"Yielding audio batch: {len(audio_np)} samples")
                            audio_runtime_data = numpy_to_audio(audio_np, self.sample_rate, channels=1)
                            yield audio_runtime_data
                            logger.debug(f"Successfully yielded audio batch")

                            audio_token_batch.clear()

                        except RuntimeError as e:
                            if "CUDA error" in str(e) or "CUBLAS" in str(e):
                                logger.warning(f"CUDA error decoding audio batch, skipping: {str(e)[:100]}")
                                audio_token_batch.clear()
                                continue
                            else:
                                logger.error(f"Failed to decode audio batch: {e}", exc_info=True)
                                raise
                        except Exception as e:
                            logger.error(f"Unexpected error decoding audio batch: {e}", exc_info=True)
                            audio_token_batch.clear()

            # Handle final audio batch (if any remaining tokens)
            if audio_token_batch:
                try:
                    # The last audio token is an end-of-audio marker and should not be decoded
                    # According to LiquidAI docs: "Detokenize audio, removing the last 'end-of-audio' codes"
                    if len(audio_token_batch) == 1:
                        logger.debug(f"Final batch: Skipping decode - only end-of-audio marker")
                    else:
                        tokens_to_decode = audio_token_batch[:-1]
                        logger.debug(f"Final batch: Decoding {len(tokens_to_decode)} tokens (removing end-of-audio marker)")

                        # Decode directly (no thread isolation)
                        cloned_tokens = [t.clone().detach() for t in tokens_to_decode]

                        batch_tensor = torch.stack(cloned_tokens, dim=0).T
                        mimi_codes = batch_tensor.unsqueeze(0)
                        with torch.no_grad():
                            waveform = self._processor.mimi.decode(mimi_codes)[0]

                        audio_np = waveform.cpu().numpy()
                        logger.debug(f"Decoded final audio batch: shape={audio_np.shape}, dtype={audio_np.dtype}")

                        if audio_np.ndim == 2:
                            audio_np = audio_np[0]

                        logger.debug(f"Yielding final audio batch: {len(audio_np)} samples")
                        audio_runtime_data = numpy_to_audio(audio_np, self.sample_rate, channels=1)
                        yield audio_runtime_data
                        logger.debug(f"Successfully yielded final audio batch")

                    audio_token_batch.clear()

                except RuntimeError as e:
                    if "CUDA error" in str(e) or "CUBLAS" in str(e):
                        logger.warning(f"CUDA error decoding final audio batch: {str(e)[:100]}")
                    else:
                        logger.error(f"Failed to decode final audio batch: {e}", exc_info=True)
                        raise
                except Exception as e:
                    logger.error(f"Unexpected error decoding final audio batch: {e}", exc_info=True)

            logger.info(f"Generation completed: {len(text_tokens_for_history)} text tokens, {len(audio_tokens_for_history)} audio tokens")

            # Emit completion markers
            logger.debug("Generation complete, yielding <|text_end|> and <|audio_end|>")
            yield RuntimeData.text("<|text_end|>")
            yield RuntimeData.text("<|audio_end|>")

            # Append to chat history for context
            try:
                if text_tokens_for_history or audio_tokens_for_history:
                    # Update chat history directly (no thread isolation)
                    logger.debug(f"Preparing {len(text_tokens_for_history)} text tokens and {len(audio_tokens_for_history)} audio tokens for history")

                    # Stack tokens
                    text_stack = None
                    if text_tokens_for_history:
                        logger.debug(f"Stacking text tokens...")
                        text_stack = torch.stack(text_tokens_for_history, 1)
                        logger.debug(f"Stacked text tokens: {text_stack.shape}")

                    audio_stack = None
                    if audio_tokens_for_history:
                        logger.debug(f"Stacking audio tokens...")
                        audio_stack = torch.stack(audio_tokens_for_history, 1)
                        logger.debug(f"Stacked audio tokens: {audio_stack.shape}")

                    # Convert modality flags
                    modality_tensor = None
                    if modality_flags_for_history:
                        modality_values = [int(flag.value) for flag in modality_flags_for_history]
                        modality_tensor = torch.tensor(modality_values, dtype=torch.long).unsqueeze(0)

                    logger.debug("Calling chat.append()...")
                    if self.text_only:
                        # In text-only mode, `ChatState.append` still requires `audio_out` with
                        # length equal to the number of codebooks. Provide an empty sequence
                        # with shape [codebooks, 0] to represent no audio tokens.
                        codebooks = (
                            getattr(self._processor, "codebooks", None)
                            or getattr(getattr(self._processor, "mimi", None), "codebooks", None)
                        )
                        if codebooks is None:
                            # Fallback: infer from audio_stack if available, else default to 8
                            codebooks = int(audio_stack.size(0)) if isinstance(audio_stack, torch.Tensor) else 8

                        # Use a LongTensor to match token id dtype expectations
                        audio_empty = torch.empty((codebooks, 0), dtype=torch.long)

                        chat.append(
                            text=text_stack,
                            audio_out=audio_empty,
                            modality_flag=modality_tensor,
                        )
                    else:
                        chat.append(
                            text=text_stack,
                            audio_out=audio_stack,
                            modality_flag=modality_tensor,
                        )
                    chat.end_turn()
                    logger.debug("Chat history updated successfully")
                else:
                    logger.debug("No tokens to append, ending turn...")
                    chat.end_turn()
            except Exception as e:
                logger.error(f"Failed to append to chat history: {e}", exc_info=True)
                # Even if history append fails, we've already yielded all the data
                try:
                    chat.end_turn()
                except:
                    pass

            # If we haven't yielded anything yet, yield an empty audio response to prevent "No output" error
            # This ensures the async generator always yields at least one value
            logger.debug("Process method completed successfully")

        except Exception as e:
            logger.error(f"Error during LFM2-Audio generation: {e}", exc_info=True)
            # Yield an error message instead of raising to prevent "No output" error
            yield RuntimeData.text(f"ERROR: Speech-to-speech generation failed: {str(e)}")

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
        system_prompt="Respond with interleaved text and audio.",
        device="cpu",  # Use "cuda" if available
        audio_temperature=1.0,
        audio_top_k=4,
        max_new_tokens=4096,
        sample_rate=24000,
    )

    # Initialize
    await s2s_node.initialize()

    # Load test audio
    # You would need to provide your own test audio file
    test_audio_path = "examples/transcribe_demo.wav"
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

    async for response in s2s_node.process(input_audio):
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
