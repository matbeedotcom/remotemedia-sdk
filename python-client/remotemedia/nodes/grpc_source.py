"""
GRPC Source Node for RemoteMedia Pipeline Streaming.

This node acts as a streaming source that can receive data from gRPC clients
and feed it into a pipeline, similar to how WebRTCStreamSource works.
"""

import asyncio
import logging
import numpy as np
from typing import Any, AsyncGenerator, Optional, Dict, Tuple
import json

from ..core.node import Node

logger = logging.getLogger(__name__)


class GRPCStreamSource(Node):
    """
    Node that serves as a source for gRPC streaming data.
    
    This node receives data from gRPC stream requests and converts them
    to the pipeline's internal format, acting as an adapter between
    the gRPC transport and the pipeline processing system.
    """
    
    def __init__(self, session_id: str = None, **kwargs):
        super().__init__(**kwargs)
        self.session_id = session_id or f"grpc_session_{id(self)}"
        self.is_streaming = True
        self._data_queue = asyncio.Queue(maxsize=500)
        self._is_active = True
        self._received_count = 0
        
    async def initialize(self):
        """Initialize the GRPC stream source."""
        await super().initialize()
        logger.info(f"GRPCStreamSource '{self.name}' initialized for session {self.session_id}")
        
    async def cleanup(self):
        """Clean up the stream source."""
        self._is_active = False
        # Drain the queue
        while not self._data_queue.empty():
            try:
                self._data_queue.get_nowait()
            except asyncio.QueueEmpty:
                break
        await super().cleanup()
        logger.info(f"GRPCStreamSource '{self.name}' cleaned up")
        
    async def add_data(self, data: Any):
        """Add data to the processing queue from the gRPC stream."""
        if not self._is_active:
            logger.warning(f"GRPCStreamSource '{self.name}': Ignoring data, source is not active")
            return
            
        try:
            if not self._data_queue.full():
                await self._data_queue.put(data)
                self._received_count += 1
                logger.debug(f"GRPCStreamSource '{self.name}': Queued data chunk {self._received_count}")
            else:
                # Drop oldest data to make room
                try:
                    self._data_queue.get_nowait()
                    logger.warning(f"GRPCStreamSource '{self.name}': Queue full, dropped oldest data")
                except asyncio.QueueEmpty:
                    pass
                await self._data_queue.put(data)
                self._received_count += 1
        except Exception as e:
            logger.error(f"GRPCStreamSource '{self.name}': Error adding data: {e}")
            
    async def end_stream(self):
        """Signal that the stream has ended."""
        self._is_active = False
        # Add a sentinel value to signal end of stream
        await self._data_queue.put(None)
        logger.info(f"GRPCStreamSource '{self.name}': Stream ended")
        
    async def process(self, data_stream=None) -> AsyncGenerator[Any, None]:
        """Generate data from the gRPC stream."""
        logger.info(f"GRPCStreamSource '{self.name}': Starting to process gRPC stream data")
        
        try:
            processed_count = 0
            while True:
                try:
                    # Wait for data with a timeout to allow for cleanup
                    data = await asyncio.wait_for(self._data_queue.get(), timeout=1.0)
                    
                    # Check for end of stream sentinel
                    if data is None:
                        logger.info(f"GRPCStreamSource '{self.name}': Received end of stream signal")
                        break
                        
                    # Convert the data to pipeline format
                    converted_data = await self._convert_grpc_data(data)
                    if converted_data is not None:
                        processed_count += 1
                        logger.debug(f"GRPCStreamSource '{self.name}': Yielding processed data chunk {processed_count}")
                        yield converted_data
                        
                except asyncio.TimeoutError:
                    # Check if we should continue waiting
                    if not self._is_active and self._data_queue.empty():
                        logger.info(f"GRPCStreamSource '{self.name}': No more data and source inactive")
                        break
                    continue
                except Exception as e:
                    logger.error(f"GRPCStreamSource '{self.name}': Error processing data: {e}")
                    break
                    
            logger.info(f"GRPCStreamSource '{self.name}': Finished processing stream. Total chunks: {processed_count}")
            
        except asyncio.CancelledError:
            logger.debug(f"GRPCStreamSource '{self.name}': Processing cancelled")
        except Exception as e:
            logger.error(f"GRPCStreamSource '{self.name}': Unexpected error in processing: {e}")
            
    async def _convert_grpc_data(self, data: Any) -> Any:
        """Convert gRPC data to pipeline format."""
        try:
            # Handle different types of input data
            if isinstance(data, (dict, list)):
                # Data is already structured
                return self._convert_structured_data(data)
            elif isinstance(data, str):
                # Try to parse as JSON
                try:
                    parsed_data = json.loads(data)
                    return self._convert_structured_data(parsed_data)
                except json.JSONDecodeError:
                    # Treat as plain text
                    return data
            elif isinstance(data, bytes):
                # Try to decode and parse as JSON
                try:
                    decoded_data = data.decode('utf-8')
                    parsed_data = json.loads(decoded_data)
                    return self._convert_structured_data(parsed_data)
                except (UnicodeDecodeError, json.JSONDecodeError):
                    # Return as raw bytes
                    return data
            else:
                # Return as-is for other types
                return data
                
        except Exception as e:
            logger.error(f"GRPCStreamSource '{self.name}': Error converting data: {e}")
            return None
            
    def _convert_structured_data(self, data: Any) -> Any:
        """Convert structured data (dict/list) to appropriate pipeline format."""
        try:
            # Check if it's audio data in tuple format (audio_data, sample_rate)
            if isinstance(data, list) and len(data) == 2:
                audio_data, sample_rate = data
                if isinstance(audio_data, list) and isinstance(sample_rate, (int, float)):
                    # Convert to numpy array
                    audio_array = np.array(audio_data, dtype=np.float32)
                    
                    # Ensure correct shape (channels, samples)
                    if audio_array.ndim == 1:
                        audio_array = audio_array.reshape(1, -1)
                    
                    # Add session metadata
                    metadata = {
                        'session_id': self.session_id,
                        'source': 'grpc'
                    }
                    
                    logger.debug(f"GRPCStreamSource '{self.name}': Converted audio data: "
                               f"shape={audio_array.shape}, sample_rate={sample_rate}")
                    
                    return (audio_array, int(sample_rate), metadata)
            
            # Check if it's a dictionary with audio_data field
            if isinstance(data, dict):
                if 'audio_data' in data and 'sample_rate' in data:
                    audio_data = data['audio_data']
                    sample_rate = data['sample_rate']
                    
                    # Convert to numpy array
                    audio_array = np.array(audio_data, dtype=np.float32)
                    
                    # Ensure correct shape (channels, samples)
                    if audio_array.ndim == 1:
                        audio_array = audio_array.reshape(1, -1)
                    
                    # Add session metadata
                    metadata = {
                        'session_id': self.session_id,
                        'source': 'grpc'
                    }
                    
                    logger.debug(f"GRPCStreamSource '{self.name}': Converted dict audio data: "
                               f"shape={audio_array.shape}, sample_rate={sample_rate}")
                    
                    return (audio_array, int(sample_rate), metadata)
                else:
                    # Return dict as-is, maybe it's other structured data
                    return data
            
            # For other data types, return as-is
            return data
            
        except Exception as e:
            logger.error(f"GRPCStreamSource '{self.name}': Error converting structured data: {e}")
            return data


