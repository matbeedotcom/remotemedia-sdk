"""
Streaming methods that work with the current RemoteProxyClient.
"""

import asyncio
from typing import Any, List, Optional, AsyncIterator, Callable
from dataclasses import dataclass


@dataclass
class StreamConfig:
    """Configuration for streaming operations."""
    batch_size: int = 10
    timeout: float = 30.0
    continue_on_error: bool = False


class StreamingMixin:
    """
    Mixin class that adds streaming capabilities to any class.
    Works with the current RemoteProxyClient without modifications.
    """
    
    def __init__(self):
        self._stream_buffers = {}
        self._stream_positions = {}
        self._stream_complete = {}
    
    def stream_init(self, stream_id: str, generator_method: str, *args, **kwargs) -> dict:
        """Initialize a stream by calling the generator method and storing results."""
        method = getattr(self, generator_method)
        
        # Check if it's an async generator
        if asyncio.iscoroutinefunction(method):
            # This would need to be called in an async context on the server
            # For now, we'll handle sync generators
            raise NotImplementedError("Async generators need special handling")
        
        # Materialize the generator
        results = list(method(*args, **kwargs))
        
        self._stream_buffers[stream_id] = results
        self._stream_positions[stream_id] = 0
        self._stream_complete[stream_id] = False
        
        return {
            "stream_id": stream_id,
            "total_items": len(results),
            "ready": True
        }
    
    def stream_next_batch(self, stream_id: str, batch_size: int = 10) -> dict:
        """Get the next batch of items from a stream."""
        if stream_id not in self._stream_buffers:
            return {"error": "Stream not found", "items": []}
        
        buffer = self._stream_buffers[stream_id]
        pos = self._stream_positions[stream_id]
        
        # Get next batch
        next_pos = min(pos + batch_size, len(buffer))
        items = buffer[pos:next_pos]
        
        self._stream_positions[stream_id] = next_pos
        
        # Check if complete
        is_complete = next_pos >= len(buffer)
        if is_complete:
            self._stream_complete[stream_id] = True
        
        return {
            "items": items,
            "has_more": not is_complete,
            "position": next_pos,
            "total": len(buffer)
        }
    
    def stream_close(self, stream_id: str) -> dict:
        """Close and cleanup a stream."""
        if stream_id in self._stream_buffers:
            del self._stream_buffers[stream_id]
            del self._stream_positions[stream_id]
            del self._stream_complete[stream_id]
            return {"closed": True}
        return {"closed": False, "error": "Stream not found"}
    
    async def async_stream_init(self, stream_id: str, generator_method: str, *args, **kwargs) -> dict:
        """Initialize an async stream."""
        method = getattr(self, generator_method)
        
        # Materialize the async generator
        results = []
        async for item in method(*args, **kwargs):
            results.append(item)
        
        self._stream_buffers[stream_id] = results
        self._stream_positions[stream_id] = 0
        self._stream_complete[stream_id] = False
        
        return {
            "stream_id": stream_id,
            "total_items": len(results),
            "ready": True
        }


class RemoteStreamIterator:
    """
    Client-side iterator that fetches batches from remote stream.
    """
    
    def __init__(self, remote_obj, stream_id: str, batch_size: int = 10):
        self.remote_obj = remote_obj
        self.stream_id = stream_id
        self.batch_size = batch_size
        self.current_batch = []
        self.current_index = 0
        self.has_more = True
    
    def __aiter__(self):
        return self
    
    async def __anext__(self):
        # If we've exhausted the current batch, fetch more
        if self.current_index >= len(self.current_batch):
            if not self.has_more:
                # Cleanup the stream
                await self.remote_obj.stream_close(self.stream_id)
                raise StopAsyncIteration
            
            # Fetch next batch
            result = await self.remote_obj.stream_next_batch(self.stream_id, self.batch_size)
            self.current_batch = result["items"]
            self.current_index = 0
            self.has_more = result["has_more"]
            
            if not self.current_batch:
                # No more items
                await self.remote_obj.stream_close(self.stream_id)
                raise StopAsyncIteration
        
        # Return next item from current batch
        item = self.current_batch[self.current_index]
        self.current_index += 1
        return item


# Helper function to create streaming iterator
def stream_from_remote(remote_obj, generator_method: str, *args, batch_size: int = 10, **kwargs):
    """
    Create a streaming iterator from a remote generator method.
    
    Usage:
        async for item in stream_from_remote(remote_obj, 'generate_data', arg1, arg2):
            process(item)
    """
    import uuid
    stream_id = str(uuid.uuid4())
    
    class AsyncStreamContext:
        """Async context that initializes stream and returns iterator."""
        
        def __init__(self, remote_obj, stream_id, generator_method, args, kwargs, batch_size):
            self.remote_obj = remote_obj
            self.stream_id = stream_id
            self.generator_method = generator_method
            self.args = args
            self.kwargs = kwargs
            self.batch_size = batch_size
        
        async def __aenter__(self):
            # Initialize the stream
            if hasattr(self.remote_obj._obj, self.generator_method):
                method = getattr(self.remote_obj._obj, self.generator_method)
                if asyncio.iscoroutinefunction(method):
                    await self.remote_obj.async_stream_init(self.stream_id, self.generator_method, *self.args, **self.kwargs)
                else:
                    await self.remote_obj.stream_init(self.stream_id, self.generator_method, *self.args, **self.kwargs)
            else:
                await self.remote_obj.stream_init(self.stream_id, self.generator_method, *self.args, **self.kwargs)
            
            # Return iterator
            return RemoteStreamIterator(self.remote_obj, self.stream_id, self.batch_size)
        
        async def __aexit__(self, exc_type, exc_val, exc_tb):
            # Cleanup if needed
            pass
        
        def __aiter__(self):
            # For direct iteration without context manager
            return self.__aenter__()
    
    return AsyncStreamContext(remote_obj, stream_id, generator_method, args, kwargs, batch_size)


# Example usage class
class StreamingDataProcessor(StreamingMixin):
    """Example class that uses the streaming mixin."""
    
    def __init__(self):
        super().__init__()
        self.chunk_size = 1024
    
    def read_large_file(self, filename: str, chunks: int = 100):
        """Generator that simulates reading a large file."""
        for i in range(chunks):
            yield f"Chunk {i+1}/{chunks} from {filename} (bytes {i*self.chunk_size}-{(i+1)*self.chunk_size})"
    
    async def process_data_stream(self, count: int = 50):
        """Async generator that simulates data processing."""
        for i in range(count):
            await asyncio.sleep(0.01)  # Simulate processing
            yield {"index": i, "value": i * 2.5, "status": "processed"}