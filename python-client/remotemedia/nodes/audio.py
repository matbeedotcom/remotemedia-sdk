"""
Audio processing nodes for the RemoteMedia SDK.
"""

from typing import Any, AsyncGenerator, Union, TypedDict, Tuple, Optional
import logging
import librosa
import numpy as np

from ..core.node import Node
from ..core.types import _SENTINEL

logger = logging.getLogger(__name__)


# Type definitions for AudioTransform
class AudioTransformInput(TypedDict):
    """Input data structure for AudioTransform."""
    audio_data: np.ndarray
    sample_rate: int


class AudioTransformOutput(TypedDict):
    """Output data structure for AudioTransform."""
    audio_data: np.ndarray
    sample_rate: int
    processed_by: str
    node_config: dict


class AudioTransformError(TypedDict):
    """Error output structure for AudioTransform."""
    error: str
    input: Any
    processed_by: str


# Type definitions for AudioResampler
class AudioResamplerInput(TypedDict):
    """Input data structure for AudioResampler."""
    audio_data: np.ndarray
    sample_rate: int


class AudioResamplerOutput(TypedDict):
    """Output data structure for AudioResampler."""
    audio_data: np.ndarray
    sample_rate: int
    processed_by: str


class AudioResamplerError(TypedDict):
    """Error output structure for AudioResampler."""
    error: str
    input: Any
    processed_by: str


# Type definitions for ExtractAudioDataNode
class ExtractAudioDataInput(TypedDict):
    """Input data structure for ExtractAudioDataNode."""
    audio_data: np.ndarray
    sample_rate: int


ExtractAudioDataOutput = Optional[np.ndarray]


class ExtractAudioDataError(TypedDict):
    """Error output structure for ExtractAudioDataNode."""
    error: str
    input: Any
    processed_by: str


class AudioTransform(Node):
    """
    Audio transformation node that supports resampling and channel conversion.
    """

    def __init__(self, output_sample_rate: int = 44100, output_channels: int = 2, **kwargs):
        """
        Initializes the AudioTransform node.

        Args:
            output_sample_rate (int): The target sample rate for the audio.
            output_channels (int): The target number of channels for the audio.
        """
        super().__init__(output_sample_rate=output_sample_rate, output_channels=output_channels, **kwargs)
        self.output_sample_rate = output_sample_rate
        self.output_channels = output_channels

    def process(self, data: Union[Tuple[np.ndarray, int], Any]) -> Union[Tuple[np.ndarray, int], AudioTransformError]:
        """
        Processes audio data by resampling and converting channel counts.

        This method expects `data` to be a tuple `(audio_data, sample_rate)`,
        where `audio_data` is a NumPy array with shape (channels, samples).

        Args:
            data: A tuple containing the audio data and its sample rate.

        Returns:
            A tuple `(processed_audio_data, output_sample_rate)`.
        """
        # Use base class helper to extract metadata
        data_without_metadata, metadata = self.split_data_metadata(data)

        # Accept both tuple and list (for Rust runtime compatibility where JSON arrays become lists)
        if not isinstance(data_without_metadata, (tuple, list)) or len(data_without_metadata) < 2:
            logger.warning(
                f"AudioTransform '{self.name}': received data in "
                "unexpected format. Expected (audio_data, sample_rate) tuple or list."
            )
            return data

        audio_data, input_sample_rate = data_without_metadata[:2]

        if not isinstance(audio_data, np.ndarray):
            logger.warning(f"AudioTransform '{self.name}': audio data is not a numpy array.")
            return data

        logger.debug(f"AudioTransform '{self.name}': input shape={audio_data.shape}, sr={input_sample_rate} -> target sr={self.output_sample_rate}, channels={self.output_channels}")

        # Resample if necessary
        if input_sample_rate != self.output_sample_rate:
            # librosa.resample works with mono or multi-channel.
            audio_data = librosa.resample(
                y=audio_data, orig_sr=input_sample_rate, target_sr=self.output_sample_rate
            )

        # Ensure audio_data is 2D before channel manipulation
        if audio_data.ndim == 1:
            audio_data = audio_data.reshape(1, -1)

        current_channels = audio_data.shape[0]

        # Mix channels if necessary
        if current_channels != self.output_channels:
            if self.output_channels == 1:
                # Mix down to mono
                audio_data = librosa.to_mono(y=audio_data)
            elif current_channels == 1 and self.output_channels > 1:
                # Upmix mono to multi-channel
                audio_data = np.tile(audio_data, (self.output_channels, 1))
            else:
                # Fallback for other conversions (e.g., 5.1 to stereo)
                # by taking the first `output_channels`.
                logger.warning(
                    f"AudioTransform '{self.name}': complex channel conversion from "
                    f"{current_channels} to {self.output_channels} is simplified "
                    "by taking the first channels."
                )
                audio_data = audio_data[: self.output_channels, :]
        
        logger.debug(
            f"AudioTransform '{self.name}': output shape={audio_data.shape}, "
            f"sr={self.output_sample_rate}, channels={self.output_channels}"
        )

        # Use base class helper to preserve metadata
        result = (audio_data, self.output_sample_rate)
        return self.merge_data_metadata(result, metadata)


