"""
Pipeline Processor for WebRTC streams integration.
"""

import asyncio
import logging
import numpy as np
import fractions
from typing import Dict, Any, Optional, List, Callable
from dataclasses import dataclass
import json

from aiortc import MediaStreamTrack
from aiortc.mediastreams import AudioStreamTrack, VideoStreamTrack
from av import AudioFrame, VideoFrame
import time

from ..core.pipeline import Pipeline
from ..core.node import Node
from ..nodes.source import AudioTrackSource

logger = logging.getLogger(__name__)


class AudioOutputTrack(AudioStreamTrack):
    """
    Audio track that streams generated audio back to WebRTC client.
    """
    
    def __init__(self, sample_rate: int = 24000):
        super().__init__()
        self.sample_rate = sample_rate
        self._audio_queue = asyncio.Queue()
        self._current_audio = None
        self._current_position = 0
        self._start_time = None
        self._cumulative_samples = 0  # Use cumulative samples for PTS instead of frame count
        self._next_frame_time = None  # Track when the next frame should be sent
        
    async def add_audio(self, audio_data: np.ndarray, audio_sample_rate: int):
        """Add generated audio to be streamed."""
        # Resample if needed (simple approach)
        if audio_sample_rate != self.sample_rate:
            # Simple resampling - in production you'd want proper resampling
            ratio = self.sample_rate / audio_sample_rate
            new_length = int(len(audio_data) * ratio)
            audio_data = np.interp(np.linspace(0, len(audio_data), new_length), 
                                 np.arange(len(audio_data)), audio_data)
        
        # Convert to float32
        audio_data = audio_data.astype(np.float32)
        
        # Split large audio into 20ms chunks for proper WebRTC streaming
        frame_samples = int(self.sample_rate * 0.02)  # 20ms frames
        total_frames = (len(audio_data) + frame_samples - 1) // frame_samples  # Ceiling division
        
        duration_s = len(audio_data) / self.sample_rate
        queue_size = self._audio_queue.qsize()
        logger.info(f"📻 AudioOutputTrack: Adding {duration_s:.2f}s of audio ({total_frames} x 20ms frames) to WebRTC stream queue (queue size: {queue_size})")
        
        # Reset PTS timing for new audio to ensure immediate playback
        # This prevents audio from being scheduled far in the future
        self._cumulative_samples = 0
        
        # Reset frame timing for new audio content
        current_time = time.time()
        if self._start_time is None:
            self._start_time = current_time
        self._next_frame_time = current_time  # Start sending immediately
        
        # Split audio into 20ms chunks and queue each chunk
        for i in range(total_frames):
            start_idx = i * frame_samples
            end_idx = min(start_idx + frame_samples, len(audio_data))
            
            chunk = audio_data[start_idx:end_idx]
            
            # Pad the last chunk if it's shorter than frame_samples
            if len(chunk) < frame_samples:
                padding = np.zeros(frame_samples - len(chunk), dtype=np.float32)
                chunk = np.concatenate([chunk, padding])
            
            await self._audio_queue.put(chunk)
        
    async def recv(self):
        """Generate audio frames for WebRTC with proper timing."""
        if self._start_time is None:
            self._start_time = time.time()
            self._next_frame_time = self._start_time
            
        # Frame size: 20ms at the target sample rate
        frame_samples = int(self.sample_rate * 0.02)  # 20ms frames
        frame_duration = 0.02  # 20ms in seconds
        
        # Wait until it's time to send the next frame
        current_time = time.time()
        if self._next_frame_time is None:
            self._next_frame_time = current_time
        
        # If we're ahead of schedule, wait
        if current_time < self._next_frame_time:
            sleep_time = self._next_frame_time - current_time
            await asyncio.sleep(sleep_time)
        
        # Update next frame time
        self._next_frame_time = self._next_frame_time + frame_duration if self._next_frame_time else current_time + frame_duration
        
        # Try to get a 20ms audio frame from the queue
        try:
            audio_data = self._audio_queue.get_nowait()
            remaining_queue_size = self._audio_queue.qsize()
            
            # Log occasionally when streaming frames
            if self._cumulative_samples % (100 * frame_samples) == 0 and remaining_queue_size > 0:  # Log every ~2 seconds
                logger.info(f"📻 AudioOutputTrack: Streaming PTS={self._cumulative_samples} (queue remaining: {remaining_queue_size})")
                
        except asyncio.QueueEmpty:
            # No audio available, send silence
            audio_data = np.zeros(frame_samples, dtype=np.float32)
            # Track silence periods - log occasionally
            if self._cumulative_samples % (200 * frame_samples) == 0:  # Log every ~4 seconds of silence
                logger.debug(f"📻 AudioOutputTrack: Sending silence PTS={self._cumulative_samples} (no audio in queue)")
        
        # Convert float32 audio to int16 for aiortc
        audio_int16 = (audio_data * 32767).astype(np.int16)
        
        # Create AudioFrame
        frame = AudioFrame.from_ndarray(
            audio_int16.reshape(1, -1),  # Shape: (channels, samples)
            format='s16',
            layout='mono'
        )
        frame.sample_rate = self.sample_rate
        
        # Set timing using cumulative samples (like aiortc does)
        frame.pts = self._cumulative_samples
        frame.time_base = fractions.Fraction(1, self.sample_rate)
        self._cumulative_samples += frame.samples
        
        return frame


