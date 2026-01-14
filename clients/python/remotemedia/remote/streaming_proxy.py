"""
Streaming-aware Remote Proxy that preserves generator iteration behavior.
"""

import asyncio
import inspect
import logging
import uuid
from typing import Any, AsyncGenerator, Generator, Optional, Dict, List
from functools import wraps

from .client import RemoteExecutionClient
from .proxy_client import RemoteProxyClient, RemoteProxy
from ..core.node import RemoteExecutorConfig
from ..core.exceptions import RemoteExecutionError

logger = logging.getLogger(__name__)


class StreamingRemoteProxy(RemoteProxy):
    """
    Enhanced proxy that supports streaming generators.
    """
    
    def __init__(self, client: 'StreamingRemoteProxyClient', obj: Any, session_id: str):
        super().__init__(client, obj, session_id)
        self._streaming_sessions: Dict[str, str] = {}  # method_name -> stream_session_id
    
    def __getattr__(self, name: str) -> Any:
        """
        Capture attribute access and create appropriate wrappers.
        """
        # Check if the attribute exists on the original object
        if not hasattr(self._obj, name):
            raise AttributeError(f"'{type(self._obj).__name__}' object has no attribute '{name}'")
        
        attr = getattr(self._obj, name)
        
        # Handle properties and non-callable attributes
        if not callable(attr):
            # For properties and attributes, we need to fetch them remotely
            async def get_property():
                """Fetch property value remotely."""
                result = await self._client._execute_remote_method(
                    session_id=self._session_id,
                    method_name=name,
                    method_args=[]
                )
                return result
            return get_property()
        
        # Check if it's a generator function
        if inspect.isgeneratorfunction(attr):
            @wraps(attr)
            def remote_generator(*args, **kwargs):
                """Return a generator that fetches items from remote."""
                return self._client._create_remote_generator(
                    session_id=self._session_id,
                    method_name=name,
                    method_args=list(args),
                    method_kwargs=kwargs,
                    is_async=False
                )
            return remote_generator
        
        # Check if it's an async generator function
        if inspect.isasyncgenfunction(attr):
            @wraps(attr)
            def remote_async_generator(*args, **kwargs):
                """Return an async generator that fetches items from remote."""
                return self._client._create_remote_generator(
                    session_id=self._session_id,
                    method_name=name,
                    method_args=list(args),
                    method_kwargs=kwargs,
                    is_async=True
                )
            return remote_async_generator
        
        # Regular method (sync or async)
        @wraps(attr)
        async def remote_method(*args, **kwargs):
            """Execute the method remotely."""
            return await self._client._execute_remote_method(
                session_id=self._session_id,
                method_name=name,
                method_args=list(args),
                method_kwargs=kwargs
            )
        
        return remote_method