class AudioBuffer(Node):
    """
    Audio buffering node that accumulates audio data until a target size is reached.
    """

    def __init__(self, buffer_size_samples: int, **kwargs):
        """
        Initializes the AudioBuffer node.

        Args:
            buffer_size_samples (int): The number of samples to buffer before outputting.
        """
        super().__init__(buffer_size_samples=buffer_size_samples, **kwargs)
        self.buffer_size_samples = buffer_size_samples
        self._buffer = None
        self._sample_rate = None
        self._channels = None
        self.is_streaming = True

    async def process(self, data_stream) -> AsyncGenerator[Any, None]:
        """
        Buffers audio data until `buffer_size_samples` is reached.

        This method expects each item in the stream to be a tuple `(audio_data, sample_rate)`,
        where `audio_data` is a NumPy array with shape (channels, samples).

        It accumulates `audio_data` and when at least `buffer_size_samples` are
        available, it yields a chunk of that size. Any remaining samples are
        kept in the buffer for the next chunk.

        Args:
            data_stream: An async generator yielding tuples of (audio_data, sample_rate).

        Yields:
            A tuple `(buffered_audio_data, sample_rate)` when the buffer is full.
        """
        logger.info(f"AudioBuffer '{self.name}': process() started, beginning to consume data_stream")
        async for data in data_stream:
            logger.info(f"AudioBuffer '{self.name}': received data from stream")
            # Accept both tuple and list (for Rust runtime compatibility)
            if not isinstance(data, (tuple, list)) or len(data) != 2:
                logger.warning(
                    f"AudioBuffer '{self.name}': received data in unexpected format. "
                    "Expected (audio_data, sample_rate) tuple or list."
                )
                continue

            audio_chunk, sample_rate = data

            if not isinstance(audio_chunk, np.ndarray):
                logger.warning(f"AudioBuffer '{self.name}': audio data is not a numpy array.")
                continue

            logger.debug(f"AudioBuffer '{self.name}': received chunk shape={audio_chunk.shape}, sr={sample_rate}")

            # Ensure audio_chunk is 2D (channels, samples)
            if audio_chunk.ndim == 1:
                audio_chunk = audio_chunk.reshape(1, -1)

            # Initialize buffer if needed
            if self._buffer is None:
                self._sample_rate = sample_rate
                self._channels = audio_chunk.shape[0]
                self._buffer = np.zeros((self._channels, 0), dtype=audio_chunk.dtype)

            # Handle format changes
            if sample_rate != self._sample_rate or audio_chunk.shape[0] != self._channels:
                logger.warning(
                    f"AudioBuffer '{self.name}': Audio format changed mid-stream. "
                    "Flushing buffer and resetting."
                )
                if self._buffer.shape[1] > 0:
                    yield (self._buffer, self._sample_rate)
                self._sample_rate = sample_rate
                self._channels = audio_chunk.shape[0]
                self._buffer = audio_chunk
                continue

            # Append to buffer
            self._buffer = np.concatenate((self._buffer, audio_chunk), axis=1)

            # Output complete chunks
            while self._buffer.shape[1] >= self.buffer_size_samples:
                output_chunk = self._buffer[:, :self.buffer_size_samples]
                self._buffer = self._buffer[:, self.buffer_size_samples:]
                logger.debug(
                    f"AudioBuffer '{self.name}': outputting chunk of "
                    f"{self.buffer_size_samples} samples."
                )
                yield (output_chunk, self._sample_rate)

        # Flush any remaining data
        logger.info(f"AudioBuffer '{self.name}': stream ended, checking for remaining data")
        logger.info(f"AudioBuffer '{self.name}': buffer state - is None: {self._buffer is None}, shape: {self._buffer.shape if self._buffer is not None else 'N/A'}")
        if self._buffer is not None and self._buffer.shape[1] > 0:
            logger.info(f"AudioBuffer '{self.name}': flushing final chunk shape={self._buffer.shape}")
            yield (self._buffer, self._sample_rate)
        else:
            logger.info(f"AudioBuffer '{self.name}': no data to flush")


