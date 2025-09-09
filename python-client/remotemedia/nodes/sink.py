import asyncio
import logging
from typing import Any, Dict, Union, TypedDict, Optional

import av
from av.frame import Frame

from ..core.node import Node
from ..core.exceptions import NodeError

logger = logging.getLogger(__name__)


# Type definitions for MediaWriterNode
class MediaWriterInput(TypedDict):
    """Input data structure for MediaWriterNode."""
    audio: Optional[Frame]
    video: Optional[Frame]


class MediaWriterError(TypedDict):
    """Error output structure for MediaWriterNode."""
    error: str
    input: Any
    processed_by: str

class MediaWriterNode(Node):
    """
    A node that writes media frames to a local file.
    This node is a "sink" and is meant to be used at the end of a pipeline.
    It takes av.AudioFrame or av.VideoFrame objects and writes them to a
    media container like MP4 or WAV.
    """

    def __init__(self, output_path: str, **kwargs):
        """
        Initialize the media writer node.
        Args:
            output_path: Path to the output media file.
            **kwargs: Additional node parameters.
        """
        super().__init__(**kwargs)
        self.output_path = output_path
        self.container = None
        self.streams: Dict[str, av.stream.Stream] = {}

    async def initialize(self):
        """
        Opens the output file for writing.
        """
        try:
            # av.open is a blocking call, so run it in a thread pool
            self.container = await asyncio.to_thread(av.open, self.output_path, mode='w')
            logger.info(f"Opened output file for writing: {self.output_path}")
        except Exception as e:
            raise NodeError(f"Failed to open output file {self.output_path}: {e}") from e

    async def process(self, data: Union[Dict[str, Frame], Any]) -> Optional[MediaWriterError]:
        """
        Processes a frame and writes it to the output file.
        Expects data to be a dictionary containing an 'audio' or 'video' key
        with an av.frame.Frame object as the value.
        """
        try:
            if not isinstance(data, dict):
                logger.warning(f"'{self.name}' received data in unexpected format. Expected dict.")
                return {
                    "error": "Input must be a dictionary with 'audio' or 'video' keys",
                    "input": data,
                    "processed_by": f"MediaWriterNode[{self.name}]"
                }

            for track_type, frame in data.items():
                if not isinstance(frame, Frame):
                    continue

                if track_type not in self.streams:
                    await asyncio.to_thread(self._add_stream, track_type, frame)

                await self._write_frame(frame, track_type)
            
            return None  # Success
        except Exception as e:
            logger.error(f"MediaWriterNode '{self.name}': error processing frame: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"MediaWriterNode[{self.name}]"
            }

    def _add_stream(self, track_type: str, frame: Frame):
        """Adds a new stream to the container based on the first frame."""
        logger.info(f"Adding {track_type} stream to '{self.output_path}'.")
        try:
            if track_type == 'video':
                stream = self.container.add_stream("libx264", rate=30)
                stream.width = frame.width
                stream.height = frame.height
                stream.pix_fmt = frame.format.name
                if frame.time_base:
                    stream.time_base = frame.time_base
            elif track_type == 'audio':
                codec_name = 'pcm_s16le' if self.output_path.endswith('.wav') else 'aac'
                stream = self.container.add_stream(codec_name, rate=frame.sample_rate)
                stream.layout = frame.layout.name
                if frame.time_base:
                    stream.time_base = frame.time_base
            else:
                logger.warning(f"Unsupported track type: {track_type}")
                return

            self.streams[track_type] = stream
        except Exception as e:
            raise NodeError(f"Failed to add {track_type} stream: {e}") from e

    async def _write_frame(self, frame: Frame, track_type: str):
        """Encodes and muxes a single frame."""
        if track_type not in self.streams:
            logger.warning(f"No stream available for track type {track_type}. Skipping frame.")
            return
            
        stream = self.streams[track_type]

        def encode_and_mux():
            try:
                packets = stream.encode(frame)
                if packets:
                    self.container.mux(packets)
            except Exception as e:
                logger.error(f"Error during frame encoding/muxing: {e}")

        await asyncio.to_thread(encode_and_mux)
        logger.debug(f"Wrote {track_type} frame (pts={frame.pts})")

    async def cleanup(self):
        """
        Flushes all streams and closes the output file.
        """
        if self.container:
            logger.info(f"Closing output file: {self.output_path}")

            def flush_and_close():
                for track_type, stream in self.streams.items():
                    try:
                        packets = stream.encode()
                        if packets:
                            self.container.mux(packets)
                        logger.debug(f"Flushed {track_type} stream.")
                    except Exception as e:
                        logger.error(f"Failed to flush stream {stream}: {e}")
                self.container.close()

            await asyncio.to_thread(flush_and_close)
            self.container = None
            self.streams = {}

__all__ = ["MediaWriterNode"] 