@dataclass
class StreamMetadata:
    """Metadata for a WebRTC stream."""
    track_id: str
    kind: str  # 'audio' or 'video'
    label: Optional[str] = None
    enabled: bool = True


class WebRTCStreamSource(Node):
    """
    Node that serves as a source for WebRTC audio/video streams.
    
    This node receives frames from WebRTC tracks and converts them
    to the pipeline's internal format.
    """
    
    def __init__(self, track: MediaStreamTrack, connection_id: str = None, **kwargs):
        super().__init__(**kwargs)
        self.track = track
        self.connection_id = connection_id
        self.is_streaming = True
        self._frame_queue = asyncio.Queue(maxsize=500)  # Increased buffer for ML processing delays
        self._processing_task: Optional[asyncio.Task] = None
        
    async def initialize(self):
        """Initialize the stream source."""
        await super().initialize()
        self._processing_task = asyncio.create_task(self._frame_reader())
        
    async def cleanup(self):
        """Clean up the stream source."""
        if self._processing_task:
            self._processing_task.cancel()
            try:
                await self._processing_task
            except asyncio.CancelledError:
                pass
        await super().cleanup()
        
    async def _frame_reader(self):
        """Read frames from the WebRTC track and queue them."""
        try:
            while True:
                try:
                    frame = await self.track.recv()
                    if not self._frame_queue.full():
                        await self._frame_queue.put(frame)
                    else:
                        # Dispose of oldest frame to make room for new one
                        try:
                            self._frame_queue.get_nowait()  # Remove oldest frame
                            logger.debug(f"Frame queue full for track {self.track.id}, disposing oldest frame")
                        except asyncio.QueueEmpty:
                            pass
                        await self._frame_queue.put(frame)  # Add new frame
                except Exception as e:
                    logger.error(f"Error reading frame from track {self.track.id}: {e}")
                    break
        except asyncio.CancelledError:
            logger.debug(f"Frame reader cancelled for track {self.track.id}")
        
    async def process(self, data_stream=None):
        """Generate frames from the WebRTC track."""
        try:
            while True:
                try:
                    frame = await asyncio.wait_for(self._frame_queue.get(), timeout=1.0)
                    converted_frame = await self._convert_frame(frame)
                    if converted_frame is not None:
                        logger.debug(f"WebRTCStreamSource: Generated frame from {self.track.kind} track")
                        yield converted_frame
                except asyncio.TimeoutError:
                    continue
                except Exception as e:
                    logger.error(f"Error processing frame: {e}")
                    break
        except asyncio.CancelledError:
            logger.debug(f"Stream source processing cancelled for track {self.track.id}")
            
    async def _convert_frame(self, frame):
        """Convert WebRTC frame to pipeline format."""
        if isinstance(frame, AudioFrame):
            return await self._convert_audio_frame(frame)
        elif isinstance(frame, VideoFrame):
            return await self._convert_video_frame(frame)
        else:
            logger.warning(f"Unknown frame type: {type(frame)}")
            return None
            
    async def _convert_audio_frame(self, frame: AudioFrame):
        """Convert audio frame to pipeline format."""
        try:
            # Convert to numpy array
            audio_array = frame.to_ndarray()
            
            # Convert to floating-point if it's integer (required by librosa)
            if audio_array.dtype in [np.int16, np.int32]:
                if audio_array.dtype == np.int16:
                    audio_array = audio_array.astype(np.float32) / 32768.0
                elif audio_array.dtype == np.int32:
                    audio_array = audio_array.astype(np.float32) / 2147483648.0
            elif audio_array.dtype != np.float32:
                audio_array = audio_array.astype(np.float32)
            
            # Ensure correct shape (channels, samples)
            if audio_array.ndim == 1:
                audio_array = audio_array.reshape(1, -1)
            elif audio_array.ndim == 2:
                # AudioFrame typically gives (samples, channels), so we need to transpose to (channels, samples)
                # For typical WebRTC audio: (1920, 1) should become (1, 1920)
                if audio_array.shape[0] > audio_array.shape[1]:
                    logger.debug(f"WebRTCStreamSource: Transposing audio from {audio_array.shape} (samples, channels) to (channels, samples)")
                    audio_array = audio_array.T
                    
            # Log audio reception for debugging (reduced frequency)
            duration_s = audio_array.shape[1] / frame.sample_rate if audio_array.ndim > 1 else len(audio_array) / frame.sample_rate
            
            # Check if audio is all zeros
            audio_rms = np.sqrt(np.mean(audio_array**2))
            
            # Only log every 25th frame (~1 second at 40ms frames) or when audio is detected
            if not hasattr(self, '_frame_log_counter'):
                self._frame_log_counter = 0
                self._last_audio_rms = 0.0
            
            self._frame_log_counter += 1
            log_this_frame = (self._frame_log_counter % 25 == 0) or (audio_rms > 0.001 and self._last_audio_rms <= 0.001)
            
            if log_this_frame:
                audio_min = np.min(audio_array)
                audio_max = np.max(audio_array)
                audio_nonzero_count = np.count_nonzero(audio_array)
                logger.debug(f"🎧 WebRTC AUDIO IN: {duration_s:.3f}s @ {frame.sample_rate}Hz | rms={audio_rms:.6f} | nonzero={audio_nonzero_count}/{audio_array.size}")
            
            if audio_rms == 0.0 and self._last_audio_rms > 0.0:
                logger.debug("⚠️  Audio went silent")
            elif audio_rms > 0.0 and self._last_audio_rms == 0.0:
                logger.debug("🎵 Audio detected")
                
            self._last_audio_rms = audio_rms
            
            logger.debug(f"WebRTCStreamSource: Converted audio frame: shape={audio_array.shape}, dtype={audio_array.dtype}, sample_rate={frame.sample_rate}")
            
            # Include connection_id as session_id in metadata for conversation continuity
            metadata = {
                'session_id': self.connection_id,
                'timestamp': frame.pts,
                'source': 'webrtc'
            }
            
            return (audio_array, frame.sample_rate, metadata)
            
        except Exception as e:
            logger.error(f"Error converting audio frame: {e}")
            return None
            
    async def _convert_video_frame(self, frame: VideoFrame):
        """Convert video frame to pipeline format."""
        try:
            # Convert to numpy array (RGB format)
            video_array = frame.to_ndarray(format='rgb24')
            
            return {
                'frame': video_array,
                'width': frame.width,
                'height': frame.height,
                'format': 'rgb24',
                'pts': frame.pts,
                'time_base': frame.time_base,
                'session_id': self.connection_id,
                'source': 'webrtc'
            }
            
        except Exception as e:
            logger.error(f"Error converting video frame: {e}")
            return None