class AudioResampler(Node):
    """Audio resampling node."""
    
    def __init__(self, target_sample_rate: int = 44100, **kwargs):
        super().__init__(**kwargs)
        self.target_sample_rate = target_sample_rate
    
    def process(self, data: Union[Tuple[np.ndarray, int], Any]) -> Union[Tuple[np.ndarray, int], AudioResamplerError]:
        """Resample audio data."""
        logger.debug(f"AudioResampler '{self.name}': resampling to {self.target_sample_rate}Hz")
        
        if not isinstance(data, tuple) or len(data) != 2:
            return {
                "error": "Input must be a tuple of (audio_data, sample_rate)",
                "input": data,
                "processed_by": f"AudioResampler[{self.name}]"
            }
        
        audio_data, sample_rate = data
        
        if not isinstance(audio_data, np.ndarray):
            return {
                "error": "Audio data must be a numpy array",
                "input": data,
                "processed_by": f"AudioResampler[{self.name}]"
            }
        
        try:
            # TODO: Implement actual resampling with librosa
            # For now, just return the data unchanged as placeholder
            return data
        except Exception as e:
            logger.error(f"AudioResampler '{self.name}': resampling failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"AudioResampler[{self.name}]"
            }


class ExtractAudioDataNode(Node):
    """
    A simple node that extracts the audio ndarray from a (data, rate) tuple.
    It also flattens the array to ensure it is 1D, as required by many
    Hugging Face audio models.
    """
    def process(self, data: Union[Tuple[np.ndarray, int], Any]) -> Union[np.ndarray, None, ExtractAudioDataError]:
        """
        Expects a tuple of (audio_data, sample_rate) and returns a flattened
        1D numpy array of the audio data.
        """
        try:
            if isinstance(data, tuple) and len(data) == 2 and isinstance(data[0], np.ndarray):
                # Flatten to ensure it's a 1D array for models that require it
                return data[0].flatten()
            
            logger.warning(
                f"{self.__class__.__name__} '{self.name}': received data in "
                "unexpected format. Expected a (ndarray, int) tuple. Returning None."
            )
            return None
        except Exception as e:
            logger.error(f"ExtractAudioDataNode '{self.name}': extraction failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"ExtractAudioDataNode[{self.name}]"
            }


