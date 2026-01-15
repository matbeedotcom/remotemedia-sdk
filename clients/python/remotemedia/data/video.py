"""
Video frame data types and IPC deserialization.

Spec 012: Video Codec Support (AV1/VP8/AVC)
"""

import struct
from dataclasses import dataclass
from enum import IntEnum
from typing import Optional


class PixelFormat(IntEnum):
    """Pixel format enumeration matching Rust PixelFormat"""
    UNSPECIFIED = 0
    YUV420P = 1
    I420 = 2
    NV12 = 3
    RGB24 = 4
    RGBA32 = 5
    ENCODED = 255


class VideoCodec(IntEnum):
    """Video codec enumeration matching Rust VideoCodec"""
    UNSPECIFIED = 0  # Raw frame (no codec)
    VP8 = 1
    H264 = 2
    AV1 = 3


@dataclass
class VideoFrame:
    """
    Video frame data structure matching RuntimeData::Video.

    Supports both raw and encoded video frames with zero-copy IPC deserialization.

    Attributes:
        pixel_data: Raw pixel data or encoded bitstream
        width: Frame width in pixels
        height: Frame height in pixels
        format: Pixel format (PixelFormat enum)
        codec: Video codec (None for raw, VideoCodec for encoded)
        frame_number: Sequential frame number
        timestamp_us: Presentation timestamp in microseconds
        is_keyframe: True for I-frames, False for P/B-frames
    """

    pixel_data: bytes
    width: int
    height: int
    format: PixelFormat
    codec: Optional[VideoCodec]
    frame_number: int
    timestamp_us: int
    is_keyframe: bool

    @classmethod
    def deserialize_ipc(cls, buffer: bytes) -> "VideoFrame":
        """
        Deserialize video frame from iceoryx2 IPC buffer.

        Binary format (from data-model.md):
        ```
        width (4) | height (4) | format (1) | codec (1) |
        frame_number (8) | is_keyframe (1) | pixel_data (variable)
        ```

        Total header: 19 bytes

        Args:
            buffer: IPC payload bytes (after session header)

        Returns:
            VideoFrame instance

        Raises:
            ValueError: If buffer is malformed
        """
        if len(buffer) < 19:
            raise ValueError(f"Video payload too short: {len(buffer)} bytes")

        offset = 0

        # Width (4 bytes, little-endian)
        width = struct.unpack("<I", buffer[offset : offset + 4])[0]
        offset += 4

        # Height (4 bytes, little-endian)
        height = struct.unpack("<I", buffer[offset : offset + 4])[0]
        offset += 4

        # Format (1 byte)
        format_val = buffer[offset]
        pixel_format = PixelFormat(format_val)
        offset += 1

        # Codec (1 byte, 0=None)
        codec_val = buffer[offset]
        codec = VideoCodec(codec_val) if codec_val > 0 else None
        offset += 1

        # Frame number (8 bytes, little-endian)
        frame_number = struct.unpack("<Q", buffer[offset : offset + 8])[0]
        offset += 8

        # Is keyframe (1 byte, 0=false, 1=true)
        is_keyframe = buffer[offset] != 0
        offset += 1

        # Pixel data (remaining bytes - zero-copy view)
        pixel_data = buffer[offset:]

        return cls(
            pixel_data=bytes(pixel_data),  # Convert memoryview to bytes
            width=width,
            height=height,
            format=pixel_format,
            codec=codec,
            frame_number=frame_number,
            timestamp_us=0,  # Timestamp comes from outer RuntimeData wrapper
            is_keyframe=is_keyframe,
        )

    def to_dict(self):
        """Convert to dictionary for JSON serialization"""
        return {
            "width": self.width,
            "height": self.height,
            "format": self.format.name,
            "codec": self.codec.name if self.codec else None,
            "frame_number": self.frame_number,
            "timestamp_us": self.timestamp_us,
            "is_keyframe": self.is_keyframe,
            "data_size_bytes": len(self.pixel_data),
        }

    def __repr__(self):
        codec_str = self.codec.name if self.codec else "RAW"
        return (
            f"VideoFrame({self.width}x{self.height}, "
            f"{self.format.name}, {codec_str}, "
            f"frame={self.frame_number}, keyframe={self.is_keyframe}, "
            f"{len(self.pixel_data)} bytes)"
        )
