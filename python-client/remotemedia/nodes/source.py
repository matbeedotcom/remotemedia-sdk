"""
Source nodes for the RemoteMedia SDK.

Source nodes are responsible for introducing data into the pipeline,
for example by reading from a file, network stream, or hardware device.
"""
import asyncio
import logging
import os
from typing import AsyncGenerator, Any, Union, TypedDict, Optional, Tuple

import numpy as np
from av import AudioFrame, VideoFrame
from av.frame import Frame

from ..core.node import Node
from ..core.exceptions import NodeError

logger = logging.getLogger(__name__)


# Type definitions for TrackSource nodes
TrackSourceInput = Any
TrackSourceOutput = Optional[Any]


class TrackSourceError(TypedDict):
    """Error output structure for TrackSource."""
    error: str
    input: Any
    processed_by: str

class MediaReaderNode(Node):
    """
    A node that reads media from a local file or URL and yields frames.

    This node is a "source" and is not meant to be used in the middle of a
    pipeline. Its purpose is to generate a stream of data to be fed into
    a pipeline.
    """

    def __init__(self, path: str, **kwargs):
        """
        Initialize the media source node.

        Args:
            path: Path to the media file or URL
            **kwargs: Additional node parameters
        """
        super().__init__(path=path, **kwargs)
        self.path = path

    def process(self, data: Optional[Any] = None) -> AsyncGenerator[Any, None]:
        """
        Ignores any input data and returns an async generator of media frames.
        """
        return self.stream()

    async def stream(self) -> AsyncGenerator[Any, None]:
        """
        Asynchronously yields frames from the media source.
        """
        try:
            from aiortc.contrib.media import MediaPlayer
            from aiortc.mediastreams import MediaStreamError
        except ImportError:
            raise NodeError("PyAV and aiortc are required for MediaReaderNode. Please install them.")

        player = MediaPlayer(self.path, loop=False)
        
        if player.audio is None and player.video is None:
            logger.warning("No audio or video tracks found in the source.")
            return

        # We can create a simple multiplexer here to yield frames as they come
        queue = asyncio.Queue()
        _sentinel = object()

        async def track_reader(track, track_type):
            logger.info(f"Starting reader for {track_type} track.")
            try:
                while True:
                    try:
                        frame = await track.recv()
                        logger.debug(f"Received {track_type} frame: {frame.pts}")
                        await queue.put({track_type: frame})
                    except MediaStreamError:
                        logger.info(f"{track_type} track finished.")
                        break
            finally:
                await queue.put(_sentinel)

        tasks = []
        if player.audio:
            tasks.append(asyncio.create_task(track_reader(player.audio, 'audio')))
        if player.video:
            tasks.append(asyncio.create_task(track_reader(player.video, 'video')))
        
        finished_tracks = 0
        while finished_tracks < len(tasks):
            item = await queue.get()
            if item is _sentinel:
                finished_tracks += 1
                logger.debug(f"A track reader finished. {finished_tracks}/{len(tasks)} done.")
            else:
                logger.debug(f"MediaReaderNode: Yielding frame from queue.")
                yield item
        logger.info("All track readers have finished.")


class TrackSource(Node):
    """
    Base class for track source nodes that extract a specific track from a
    stream of mixed-media dictionaries.
    """
    # Subclasses should override these
    _track_type: str = ""
    _frame_type: type = Frame

    def process(self, data: Any) -> Union[Any, None, TrackSourceError]:
        """
        Processes input data, expecting a dictionary like `{'audio': frame}`.
        It extracts the frame for the specific track type and processes it.
        """
        try:
            if not isinstance(data, dict) or self._track_type not in data:
                # Not the data this track is looking for, ignore silently.
                return None

            frame = data[self._track_type]

            if not isinstance(frame, self._frame_type):
                logger.warning(
                    f"{self.__class__.__name__} '{self.name}': received data for track "
                    f"'{self._track_type}' with unexpected frame type {type(frame)}."
                )
                return None
            
            logger.debug(f"{self.__class__.__name__}: Processing frame.")
            return self._process_frame(frame)
        except Exception as e:
            logger.error(f"{self.__class__.__name__} '{self.name}': processing failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"{self.__class__.__name__}[{self.name}]"
            }

    def _process_frame(self, frame: Frame) -> Any:
        raise NotImplementedError