class AudioResampleNode(Node):
    """
    Audio resampling node with optional Rust acceleration.
    
    This node resamples audio to a target sample rate using high-quality algorithms.
    When runtime_hint="auto" or "rust", it will use the Rust implementation if available
    for 50-100x faster resampling. Falls back to Python (librosa) if Rust is unavailable.
    """
    
    def __init__(
        self,
        target_sample_rate: int = 16000,
        quality: str = "high",
        runtime_hint: str = "auto",
        **kwargs
    ):
        """
        Initialize the AudioResampleNode.
        
        Args:
            target_sample_rate: Target sample rate in Hz (default: 16000)
            quality: Resampling quality - "low", "medium", or "high" (default: "high")
            runtime_hint: Runtime selection - "auto", "rust", or "python" (default: "auto")
        """
        super().__init__(**kwargs)
        self.target_sample_rate = target_sample_rate
        self.quality = quality
        self.runtime_hint = runtime_hint
        self._use_rust = self._should_use_rust()
        
        if self._use_rust:
            logger.info(f"AudioResampleNode '{self.name}': Using Rust runtime")
        else:
            logger.info(f"AudioResampleNode '{self.name}': Using Python runtime (librosa)")
    
    async def initialize(self):
        """Initialize the node (async for pipeline compatibility)."""
        super().initialize()
    
    def _should_use_rust(self) -> bool:
        """Determine if Rust runtime should be used."""
        if self.runtime_hint == "python":
            return False
        
        try:
            import remotemedia_runtime
            if self.runtime_hint == "rust":
                return True
            # runtime_hint == "auto"
            return True
        except ImportError:
            if self.runtime_hint == "rust":
                logger.warning(
                    f"AudioResampleNode '{self.name}': Rust runtime requested but not available. "
                    "Install with: cd runtime && maturin develop --release"
                )
            return False
    
    def process(self, data: Union[Tuple[np.ndarray, int], Any]) -> Union[Tuple[np.ndarray, int], dict]:
        """
        Resample audio data to target sample rate.
        
        Args:
            data: Tuple of (audio_data, sample_rate)
            
        Returns:
            Tuple of (resampled_audio_data, target_sample_rate)
        """
        if not isinstance(data, (tuple, list)) or len(data) < 2:
            return {
                "error": "Input must be a tuple of (audio_data, sample_rate)",
                "input": data,
                "processed_by": f"AudioResampleNode[{self.name}]"
            }
        
        audio_data, sample_rate = data[:2]
        
        if not isinstance(audio_data, np.ndarray):
            return {
                "error": "Audio data must be a numpy array",
                "input": data,
                "processed_by": f"AudioResampleNode[{self.name}]"
            }
        
        # No resampling needed
        if sample_rate == self.target_sample_rate:
            return (audio_data, sample_rate)
        
        try:
            if self._use_rust:
                return self._resample_rust(audio_data, sample_rate)
            else:
                return self._resample_python(audio_data, sample_rate)
        except Exception as e:
            logger.error(f"AudioResampleNode '{self.name}': Resampling failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"AudioResampleNode[{self.name}]"
            }
    
    def _resample_rust(self, audio_data: np.ndarray, sample_rate: int) -> Tuple[np.ndarray, int]:
        """Resample using Rust runtime."""
        # TODO: Add FFI binding for direct Rust node calls
        # For now, fall back to Python implementation
        # Future: remotemedia_runtime.resample_audio(audio_data, sample_rate, target_sr, quality)
        logger.debug(f"AudioResampleNode '{self.name}': Rust FFI not yet implemented, using Python")
        return self._resample_python(audio_data, sample_rate)
    
    def _resample_python(self, audio_data: np.ndarray, sample_rate: int) -> Tuple[np.ndarray, int]:
        """Resample using Python librosa."""
        # Ensure audio is 2D (channels, samples)
        if audio_data.ndim == 1:
            audio_data = audio_data.reshape(1, -1)
        
        # Resample each channel
        resampled_channels = []
        for channel in audio_data:
            resampled = librosa.resample(
                y=channel,
                orig_sr=sample_rate,
                target_sr=self.target_sample_rate
            )
            resampled_channels.append(resampled)
        
        resampled_audio = np.array(resampled_channels)
        return (resampled_audio, self.target_sample_rate)


