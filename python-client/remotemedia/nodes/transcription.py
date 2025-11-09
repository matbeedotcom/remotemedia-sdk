"""
Transcription nodes for the RemoteMedia SDK.

Supports both Python (WhisperX) and Rust (rwhisper) implementations.
"""

from typing import Any, AsyncGenerator, Union, Optional, Dict
import logging
import numpy as np
import asyncio

from ..core.node import Node

logger = logging.getLogger(__name__)


class WhisperXTranscriber(Node):
    """
    Transcribe audio using WhisperX (Python implementation).

    WhisperX provides:
    - Fast batched inference with CTranslate2
    - VAD preprocessing with Silero VAD
    - Word-level timestamps with Wav2Vec2 alignment
    - Speaker diarization (optional)

    Args:
        model_size: Whisper model size ("tiny", "base", "small", "medium", "large-v2", "large-v3")
        device: Device to run on ("cpu", "cuda")
        compute_type: Compute type for CTranslate2 ("float16", "int8", "float32")
        batch_size: Batch size for inference (default: 16)
        language: Language code (e.g., "en") or None for auto-detect
        align_model: Whether to load alignment model for word-level timestamps
        **kwargs: Additional Node parameters
    """

    def __init__(
        self,
        model_size: str = "base",
        device: str = "cpu",
        compute_type: str = "float32",
        batch_size: int = 16,
        language: Optional[str] = None,
        align_model: bool = False,
        vad_onset: float = 0.500,
        vad_offset: float = 0.363,
        **kwargs
    ):
        super().__init__(
            model_size=model_size,
            device=device,
            compute_type=compute_type,
            batch_size=batch_size,
            language=language,
            align_model=align_model,
            vad_onset=vad_onset,
            vad_offset=vad_offset,
            **kwargs
        )
        self.model_size = model_size
        self.device = device
        self.compute_type = compute_type
        self.batch_size = batch_size
        self.language = language
        self.align_model = align_model
        self.vad_onset = vad_onset
        self.vad_offset = vad_offset
        self.model = None
        self.alignment_model = None
        self.metadata = None
        self.is_streaming = True

    def _lazy_load_model(self):
        """Lazy load WhisperX model on first use."""
        if self.model is not None:
            return

        try:
            import whisperx
        except ImportError:
            raise ImportError(
                "WhisperX is not installed. Install with: pip install whisperx"
            )

        logger.info(f"Loading WhisperX model: {self.model_size} on {self.device}")
        self.model = whisperx.load_model(
            self.model_size,
            device=self.device,
            compute_type=self.compute_type,
            language=self.language,
            vad_options={
                "vad_onset": self.vad_onset,
                "vad_offset": self.vad_offset,
            }
        )

        # Load alignment model if requested
        if self.align_model and self.language:
            logger.info(f"Loading alignment model for language: {self.language}")
            self.alignment_model, self.metadata = whisperx.load_align_model(
                language_code=self.language,
                device=self.device
            )

    async def process(self, data_stream):
        """Process audio chunks and transcribe."""
        self._lazy_load_model()

        async for data in data_stream:
            # Handle different input formats
            if isinstance(data, tuple) and len(data) == 2:
                audio, sample_rate = data
            elif isinstance(data, dict):
                audio = data.get("audio_data", data.get("audio"))
                sample_rate = data.get("sample_rate", 16000)
            else:
                logger.warning(f"Unexpected data format: {type(data)}")
                continue

            # Ensure audio is numpy array
            if not isinstance(audio, np.ndarray):
                logger.warning(f"Audio is not numpy array: {type(audio)}")
                continue

            logger.debug(f"Received audio: shape={audio.shape}, dtype={audio.dtype}, sample_rate={sample_rate}")

            # Ensure audio is 1D float32
            if audio.ndim > 1:
                audio = audio.flatten()
            if audio.dtype != np.float32:
                audio = audio.astype(np.float32)

            # Normalize to [-1, 1] range if needed
            if audio.max() > 1.0 or audio.min() < -1.0:
                audio = audio / max(abs(audio.max()), abs(audio.min()))

            # Run transcription in executor to avoid blocking
            result = await asyncio.get_event_loop().run_in_executor(
                None,
                self._transcribe_audio,
                audio,
                sample_rate
            )

            logger.debug(f"Transcription result: {result}")
            yield result

    def _transcribe_audio(self, audio: np.ndarray, sample_rate: int) -> Dict[str, Any]:
        """Synchronous transcription (runs in executor)."""
        import whisperx

        # Debug: Log audio properties after normalization
        logger.info(f"Sending to WhisperX: shape={audio.shape}, dtype={audio.dtype}, "
                    f"range=[{audio.min():.6f}, {audio.max():.6f}], "
                    f"mean={audio.mean():.6f}, std={audio.std():.6f}, "
                    f"duration={len(audio) / sample_rate:.2f}s")

        # Check for silence
        non_zero = np.count_nonzero(np.abs(audio) > 0.0001)
        logger.info(f"WhisperX audio stats: non_zero={non_zero}/{len(audio)}, max_abs={np.abs(audio).max():.6f}")
        logger.info(f"WhisperX first 10 samples: {audio[:10].tolist()}")

        # Transcribe with WhisperX
        result = self.model.transcribe(
            audio,
            batch_size=self.batch_size,
            language=self.language,
        )

        # Perform alignment if model is loaded
        if self.alignment_model and self.metadata:
            result = whisperx.align(
                result["segments"],
                self.alignment_model,
                self.metadata,
                audio,
                device=self.device,
                return_char_alignments=False
            )

        # Extract text from segments if not directly available
        text = result.get("text", "")
        if not text and "segments" in result:
            # Concatenate all segment texts
            text = "".join(seg.get("text", "") for seg in result["segments"])

        return {
            "text": text,
            "segments": result.get("segments", []),
            "language": result.get("language", self.language),
            "audio_duration": len(audio) / sample_rate,
            "sample_rate": sample_rate
        }


