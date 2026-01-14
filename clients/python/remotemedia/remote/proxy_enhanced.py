"""
Enhanced Remote Execution Proxy Client that properly handles all Python method types.
"""

import asyncio
import inspect
import logging
from typing import Any, Optional, Dict, List, Union, AsyncGenerator, Generator
from functools import wraps

from .client import RemoteExecutionClient
from ..core.node import RemoteExecutorConfig
from ..core.exceptions import RemoteExecutionError

logger = logging.getLogger(__name__)


class RemoteProxy:
    """
    Enhanced proxy wrapper that properly handles different method types.
    """
    
    def __init__(self, client: 'RemoteProxyClient', obj: Any, session_id: str):
        self._client = client
        self._obj = obj
        self._session_id = session_id
    
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
                return await self._client._get_remote_attribute(
                    session_id=self._session_id,
                    attribute_name=name
                )
            return get_property()
        
        # Check if it's a generator function
        if inspect.isgeneratorfunction(attr):
            @wraps(attr)
            async def remote_generator(*args, **kwargs):
                """Execute generator remotely and return materialized list."""
                return await self._client._execute_remote_generator(
                    session_id=self._session_id,
                    method_name=name,
                    method_args=list(args),
                    method_kwargs=kwargs
                )
            return remote_generator
        
        # Check if it's an async generator function
        if inspect.isasyncgenfunction(attr):
            @wraps(attr)
            async def remote_async_generator(*args, **kwargs):
                """Execute async generator remotely and return materialized list."""
                return await self._client._execute_remote_async_generator(
                    session_id=self._session_id,
                    method_name=name,
                    method_args=list(args),
                    method_kwargs=kwargs
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
    
    def __repr__(self):
        return f"<RemoteProxy({type(self._obj).__name__}) at {self._session_id}>"


class RemoteProxyClient:
    """
    Enhanced client that properly handles all Python method types.
    """
    
    def __init__(self, config: RemoteExecutorConfig):
        """
        Initialize the proxy client.
        
        Args:
            config: Remote executor configuration
        """
        self.config = config
        self.client = RemoteExecutionClient(config)
        self._sessions: Dict[str, Any] = {}  # Track active sessions
    
    async def connect(self) -> None:
        """Connect to the remote execution service."""
        await self.client.connect()
    
    async def disconnect(self) -> None:
        """Disconnect from the remote execution service."""
        await self.client.disconnect()
    
    async def create_proxy(self, obj: Any, serialization_format: str = "pickle") -> RemoteProxy:
        """
        Create a proxy object that executes all methods remotely.
        
        Args:
            obj: The object to create a proxy for
            serialization_format: Serialization format to use
            
        Returns:
            A RemoteProxy that forwards all method calls to the remote service
        """
        # We need a proper initialization method that doesn't rely on properties
        # Let's use a special initialization marker
        result = await self.client.execute_object_method(
            obj=obj,
            method_name="_remote_proxy_init",  # Special marker
            method_args=[],
            serialization_format=serialization_format
        )
        
        session_id = result["session_id"]
        self._sessions[session_id] = obj
        
        return RemoteProxy(self, obj, session_id)
    
    async def _execute_remote_method(
        self,
        session_id: str,
        method_name: str,
        method_args: List[Any],
        method_kwargs: Optional[Dict[str, Any]] = None,
        serialization_format: str = "pickle"
    ) -> Any:
        """
        Execute a regular method remotely.
        """
        # Combine args and kwargs for serialization
        if method_kwargs:
            method_args = method_args + [method_kwargs]
        
        result = await self.client.execute_object_method(
            obj=None,
            method_name=method_name,
            method_args=method_args,
            serialization_format=serialization_format,
            session_id=session_id
        )
        
        return result["result"]
    
    async def _execute_remote_generator(
        self,
        session_id: str,
        method_name: str,
        method_args: List[Any],
        method_kwargs: Optional[Dict[str, Any]] = None,
        serialization_format: str = "pickle"
    ) -> List[Any]:
        """
        Execute a generator method remotely and return materialized list.
        """
        # Add a special marker to indicate this is a generator
        enhanced_method_name = f"_generator_{method_name}"
        return await self._execute_remote_method(
            session_id=session_id,
            method_name=enhanced_method_name,
            method_args=method_args,
            method_kwargs=method_kwargs,
            serialization_format=serialization_format
        )
    
    async def _execute_remote_async_generator(
        self,
        session_id: str,
        method_name: str,
        method_args: List[Any],
        method_kwargs: Optional[Dict[str, Any]] = None,
        serialization_format: str = "pickle"
    ) -> List[Any]:
        """
        Execute an async generator method remotely and return materialized list.
        """
        # Add a special marker to indicate this is an async generator
        enhanced_method_name = f"_async_generator_{method_name}"
        return await self._execute_remote_method(
            session_id=session_id,
            method_name=enhanced_method_name,
            method_args=method_args,
            method_kwargs=method_kwargs,
            serialization_format=serialization_format
        )
    
    async def _get_remote_attribute(
        self,
        session_id: str,
        attribute_name: str,
        serialization_format: str = "pickle"
    ) -> Any:
        """
        Get a remote attribute or property value.
        """
        # Use a special method to get attributes
        return await self._execute_remote_method(
            session_id=session_id,
            method_name="_get_attribute",
            method_args=[attribute_name],
            serialization_format=serialization_format
        )
    
    async def __aenter__(self):
        """Async context manager entry."""
        await self.connect()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.disconnect()


# Server-side handler enhancement needed
SERVER_SIDE_ENHANCEMENT = """
# This would need to be added to the server-side ExecuteObjectMethod handler:

# Check for special method names
if request.method_name == "_remote_proxy_init":
    # Just establish the session, don't call any method
    result = None
elif request.method_name == "_get_attribute":
    # Get attribute value
    attr_name = method_args[0]
    result = getattr(obj, attr_name)
elif request.method_name.startswith("_generator_"):
    # Handle generator by materializing it
    actual_method_name = request.method_name[11:]  # Remove "_generator_" prefix
    method = getattr(obj, actual_method_name)
    generator = method(*method_args)
    result = list(generator)  # Materialize the generator
elif request.method_name.startswith("_async_generator_"):
    # Handle async generator by materializing it
    actual_method_name = request.method_name[17:]  # Remove "_async_generator_" prefix
    method = getattr(obj, actual_method_name)
    async_gen = method(*method_args)
    result = []
    async for item in async_gen:
        result.append(item)
else:
    # Normal method execution
    method = getattr(obj, request.method_name)
    if asyncio.iscoroutinefunction(method):
        result = await method(*method_args)
    else:
        result = method(*method_args)
"""