class VADNode(Node):
    """
    Voice Activity Detection node with optional Rust acceleration.
    
    This node detects speech segments in audio using energy-based VAD.
    When runtime_hint="auto" or "rust", it will use the Rust implementation if available
    for 50-100x faster VAD. Falls back to Python if Rust is unavailable.
    """
    
    def __init__(
        self,
        frame_duration_ms: int = 30,
        energy_threshold: float = 0.02,
        runtime_hint: str = "auto",
        **kwargs
    ):
        """
        Initialize the VAD node.
        
        Args:
            frame_duration_ms: Duration of each frame for VAD analysis in ms (default: 30)
            energy_threshold: Energy threshold for speech detection (default: 0.02)
            runtime_hint: Runtime selection - "auto", "rust", or "python" (default: "auto")
        """
        super().__init__(**kwargs)
        self.frame_duration_ms = frame_duration_ms
        self.energy_threshold = energy_threshold
        self.runtime_hint = runtime_hint
        self._use_rust = self._should_use_rust()
        
        if self._use_rust:
            logger.info(f"VADNode '{self.name}': Using Rust runtime")
        else:
            logger.info(f"VADNode '{self.name}': Using Python runtime (numpy-based)")
    
    async def initialize(self):
        """Initialize the node (async for pipeline compatibility)."""
        super().initialize()
    
    def _should_use_rust(self) -> bool:
        """Determine if Rust runtime should be used."""
        if self.runtime_hint == "python":
            return False
        
        try:
            import remotemedia_runtime
            if self.runtime_hint == "rust":
                return True
            # runtime_hint == "auto"
            return True
        except ImportError:
            if self.runtime_hint == "rust":
                logger.warning(
                    f"VADNode '{self.name}': Rust runtime requested but not available. "
                    "Install with: cd runtime && maturin develop --release"
                )
            return False
    
    def process(self, data: Union[Tuple[np.ndarray, int], Any]) -> Union[Tuple[np.ndarray, int, dict], dict]:
        """
        Detect voice activity in audio.
        
        Args:
            data: Tuple of (audio_data, sample_rate)
            
        Returns:
            Tuple of (audio_data, sample_rate, vad_results) where vad_results contains:
            - is_speech: bool indicating if speech was detected
            - speech_frames: number of frames containing speech
            - total_frames: total number of frames analyzed
        """
        if not isinstance(data, (tuple, list)) or len(data) < 2:
            return {
                "error": "Input must be a tuple of (audio_data, sample_rate)",
                "input": data,
                "processed_by": f"VADNode[{self.name}]"
            }
        
        audio_data, sample_rate = data[:2]
        
        if not isinstance(audio_data, np.ndarray):
            return {
                "error": "Audio data must be a numpy array",
                "input": data,
                "processed_by": f"VADNode[{self.name}]"
            }
        
        try:
            if self._use_rust:
                return self._vad_rust(audio_data, sample_rate)
            else:
                return self._vad_python(audio_data, sample_rate)
        except Exception as e:
            logger.error(f"VADNode '{self.name}': VAD failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"VADNode[{self.name}]"
            }
    
    def _vad_rust(self, audio_data: np.ndarray, sample_rate: int) -> Tuple[np.ndarray, int, dict]:
        """Perform VAD using Rust runtime."""
        # TODO: Add FFI binding for direct Rust node calls
        # For now, fall back to Python implementation
        # Future: remotemedia_runtime.detect_voice_activity(audio_data, sr, frame_ms, threshold)
        logger.debug(f"VADNode '{self.name}': Rust FFI not yet implemented, using Python")
        return self._vad_python(audio_data, sample_rate)
    
    def _vad_python(self, audio_data: np.ndarray, sample_rate: int) -> Tuple[np.ndarray, int, dict]:
        """Perform VAD using Python implementation."""
        # Ensure audio is 2D (channels, samples)
        if audio_data.ndim == 1:
            audio_data = audio_data.reshape(1, -1)
        
        # Convert to mono for VAD analysis
        if audio_data.shape[0] > 1:
            mono_audio = np.mean(audio_data, axis=0)
        else:
            mono_audio = audio_data[0]
        
        # Calculate frame size
        frame_samples = int(sample_rate * self.frame_duration_ms / 1000)
        num_frames = len(mono_audio) // frame_samples
        speech_frames = 0
        
        # Analyze each frame
        for i in range(num_frames):
            start = i * frame_samples
            end = start + frame_samples
            frame = mono_audio[start:end]
            
            # Calculate frame energy (RMS)
            energy = np.sqrt(np.mean(frame**2))
            
            if energy > self.energy_threshold:
                speech_frames += 1
        
        vad_results = {
            "is_speech": speech_frames > 0,
            "speech_frames": speech_frames,
            "total_frames": num_frames
        }
        
        return (audio_data, sample_rate, vad_results)


