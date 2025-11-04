"""
Runtime data structures for multiprocess IPC.

This module provides Python equivalents of the Rust RuntimeData structures
for zero-copy inter-process communication.
"""

from dataclasses import dataclass
from enum import Enum
from typing import Union, Optional, Any
import numpy as np
import struct
import time
from datetime import datetime


class DataType(Enum):
    """Data type discriminator."""
    AUDIO = 1
    VIDEO = 2
    TEXT = 3
    TENSOR = 4


class AudioFormat(Enum):
    """Audio format types."""
    F32 = 1  # 32-bit float
    I16 = 2  # 16-bit integer
    I24 = 3  # 24-bit integer
    I32 = 4  # 32-bit integer
    U8 = 5   # 8-bit unsigned


class VideoFormat(Enum):
    """Video format types."""
    RGB = 1     # RGB 8-bit per channel
    RGBA = 2    # RGBA 8-bit per channel
    YUV420 = 3  # YUV 4:2:0
    YUV422 = 4  # YUV 4:2:2
    YUV444 = 5  # YUV 4:4:4
    BGR = 6     # BGR 8-bit per channel


class TensorType(Enum):
    """Tensor data types."""
    F32 = 1  # 32-bit float
    F64 = 2  # 64-bit float
    I32 = 3  # 32-bit integer
    I64 = 4  # 64-bit integer
    U8 = 5   # 8-bit unsigned
    U16 = 6  # 16-bit unsigned


@dataclass
class AudioMetadata:
    """Audio-specific metadata."""
    sample_rate: int  # Hz
    channels: int     # 1=mono, 2=stereo
    format: AudioFormat
    duration_ms: int  # Duration in milliseconds


@dataclass
class VideoMetadata:
    """Video-specific metadata."""
    width: int
    height: int
    format: VideoFormat
    fps: float


@dataclass
class TextMetadata:
    """Text-specific metadata."""
    encoding: str = "utf-8"
    language: Optional[str] = None  # ISO 639-1 code


@dataclass
class TensorMetadata:
    """Tensor-specific metadata."""
    shape: tuple
    dtype: TensorType