class GRPCStreamManager:
    """
    Manager for GRPC stream sources.
    
    This class helps manage multiple concurrent gRPC streams and their
    associated source nodes.
    """
    
    def __init__(self):
        self.active_sources: Dict[str, GRPCStreamSource] = {}
        
    def create_source(self, session_id: str, **kwargs) -> GRPCStreamSource:
        """Create a new GRPC stream source for a session."""
        if session_id in self.active_sources:
            logger.warning(f"GRPCStreamManager: Session {session_id} already exists")
            return self.active_sources[session_id]
            
        source = GRPCStreamSource(session_id=session_id, **kwargs)
        self.active_sources[session_id] = source
        logger.info(f"GRPCStreamManager: Created source for session {session_id}")
        return source
        
    def get_source(self, session_id: str) -> Optional[GRPCStreamSource]:
        """Get an existing GRPC stream source."""
        return self.active_sources.get(session_id)
        
    async def cleanup_source(self, session_id: str):
        """Clean up a GRPC stream source."""
        if session_id in self.active_sources:
            source = self.active_sources[session_id]
            await source.cleanup()
            del self.active_sources[session_id]
            logger.info(f"GRPCStreamManager: Cleaned up source for session {session_id}")
            
    async def cleanup_all(self):
        """Clean up all active sources."""
        for session_id in list(self.active_sources.keys()):
            await self.cleanup_source(session_id)


# Global manager instance
_stream_manager = GRPCStreamManager()


def get_grpc_stream_manager() -> GRPCStreamManager:
    """Get the global GRPC stream manager."""
    return _stream_manager