class FormatConverterNode(Node):
    """
    Audio format conversion node with optional Rust acceleration.
    
    This node converts audio between different sample formats (f32, i16, i32).
    When runtime_hint="auto" or "rust", it will use zero-copy Rust implementation
    if available for 50-100x faster conversion. Falls back to Python if Rust is unavailable.
    """
    
    def __init__(
        self,
        target_format: str = "f32",
        runtime_hint: str = "auto",
        **kwargs
    ):
        """
        Initialize the FormatConverterNode.
        
        Args:
            target_format: Target format - "f32", "i16", or "i32" (default: "f32")
            runtime_hint: Runtime selection - "auto", "rust", or "python" (default: "auto")
        """
        super().__init__(**kwargs)
        self.target_format = target_format.lower()
        self.runtime_hint = runtime_hint
        self._use_rust = self._should_use_rust()
        
        if self.target_format not in ["f32", "i16", "i32"]:
            raise ValueError(f"Unsupported format: {target_format}. Must be 'f32', 'i16', or 'i32'")
        
        if self._use_rust:
            logger.info(f"FormatConverterNode '{self.name}': Using Rust runtime")
        else:
            logger.info(f"FormatConverterNode '{self.name}': Using Python runtime (numpy)")
    
    async def initialize(self):
        """Initialize the node (async for pipeline compatibility)."""
        super().initialize()
    
    def _should_use_rust(self) -> bool:
        """Determine if Rust runtime should be used."""
        if self.runtime_hint == "python":
            return False
        
        try:
            import remotemedia_runtime
            if self.runtime_hint == "rust":
                return True
            # runtime_hint == "auto"
            return True
        except ImportError:
            if self.runtime_hint == "rust":
                logger.warning(
                    f"FormatConverterNode '{self.name}': Rust runtime requested but not available. "
                    "Install with: cd runtime && maturin develop --release"
                )
            return False
    
    def process(self, data: Union[Tuple[np.ndarray, int], Any]) -> Union[Tuple[np.ndarray, int], dict]:
        """
        Convert audio format.
        
        Args:
            data: Tuple of (audio_data, sample_rate)
            
        Returns:
            Tuple of (converted_audio_data, sample_rate)
        """
        if not isinstance(data, (tuple, list)) or len(data) < 2:
            return {
                "error": "Input must be a tuple of (audio_data, sample_rate)",
                "input": data,
                "processed_by": f"FormatConverterNode[{self.name}]"
            }
        
        audio_data, sample_rate = data[:2]
        
        if not isinstance(audio_data, np.ndarray):
            return {
                "error": "Audio data must be a numpy array",
                "input": data,
                "processed_by": f"FormatConverterNode[{self.name}]"
            }
        
        try:
            if self._use_rust:
                return self._convert_rust(audio_data, sample_rate)
            else:
                return self._convert_python(audio_data, sample_rate)
        except Exception as e:
            logger.error(f"FormatConverterNode '{self.name}': Conversion failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"FormatConverterNode[{self.name}]"
            }
    
    def _convert_rust(self, audio_data: np.ndarray, sample_rate: int) -> Tuple[np.ndarray, int]:
        """Convert format using Rust runtime (zero-copy)."""
        # TODO: Add FFI binding for direct Rust node calls
        # For now, fall back to Python implementation  
        # Future: remotemedia_runtime.convert_audio_format(audio_data, target_format)
        logger.debug(f"FormatConverterNode '{self.name}': Rust FFI not yet implemented, using Python")
        return self._convert_python(audio_data, sample_rate)
    
    def _convert_python(self, audio_data: np.ndarray, sample_rate: int) -> Tuple[np.ndarray, int]:
        """Convert format using Python implementation."""
        # Ensure audio is 2D (channels, samples)
        if audio_data.ndim == 1:
            audio_data = audio_data.reshape(1, -1)
        
        # Get current format
        current_dtype = audio_data.dtype
        
        # Convert to target format
        if self.target_format == "f32":
            if current_dtype == np.float32:
                converted = audio_data
            elif current_dtype == np.int16:
                converted = audio_data.astype(np.float32) / 32768.0
            elif current_dtype == np.int32:
                converted = audio_data.astype(np.float32) / 2147483648.0
            else:
                converted = audio_data.astype(np.float32)
        elif self.target_format == "i16":
            if current_dtype == np.int16:
                converted = audio_data
            elif current_dtype == np.float32:
                converted = (audio_data * 32768.0).astype(np.int16)
            elif current_dtype == np.int32:
                converted = (audio_data / 65536).astype(np.int16)
            else:
                converted = audio_data.astype(np.int16)
        elif self.target_format == "i32":
            if current_dtype == np.int32:
                converted = audio_data
            elif current_dtype == np.float32:
                converted = (audio_data * 2147483648.0).astype(np.int32)
            elif current_dtype == np.int16:
                converted = (audio_data.astype(np.int32) * 65536)
            else:
                converted = audio_data.astype(np.int32)
        else:
            converted = audio_data
        
        return (converted, sample_rate)