@dataclass
class RuntimeData:
    """
    Zero-copy data container for IPC.

    This class represents data that can be transferred between processes
    with minimal overhead using shared memory.
    """

    type: DataType
    payload: Union[np.ndarray, bytes, str]
    session_id: str
    timestamp: float
    metadata: Optional[Union[AudioMetadata, VideoMetadata, TextMetadata, TensorMetadata]] = None

    def __post_init__(self):
        """Validate and normalize data after initialization."""
        # Set timestamp if not provided
        if self.timestamp is None:
            self.timestamp = time.time()

        # Validate payload type matches metadata
        if self.type == DataType.AUDIO and not isinstance(self.metadata, AudioMetadata):
            raise ValueError("Audio data requires AudioMetadata")
        elif self.type == DataType.VIDEO and not isinstance(self.metadata, VideoMetadata):
            raise ValueError("Video data requires VideoMetadata")
        elif self.type == DataType.TEXT:
            if isinstance(self.payload, str):
                self.payload = self.payload.encode('utf-8')
            if self.metadata is None:
                self.metadata = TextMetadata()
        elif self.type == DataType.TENSOR and not isinstance(self.metadata, TensorMetadata):
            raise ValueError("Tensor data requires TensorMetadata")

    @property
    def size(self) -> int:
        """Get payload size in bytes."""
        if isinstance(self.payload, np.ndarray):
            return self.payload.nbytes
        elif isinstance(self.payload, (bytes, str)):
            return len(self.payload)
        return 0

    def as_numpy(self) -> np.ndarray:
        """
        Get payload as numpy array (zero-copy when possible).

        Returns:
            Numpy array view of the payload data

        Raises:
            ValueError: If payload cannot be converted to numpy
        """
        if isinstance(self.payload, np.ndarray):
            return self.payload
        elif isinstance(self.payload, bytes):
            return np.frombuffer(self.payload, dtype=np.uint8)
        raise ValueError(f"Cannot convert {type(self.payload)} to numpy array")

    def as_text(self, encoding: str = "utf-8") -> str:
        """
        Get payload as text string.

        Args:
            encoding: Text encoding to use

        Returns:
            Decoded text string

        Raises:
            ValueError: If payload is not text data
        """
        if self.type != DataType.TEXT:
            raise ValueError("Can only convert TEXT data to string")

        if isinstance(self.payload, str):
            return self.payload
        elif isinstance(self.payload, bytes):
            return self.payload.decode(encoding)
        else:
            raise ValueError(f"Cannot convert {type(self.payload)} to text")

    @classmethod
    def audio(cls,
              samples: Union[np.ndarray, list],
              sample_rate: int,
              channels: int = 1,
              session_id: str = "",
              format: AudioFormat = AudioFormat.F32) -> "RuntimeData":
        """
        Create audio runtime data.

        Args:
            samples: Audio samples as numpy array or list
            sample_rate: Sample rate in Hz
            channels: Number of channels (1=mono, 2=stereo)
            session_id: Session identifier
            format: Audio sample format

        Returns:
            RuntimeData instance for audio
        """
        if not isinstance(samples, np.ndarray):
            samples = np.array(samples, dtype=np.float32)

        duration_ms = int((len(samples) / channels / sample_rate) * 1000)

        metadata = AudioMetadata(
            sample_rate=sample_rate,
            channels=channels,
            format=format,
            duration_ms=duration_ms
        )

        return cls(
            type=DataType.AUDIO,
            payload=samples,
            session_id=session_id,
            timestamp=time.time(),
            metadata=metadata
        )

    @classmethod
    def video(cls,
              frame: Union[np.ndarray, bytes],
              width: int,
              height: int,
              format: VideoFormat = VideoFormat.RGB,
              fps: float = 30.0,
              session_id: str = "") -> "RuntimeData":
        """
        Create video runtime data.

        Args:
            frame: Video frame data
            width: Frame width in pixels
            height: Frame height in pixels
            format: Video format
            fps: Frames per second
            session_id: Session identifier

        Returns:
            RuntimeData instance for video
        """
        if not isinstance(frame, (np.ndarray, bytes)):
            raise ValueError("Frame must be numpy array or bytes")

        metadata = VideoMetadata(
            width=width,
            height=height,
            format=format,
            fps=fps
        )

        return cls(
            type=DataType.VIDEO,
            payload=frame,
            session_id=session_id,
            timestamp=time.time(),
            metadata=metadata
        )

    @classmethod
    def text(cls,
             text: str,
             session_id: str = "",
             language: Optional[str] = None) -> "RuntimeData":
        """
        Create text runtime data.

        Args:
            text: Text string
            session_id: Session identifier
            language: ISO 639-1 language code

        Returns:
            RuntimeData instance for text
        """
        metadata = TextMetadata(
            encoding="utf-8",
            language=language
        )

        return cls(
            type=DataType.TEXT,
            payload=text,
            session_id=session_id,
            timestamp=time.time(),
            metadata=metadata
        )

    @classmethod
    def tensor(cls,
               data: Union[np.ndarray, list],
               session_id: str = "",
               dtype: TensorType = TensorType.F32) -> "RuntimeData":
        """
        Create tensor runtime data.

        Args:
            data: Tensor data as numpy array or list
            session_id: Session identifier
            dtype: Tensor data type

        Returns:
            RuntimeData instance for tensor
        """
        if not isinstance(data, np.ndarray):
            data = np.array(data)

        metadata = TensorMetadata(
            shape=data.shape,
            dtype=dtype
        )

        return cls(
            type=DataType.TENSOR,
            payload=data,
            session_id=session_id,
            timestamp=time.time(),
            metadata=metadata
        )

    def to_bytes(self) -> bytes:
        """
        Serialize to bytes for IPC transfer.

        Returns:
            Serialized bytes representation
        """
        # This would use msgpack or similar for actual implementation
        # For now, just a placeholder
        import pickle
        return pickle.dumps(self)

    @classmethod
    def from_bytes(cls, data: bytes) -> "RuntimeData":
        """
        Deserialize from bytes.

        Args:
            data: Serialized bytes

        Returns:
            RuntimeData instance
        """
        # This would use msgpack or similar for actual implementation
        # For now, just a placeholder
        import pickle
        return pickle.loads(data)