class StreamingRemoteProxyClient(RemoteProxyClient):
    """
    Enhanced proxy client that supports streaming generators.
    """
    
    def __init__(self, config: RemoteExecutorConfig, materialize_generators: bool = False):
        """
        Initialize the streaming proxy client.
        
        Args:
            config: Remote executor configuration
            materialize_generators: If True, generators are materialized to lists (default behavior)
                                  If False, generators maintain streaming behavior
        """
        super().__init__(config)
        self.materialize_generators = materialize_generators
        self._generator_sessions: Dict[str, Dict[str, Any]] = {}  # stream_id -> generator info
    
    async def create_proxy(self, obj: Any, serialization_format: str = "pickle") -> StreamingRemoteProxy:
        """
        Create a streaming-aware proxy object.
        """
        # Use parent's initialization
        result = await self.client.execute_object_method(
            obj=obj,
            method_name="__init__",
            method_args=[],
            serialization_format=serialization_format
        )
        
        session_id = result["session_id"]
        self._sessions[session_id] = obj
        
        return StreamingRemoteProxy(self, obj, session_id)
    
    async def _create_remote_generator(
        self,
        session_id: str,
        method_name: str,
        method_args: List[Any],
        method_kwargs: Optional[Dict[str, Any]] = None,
        is_async: bool = False
    ) -> Any:
        """
        Create a generator that streams from the remote server.
        """
        if self.materialize_generators:
            # Fall back to materialized behavior
            if method_kwargs:
                method_args = method_args + [method_kwargs]
            
            result = await self._execute_remote_method(
                session_id=session_id,
                method_name=method_name,
                method_args=method_args
            )
            
            # Return the materialized list
            if is_async:
                async def list_as_async_gen():
                    for item in result:
                        yield item
                return list_as_async_gen()
            else:
                def list_as_gen():
                    for item in result:
                        yield item
                return list_as_gen()
        else:
            # Create a streaming generator
            stream_id = str(uuid.uuid4())
            
            # Initialize the generator on the server
            init_result = await self._execute_remote_method(
                session_id=session_id,
                method_name=f"_init_generator_{method_name}",
                method_args=method_args + [stream_id]
            )
            
            self._generator_sessions[stream_id] = {
                "session_id": session_id,
                "method_name": method_name,
                "is_async": is_async,
                "active": True
            }
            
            if is_async:
                return self._async_generator_proxy(stream_id)
            else:
                return self._sync_generator_proxy(stream_id)
    
    async def _async_generator_proxy(self, stream_id: str) -> AsyncGenerator[Any, None]:
        """
        Async generator that fetches items from remote server.
        """
        try:
            while self._generator_sessions[stream_id]["active"]:
                # Fetch next item from server
                result = await self._execute_remote_method(
                    session_id=self._generator_sessions[stream_id]["session_id"],
                    method_name="_next_generator_item",
                    method_args=[stream_id]
                )
                
                if result.get("done", False):
                    self._generator_sessions[stream_id]["active"] = False
                    break
                
                yield result["value"]
        finally:
            # Clean up generator session
            if stream_id in self._generator_sessions:
                await self._execute_remote_method(
                    session_id=self._generator_sessions[stream_id]["session_id"],
                    method_name="_close_generator",
                    method_args=[stream_id]
                )
                del self._generator_sessions[stream_id]
    
    def _sync_generator_proxy(self, stream_id: str) -> Generator[Any, None, None]:
        """
        Sync generator that fetches items from remote server.
        Note: This requires running in an event loop context.
        """
        loop = asyncio.get_event_loop()
        
        try:
            while self._generator_sessions[stream_id]["active"]:
                # Fetch next item from server (sync wrapper around async)
                future = asyncio.ensure_future(
                    self._execute_remote_method(
                        session_id=self._generator_sessions[stream_id]["session_id"],
                        method_name="_next_generator_item",
                        method_args=[stream_id]
                    )
                )
                result = loop.run_until_complete(future)
                
                if result.get("done", False):
                    self._generator_sessions[stream_id]["active"] = False
                    break
                
                yield result["value"]
        finally:
            # Clean up generator session
            if stream_id in self._generator_sessions:
                future = asyncio.ensure_future(
                    self._execute_remote_method(
                        session_id=self._generator_sessions[stream_id]["session_id"],
                        method_name="_close_generator",
                        method_args=[stream_id]
                    )
                )
                loop.run_until_complete(future)
                del self._generator_sessions[stream_id]


# Example usage functions that would need server support
STREAMING_SERVER_SUPPORT = """
# Server-side additions needed in ExecuteObjectMethod:

if request.method_name.startswith("_init_generator_"):
    # Initialize a generator and store it
    actual_method_name = request.method_name[16:]  # Remove prefix
    stream_id = method_args[-1]  # Last arg is stream_id
    actual_args = method_args[:-1]  # Remove stream_id from args
    
    method = getattr(obj, actual_method_name)
    if asyncio.iscoroutinefunction(method):
        generator = method(*actual_args)
    else:
        generator = method(*actual_args)
    
    # Store generator in session
    if not hasattr(obj, '_generators'):
        obj._generators = {}
    obj._generators[stream_id] = generator
    
    result = {"initialized": True, "stream_id": stream_id}

elif request.method_name == "_next_generator_item":
    stream_id = method_args[0]
    generator = obj._generators.get(stream_id)
    
    if generator is None:
        result = {"done": True, "error": "Generator not found"}
    else:
        try:
            if inspect.isasyncgen(generator):
                value = await generator.__anext__()
            else:
                value = next(generator)
            result = {"done": False, "value": value}
        except (StopIteration, StopAsyncIteration):
            result = {"done": True}

elif request.method_name == "_close_generator":
    stream_id = method_args[0]
    if hasattr(obj, '_generators') and stream_id in obj._generators:
        generator = obj._generators[stream_id]
        if hasattr(generator, 'aclose'):
            await generator.aclose()
        elif hasattr(generator, 'close'):
            generator.close()
        del obj._generators[stream_id]
    result = {"closed": True}
"""