class RustWhisperTranscriber(Node):
    """
    Transcribe audio using rwhisper (Rust implementation).

    This is a pure pass-through node that serializes to a Rust-native node in the manifest.
    The actual transcription is handled entirely by the Rust runtime.

    Args:
        model_path: Path to Whisper GGML model file (optional if model_source is provided)
        model_source: Pre-defined model to auto-download (e.g., "tiny", "tiny.en", "base", "base.en", "small", "small.en")
        language: Language code (e.g., "en") or None for auto-detect
        n_threads: Number of threads for inference
        **kwargs: Additional Node parameters
    """

    def __init__(
        self,
        model_path: Optional[str] = None,
        model_source: Optional[str] = None,
        language: Optional[str] = None,
        n_threads: int = 4,
        **kwargs
    ):
        params = {
            "language": language,
            "n_threads": n_threads,
        }
        if model_path is not None:
            params["model_path"] = model_path
        if model_source is not None:
            params["model_source"] = model_source

        super().__init__(**params, **kwargs)
        logger.debug(f"RustWhisperTranscriber '{self.name}': Initialized with params: {params}")
        self.model_path = model_path
        self.model_source = model_source
        self.language = language
        self.n_threads = n_threads
        # NOT a streaming node - it's a simple processing node executed by Rust
        self.is_streaming = False

    def process(self, data):
        """
        This should never be called in Python mode.
        The node is designed to be executed by the Rust runtime.
        """
        logger.debug(f"RustWhisperTranscriber '{self.name}': process called")
        raise NotImplementedError(
            "RustWhisperTranscriber must be executed via Rust runtime. "
            "Use pipeline.run(use_rust=True) or install the Rust runtime with: "
            "cd runtime && maturin develop --release --features whisper"
        )