class AudioTrackSource(TrackSource):
    """
    An audio track source node that converts `av.AudioFrame` objects into NumPy arrays.
    """
    _track_type = "audio"
    _frame_type = AudioFrame

    def _process_frame(self, frame: AudioFrame) -> Any:
        """
        Converts an `av.AudioFrame` to a tuple of (audio_data, sample_rate).

        Args:
            frame: An `av.AudioFrame`.

        Returns:
            A tuple `(audio_data, sample_rate)` where `audio_data` is a
            NumPy array with shape (channels, samples).
        """
        try:
            audio_data = frame.to_ndarray()

            # PyAV returns interleaved audio as shape (1, samples*channels)
            # We need to reshape it to (channels, samples)
            num_channels = len(frame.layout.channels)
            if audio_data.shape[0] == 1 and num_channels > 1:
                # Reshape from (1, samples*channels) to (channels, samples)
                total_samples = audio_data.shape[1]
                samples_per_channel = frame.samples
                audio_data = audio_data.reshape(-1, num_channels).T  # Deinterleave
                logger.debug(
                    f"AudioTrackSource '{self.name}': deinterleaved {num_channels} channels, "
                    f"shape now {audio_data.shape}"
                )

            # Normalize and convert to float32, as expected by librosa
            if audio_data.dtype == np.int16:
                audio_data = audio_data.astype(np.float32) / 32768.0
            elif audio_data.dtype == np.int32:
                audio_data = audio_data.astype(np.float32) / 2147483648.0

            logger.debug(
                f"AudioTrackSource '{self.name}': output shape={audio_data.shape}, "
                f"samples={frame.samples}, rate={frame.sample_rate}Hz, channels={num_channels}"
            )
            return (audio_data, frame.sample_rate)
        except Exception as e:
            logger.error(f"Error converting audio frame to numpy array: {e}")
            return None


class VideoTrackSource(TrackSource):
    """
    A video track source node that converts `av.VideoFrame` objects into NumPy arrays.
    """
    _track_type = "video"
    _frame_type = VideoFrame

    def __init__(self, output_format: str = "bgr24", **kwargs):
        """
        Initializes the VideoTrackSource node.

        Args:
            output_format (str): The desired output format for the NumPy array
                                 (e.g., 'bgr24', 'rgb24').
        """
        super().__init__(**kwargs)
        self.output_format = output_format

    def _process_frame(self, frame: VideoFrame) -> Any:
        """
        Converts an `av.VideoFrame` to a NumPy array.

        Args:
            frame: An `av.VideoFrame`.

        Returns:
            A NumPy array representing the video frame.
        """
        try:
            video_data = frame.to_ndarray(format=self.output_format)
            logger.debug(
                f"VideoTrackSource '{self.name}': processed video frame with "
                f"resolution {frame.width}x{frame.height}."
            )
            return (video_data, frame.pts)
        except Exception as e:
            logger.error(f"Error converting video frame to numpy array: {e}")
            return None


class LocalMediaReaderNode(Node):
    """
    A robust media reader that uses PyAV directly to stream frames from a
    local media file, offering better compatibility than aiortc.MediaPlayer.
    """
    def __init__(self, path: str, **kwargs):
        super().__init__(**kwargs)
        if not os.path.exists(path):
            raise FileNotFoundError(f"Media file not found at path: {path}")
        self.path = path
        self._queue = asyncio.Queue(maxsize=100)
        self._producer_task = None

    async def _produce_frames_blocking(self):
        """
        Internal method that runs in a separate thread to read the file
        and put frames onto the queue, without blocking the main event loop.
        """
        try:
            import av
            container = av.open(self.path)
            video_stream = next((s for s in container.streams if s.type == 'video'), None)
            audio_stream = next((s for s in container.streams if s.type == 'audio'), None)

            if not video_stream and not audio_stream:
                logger.warning(f"No audio or video streams found in '{self.path}'")
                return

            logger.info(f"Producer starting to stream from '{self.path}'...")
            for packet in container.demux(video=0 if video_stream else (), audio=0 if audio_stream else ()):
                for frame in packet.decode():
                    item = None
                    if isinstance(frame, av.VideoFrame):
                        item = {'video': frame}
                    elif isinstance(frame, av.AudioFrame):
                        item = {'audio': frame}
                    
                    if item:
                        # This is a thread-safe way to put items into an asyncio queue
                        asyncio.run_coroutine_threadsafe(self._queue.put(item), asyncio.get_running_loop()).result()

            logger.info("Producer finished streaming file.")
        except Exception as e:
            logger.error(f"Error in media file producer: {e}", exc_info=True)
        finally:
            asyncio.run_coroutine_threadsafe(self._queue.put(None), asyncio.get_running_loop()).result() # Sentinel

    async def initialize(self):
        """Start the producer task in a thread."""
        await super().initialize()
        loop = asyncio.get_running_loop()
        self._producer_task = loop.run_in_executor(
            None, # Use default ThreadPoolExecutor
            self._produce_frames_blocking
        )

    async def process(self, data: Optional[Any] = None) -> AsyncGenerator[Any, None]:
        """Yields frames from the internal queue."""
        if not self._producer_task:
            raise RuntimeError("Producer task was not started. Call initialize() first.")

        while True:
            frame = await self._queue.get()
            if frame is None: # Sentinel reached
                break
            yield frame
            # A small sleep is still good for backpressure
            await asyncio.sleep(0.001)

    async def cleanup(self):
        """Ensure the producer task is handled on cleanup."""
        if self._producer_task and not self._producer_task.done():
            # The producer will finish on its own, but we could cancel if needed
            pass
        await super().cleanup()


__all__ = ["MediaReaderNode", "AudioTrackSource", "VideoTrackSource", "TrackSource", "LocalMediaReaderNode"] 