class WebRTCStreamSink(Node):
    """
    Node that serves as a sink for processed data back to WebRTC.
    
    This node receives processed data from the pipeline and can
    send it back through WebRTC data channels or create new tracks.
    """
    
    def __init__(self, connection_id: str, output_callback: Optional[Callable] = None, **kwargs):
        super().__init__(**kwargs)
        self.connection_id = connection_id
        self.output_callback = output_callback
        self.is_streaming = True
        
    async def process(self, data_stream):
        """Process data and forward to WebRTC output."""
        async for data in data_stream:
            try:
                # Send data back through callback if available
                if self.output_callback:
                    await self.output_callback(data)
                    
                # Forward data through pipeline
                yield data
                
            except Exception as e:
                logger.error(f"Error in WebRTC sink: {e}")
                continue


class WebRTCDataChannelNode(Node):
    """
    Node for handling WebRTC data channel messages.
    
    This node can both receive and send data through WebRTC data channels.
    """
    
    def __init__(self, channel_name: str, connection_id: str, **kwargs):
        super().__init__(**kwargs)
        self.channel_name = channel_name
        self.connection_id = connection_id
        self.is_streaming = True
        self._message_queue = asyncio.Queue(maxsize=1000)
        self._send_callback: Optional[Callable] = None
        
    def set_send_callback(self, callback: Callable[[str, Any], None]):
        """Set callback for sending data through the data channel."""
        self._send_callback = callback
        
    async def add_message(self, message: Any):
        """Add a message to the processing queue."""
        if not self._message_queue.full():
            await self._message_queue.put(message)
        else:
            logger.warning(f"Message queue full for channel {self.channel_name}")
            
    async def process(self, data_stream):
        """Process data channel messages and pipeline data."""
        # Create concurrent tasks for both data sources
        async def message_generator():
            while True:
                try:
                    message = await asyncio.wait_for(self._message_queue.get(), timeout=0.1)
                    yield {"type": "datachannel", "channel": self.channel_name, "data": message}
                except asyncio.TimeoutError:
                    continue
                    
        async def pipeline_data_processor():
            async for data in data_stream:
                # Send processed data back through data channel if callback is set
                if self._send_callback:
                    try:
                        serialized_data = json.dumps(data) if isinstance(data, (dict, list)) else str(data)
                        self._send_callback(self.channel_name, serialized_data)
                    except Exception as e:
                        logger.error(f"Error sending data through channel {self.channel_name}: {e}")
                
                yield data
                
        # Process both streams concurrently
        async for item in self._merge_streams(message_generator(), pipeline_data_processor()):
            yield item
            
    async def _merge_streams(self, *streams):
        """Merge multiple async generators."""
        tasks = [asyncio.create_task(stream.__anext__()) for stream in streams]
        
        while tasks:
            done, pending = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
            
            for task in done:
                try:
                    result = task.result()
                    yield result
                    
                    # Get the stream index and create a new task for it
                    stream_idx = tasks.index(task)
                    tasks[stream_idx] = asyncio.create_task(streams[stream_idx].__anext__())
                    
                except StopAsyncIteration:
                    # Remove completed stream
                    stream_idx = tasks.index(task)
                    tasks.remove(task)
                    streams = streams[:stream_idx] + streams[stream_idx+1:]
                except Exception as e:
                    logger.error(f"Error in stream merge: {e}")
                    stream_idx = tasks.index(task)
                    tasks.remove(task)
                    streams = streams[:stream_idx] + streams[stream_idx+1:]