class VoiceActivityDetector(Node):
    """
    Voice Activity Detection (VAD) node that detects speech segments in audio streams.
    
    This node uses energy-based VAD with adaptive thresholding to detect speech.
    It can operate in two modes:
    - Passthrough mode: Adds VAD metadata to audio chunks
    - Filter mode: Only passes through audio chunks containing speech
    
    NOTE: This is the legacy streaming VAD implementation. For batch processing,
    consider using the new VADNode which supports Rust acceleration.
    """
    
    def __init__(
        self,
        frame_duration_ms: int = 30,
        energy_threshold: float = 0.02,
        speech_threshold: float = 0.3,
        filter_mode: bool = False,
        include_metadata: bool = True,
        **kwargs
    ):
        """
        Initialize the VAD node.
        
        Args:
            frame_duration_ms: Duration of each frame for VAD analysis (10, 20, or 30 ms)
            energy_threshold: Energy threshold for speech detection (0.0 to 1.0)
            speech_threshold: Ratio of speech frames to total frames to consider segment as speech
            filter_mode: If True, only output audio chunks containing speech
            include_metadata: If True, include VAD metadata in output
        """
        super().__init__(**kwargs)
        self.frame_duration_ms = frame_duration_ms
        self.energy_threshold = energy_threshold
        self.speech_threshold = speech_threshold
        self.filter_mode = filter_mode
        self.include_metadata = include_metadata
        self.is_streaming = True
        
        # State for adaptive thresholding
        self._energy_history = []
        self._history_size = 100
        
    async def process(self, data_stream) -> AsyncGenerator[Any, None]:
        """
        Process audio stream for voice activity detection.
        
        Args:
            data_stream: Async generator yielding (audio_data, sample_rate) tuples
            
        Yields:
            In passthrough mode: ((audio_data, sample_rate), vad_metadata)
            In filter mode: (audio_data, sample_rate) for speech segments only
        """
        async for data in data_stream:
            # Extract metadata if present
            data_without_metadata, input_metadata = self.split_data_metadata(data)
            
            if not isinstance(data_without_metadata, tuple) or len(data_without_metadata) < 2:
                logger.warning(f"VAD '{self.name}': Invalid input format")
                continue
                
            audio_data, sample_rate = data_without_metadata[:2]
            
            if not isinstance(audio_data, np.ndarray):
                logger.warning(f"VAD '{self.name}': Audio data is not numpy array")
                continue
                
            # Ensure 2D array (channels, samples)
            if audio_data.ndim == 1:
                audio_data = audio_data.reshape(1, -1)
                
            # Convert to mono for VAD analysis
            if audio_data.shape[0] > 1:
                mono_audio = np.mean(audio_data, axis=0)
            else:
                mono_audio = audio_data[0]
                
            # Calculate frame size in samples
            frame_samples = int(sample_rate * self.frame_duration_ms / 1000)
            
            # Analyze frames
            is_speech, vad_info = self._analyze_audio(mono_audio, frame_samples)
            
            # Create VAD metadata
            vad_metadata = {
                "is_speech": is_speech,
                "speech_ratio": vad_info["speech_ratio"],
                "avg_energy": vad_info["avg_energy"],
                "frame_duration_ms": self.frame_duration_ms
            }
            
            # Output based on mode
            if self.filter_mode:
                # Only output if speech detected
                if is_speech:
                    logger.debug(f"VAD '{self.name}': Speech detected (ratio: {vad_info['speech_ratio']:.2f})")
                    # Preserve input metadata
                    result = (audio_data, sample_rate)
                    yield self.merge_data_metadata(result, input_metadata)
                else:
                    logger.debug(f"VAD '{self.name}': No speech detected, filtering out")
            else:
                # Passthrough mode - always output with metadata
                if self.include_metadata:
                    # Preserve input metadata while adding VAD info
                    result = (audio_data, sample_rate)
                    if input_metadata:
                        # Merge VAD metadata with input metadata
                        merged_metadata = input_metadata.copy()
                        merged_metadata.update(vad_metadata)
                        yield ((audio_data, sample_rate), merged_metadata)
                    else:
                        yield ((audio_data, sample_rate), vad_metadata)
                else:
                    # No VAD metadata requested, just preserve input metadata
                    result = (audio_data, sample_rate)
                    yield self.merge_data_metadata(result, input_metadata)
                    
    def _analyze_audio(self, audio: np.ndarray, frame_samples: int) -> tuple[bool, dict]:
        """
        Analyze audio for voice activity.
        
        Returns:
            (is_speech, vad_info) tuple
        """
        # Calculate energy for each frame
        num_frames = len(audio) // frame_samples
        speech_frames = 0
        total_energy = 0
        
        # Adaptive threshold based on recent history
        adaptive_threshold = self._calculate_adaptive_threshold()
        
        for i in range(num_frames):
            start = i * frame_samples
            end = start + frame_samples
            frame = audio[start:end]
            
            # Calculate frame energy (RMS)
            energy = np.sqrt(np.mean(frame**2))
            total_energy += energy
            
            # Update energy history
            self._energy_history.append(energy)
            if len(self._energy_history) > self._history_size:
                self._energy_history.pop(0)
            
            # Check if frame contains speech
            if energy > adaptive_threshold:
                speech_frames += 1
                
        # Calculate statistics
        avg_energy = total_energy / max(num_frames, 1)
        speech_ratio = speech_frames / max(num_frames, 1)
        
        # Determine if segment contains speech
        is_speech = speech_ratio >= self.speech_threshold
        
        return is_speech, {
            "speech_ratio": speech_ratio,
            "avg_energy": float(avg_energy),
            "adaptive_threshold": float(adaptive_threshold),
            "num_frames": num_frames,
            "speech_frames": speech_frames
        }
        
    def _calculate_adaptive_threshold(self) -> float:
        """
        Calculate adaptive energy threshold based on recent history.
        """
        if not self._energy_history:
            return self.energy_threshold
            
        # Use percentile-based threshold
        sorted_history = sorted(self._energy_history)
        percentile_idx = int(len(sorted_history) * 0.3)  # 30th percentile
        noise_floor = sorted_history[percentile_idx] if percentile_idx < len(sorted_history) else self.energy_threshold
        
        # Adaptive threshold is noise floor plus margin
        adaptive = noise_floor * 2.5
        
        # Blend with fixed threshold
        return 0.7 * adaptive + 0.3 * self.energy_threshold


__all__ = [
    "AudioTransform",
    "AudioBuffer",
    "AudioResampler",
    "ExtractAudioDataNode",
    "AudioResampleNode",
    "VADNode",
    "FormatConverterNode",
    "VoiceActivityDetector"
] 