class WebRTCPipelineProcessor:
    """
    Main processor that integrates WebRTC streams with RemoteMedia pipelines.
    
    This class manages the connection between WebRTC tracks/data channels
    and the pipeline processing system.
    """
    
    def __init__(self, pipeline: Pipeline, connection_id: str):
        self.pipeline = pipeline
        self.connection_id = connection_id
        self.active_tracks: Dict[str, MediaStreamTrack] = {}
        self.stream_sources: Dict[str, WebRTCStreamSource] = {}
        self.data_channels: Dict[str, WebRTCDataChannelNode] = {}
        self.stream_sink: Optional[WebRTCStreamSink] = None
        self.audio_output_track: Optional[AudioOutputTrack] = None
        self._processing_task: Optional[asyncio.Task] = None
        self._initialized = False
        
    async def initialize(self):
        """Initialize the pipeline processor."""
        if self._initialized:
            return
            
        # Create audio output track for sending generated audio back to client
        self.audio_output_track = AudioOutputTrack(sample_rate=24000)
        
        # Add a stream sink to the pipeline for output handling
        self.stream_sink = WebRTCStreamSink(
            connection_id=self.connection_id,
            output_callback=self._handle_pipeline_output,
            name=f"WebRTCSink_{self.connection_id}"
        )
        
        # Don't start processing until tracks are added
        self._initialized = True
        
        logger.info(f"Pipeline processor initialized for connection {self.connection_id}")
        
    async def cleanup(self):
        """Clean up the pipeline processor."""
        if self._processing_task:
            self._processing_task.cancel()
            try:
                await self._processing_task
            except asyncio.CancelledError:
                pass
                
        # Clean up stream sources
        for source in self.stream_sources.values():
            await source.cleanup()
        self.stream_sources.clear()
        
        # Clean up data channels
        self.data_channels.clear()
        
        # Clean up pipeline
        if hasattr(self.pipeline, 'cleanup'):
            await self.pipeline.cleanup()
            
        self._initialized = False
        logger.info(f"Pipeline processor cleaned up for connection {self.connection_id}")
        
    async def add_track(self, track: MediaStreamTrack):
        """Add a WebRTC track to the pipeline."""
        track_id = f"{track.kind}_{track.id}"
        self.active_tracks[track_id] = track
        
        # Only process audio tracks for now
        if track.kind == "audio":
            # Create a stream source for this track
            source = WebRTCStreamSource(
                track=track,
                connection_id=self.connection_id,
                name=f"WebRTCSource_{track_id}"
            )
            self.stream_sources[track_id] = source
            
            # Insert the source at the beginning of the pipeline
            # We need to restructure the pipeline to have WebRTC source as the first node
            nodes = list(self.pipeline.nodes)
            self.pipeline.nodes.clear()
            
            # Add WebRTC source first
            self.pipeline.add_node(source)
            
            # Add back the original nodes
            for node in nodes:
                if node != self.stream_sink:  # Don't add sink yet
                    self.pipeline.add_node(node)
            
            await source.initialize()
            
            # Start pipeline processing if this is the first track and we haven't started yet
            if not self._processing_task and self._initialized:
                self._processing_task = asyncio.create_task(self._process_pipeline())
                logger.info(f"Started pipeline processing for connection {self.connection_id}")
            
            logger.info(f"Added {track.kind} track {track.id} to pipeline for connection {self.connection_id}")
        else:
            logger.info(f"Ignoring {track.kind} track {track.id} (only audio processing supported)")
        
    async def remove_track(self, track: MediaStreamTrack):
        """Remove a WebRTC track from the pipeline."""
        track_id = f"{track.kind}_{track.id}"
        
        if track_id in self.active_tracks:
            del self.active_tracks[track_id]
            
        if track_id in self.stream_sources:
            await self.stream_sources[track_id].cleanup()
            del self.stream_sources[track_id]
            
        logger.info(f"Removed {track.kind} track {track.id} from pipeline for connection {self.connection_id}")
        
    async def process_data_message(self, channel_name: str, message: Any):
        """Process a data channel message through the pipeline."""
        if channel_name not in self.data_channels:
            # Create a new data channel node
            channel_node = WebRTCDataChannelNode(
                channel_name=channel_name,
                connection_id=self.connection_id,
                name=f"DataChannel_{channel_name}_{self.connection_id}"
            )
            self.data_channels[channel_name] = channel_node
            
            # Only add to pipeline if it's not initialized yet
            try:
                self.pipeline.add_node(channel_node)
            except Exception as e:
                logger.debug(f"Could not add data channel node to initialized pipeline: {e}")
                # Handle data channel messages directly without adding to pipeline
            
        # Add message to the channel's queue
        if channel_name in self.data_channels:
            await self.data_channels[channel_name].add_message(message)
        
    async def _process_pipeline(self):
        """Process the pipeline continuously."""
        try:
            # Add the sink node to the end of the pipeline
            self.pipeline.add_node(self.stream_sink)
            
            # Start pipeline processing
            logger.info(f"🚀 Starting pipeline processing for connection {self.connection_id}")
            async with self.pipeline.managed_execution():
                async for result in self.pipeline.process():
                    logger.info(f"🔄 Pipeline result for {self.connection_id}: {type(result)}")
                    
        except asyncio.CancelledError:
            logger.debug(f"Pipeline processing cancelled for connection {self.connection_id}")
        except Exception as e:
            logger.error(f"Error in pipeline processing for {self.connection_id}: {e}")
            
    async def _handle_pipeline_output(self, data: Any):
        """Handle output from the pipeline."""
        try:
            logger.info(f"🔄 Pipeline output for {self.connection_id}: {type(data)}")
            
            # Handle different types of pipeline output
            if isinstance(data, tuple) and len(data) == 2:
                # Check if it's audio data (numpy array, sample_rate)
                audio_data, sample_rate = data
                if isinstance(audio_data, np.ndarray):
                    logger.info(f"🎵 Processing audio output: shape={audio_data.shape}, sample_rate={sample_rate}")
                    await self._send_audio_to_client(audio_data, sample_rate)
                else:
                    logger.info(f"🔄 Non-audio tuple output: {type(audio_data)}, {type(sample_rate)}")
            elif isinstance(data, str):
                # Handle text responses
                logger.info(f"📝 Processing text output: '{data[:50]}{'...' if len(data) > 50 else ''}'")
                await self._send_text_to_client(data)
            elif isinstance(data, dict):
                # Handle structured data
                logger.info(f"📊 Processing dict output: {list(data.keys()) if hasattr(data, 'keys') else data}")
                await self._send_data_to_client(data)
            else:
                logger.info(f"❓ Unknown pipeline output type: {type(data)} - {data}")
            
        except Exception as e:
            logger.error(f"Error handling pipeline output for {self.connection_id}: {e}")
            
    async def _send_audio_to_client(self, audio_data: np.ndarray, sample_rate: int):
        """Send generated audio back to the WebRTC client via audio track."""
        try:
            if self.audio_output_track is None:
                logger.warning(f"No audio output track available for {self.connection_id}")
                return
                
            # Convert audio data to proper format
            if audio_data.ndim > 1:
                audio_flat = audio_data.flatten()
            else:
                audio_flat = audio_data
                
            duration_s = len(audio_flat) / sample_rate
            
            # Check for audio issues
            if len(audio_flat) == 0:
                logger.warning(f"⚠️ Received empty audio data for {self.connection_id}")
                return
                
            # Analyze audio characteristics
            audio_min = np.min(audio_flat)
            audio_max = np.max(audio_flat)
            audio_rms = np.sqrt(np.mean(audio_flat**2))
            nonzero_count = np.count_nonzero(audio_flat)
            
            logger.info(f"🔊 TTS->WebRTC: {duration_s:.2f}s @ {sample_rate}Hz | "
                       f"min={audio_min:.4f} max={audio_max:.4f} rms={audio_rms:.4f} | "
                       f"nonzero={nonzero_count}/{len(audio_flat)}")
            
            if audio_rms == 0.0:
                logger.warning(f"⚠️ TTS audio is silent (all zeros) for {self.connection_id}")
            
            # Track total audio sent for this response  
            if not hasattr(self, '_current_tts_chunks'):
                self._current_tts_chunks = 0
                self._current_tts_duration = 0.0
            
            self._current_tts_chunks += 1
            self._current_tts_duration += duration_s
            
            # Save TTS audio to file for debugging
            await self._save_tts_audio_debug(audio_flat, sample_rate, self._current_tts_chunks)
                
            # Add audio to the output track
            await self.audio_output_track.add_audio(audio_flat, sample_rate)
            
            logger.info(f"✅ TTS Chunk #{self._current_tts_chunks}: {duration_s:.2f}s queued for WebRTC client {self.connection_id} | Total: {self._current_tts_duration:.2f}s")
            
        except Exception as e:
            logger.error(f"❌ Error sending audio to client {self.connection_id}: {e}")
            
    async def _save_tts_audio_debug(self, audio_data: np.ndarray, sample_rate: int, chunk_num: int):
        """Save TTS audio to file for debugging purposes."""
        try:
            import os
            from pathlib import Path
            import time
            
            # Create debug directory
            debug_dir = Path("generated_responses")
            debug_dir.mkdir(exist_ok=True)
            
            # Generate filename with timestamp and chunk number
            timestamp = int(time.time())
            filename = f"tts_chunk_{timestamp}_{chunk_num}_{len(audio_data)}samples_{sample_rate}hz.wav"
            filepath = debug_dir / filename
            
            # Save as WAV file
            try:
                import soundfile as sf
                sf.write(str(filepath), audio_data, sample_rate)
                logger.info(f"💾 Saved TTS audio debug file: {filepath} ({len(audio_data)/sample_rate:.2f}s)")
            except ImportError:
                # Fallback: save as numpy array if soundfile not available
                np.save(str(filepath.with_suffix('.npy')), audio_data)
                logger.info(f"💾 Saved TTS audio debug file (numpy): {filepath.with_suffix('.npy')} ({len(audio_data)/sample_rate:.2f}s)")
                
        except Exception as e:
            logger.error(f"❌ Failed to save TTS audio debug file: {e}")
            
    async def _send_text_to_client(self, text: str):
        """Send text response to the WebRTC client."""
        try:
            # Reset TTS tracking for new response
            self._current_tts_chunks = 0
            self._current_tts_duration = 0.0
            
            text_message = {
                "type": "text_response",
                "text": text,
                "timestamp": asyncio.get_event_loop().time()
            }
            await self._send_data_to_client(text_message)
            logger.info(f"📝 Starting TTS for response: '{text[:50]}{'...' if len(text) > 50 else ''}' to {self.connection_id}")
            
        except Exception as e:
            logger.error(f"Error sending text to client {self.connection_id}: {e}")
            
    async def _send_data_to_client(self, data: dict):
        """Send data to client via data channels."""
        try:
            import json
            
            # Try to send via any available data channel
            for channel_name, channel_node in self.data_channels.items():
                if hasattr(channel_node, '_send_callback') and channel_node._send_callback:
                    serialized_data = json.dumps(data)
                    channel_node._send_callback(channel_name, serialized_data)
                    logger.debug(f"Sent data via channel '{channel_name}' to {self.connection_id}")
                    return
                    
            # If no data channels available, log the data
            logger.warning(f"No data channels available for {self.connection_id}, logging data: {data}")
            
        except Exception as e:
            logger.error(f"Error sending data to client {self.connection_id}: {e}")
            
    def get_stats(self) -> Dict[str, Any]:
        """Get statistics about the pipeline processor."""
        return {
            "connection_id": self.connection_id,
            "active_tracks": len(self.active_tracks),
            "stream_sources": len(self.stream_sources),
            "data_channels": len(self.data_channels),
            "initialized": self._initialized,
            "pipeline_nodes": len(self.pipeline.nodes) if hasattr(self.pipeline, 'nodes') else 0
        }