"""
Remote Execution Proxy Client that transparently captures and executes methods remotely.
"""

import asyncio
import inspect
import logging
from typing import Any, Optional, Dict, List
from functools import wraps

from .client import RemoteExecutionClient
from ..core.node import RemoteExecutorConfig
from ..packaging.code_packager import CodePackager
from .generator_proxy import RemoteGeneratorProxy, BatchedRemoteGeneratorProxy

logger = logging.getLogger(__name__)


class RemoteProxy:
    """
    Proxy wrapper that captures method calls and executes them remotely.
    """
    
    def __init__(self, client: 'RemoteProxyClient', obj: Any, session_id: str):
        self._client = client
        self._obj = obj
        self._session_id = session_id
    
    def __getattr__(self, name: str) -> Any:
        """
        Capture attribute access and create remote method wrappers.
        """
        # Check if the attribute exists on the original object
        if not hasattr(self._obj, name):
            raise AttributeError(f"'{type(self._obj).__name__}' object has no attribute '{name}'")
        
        attr = getattr(self._obj, name)
        
        # If it's a method, wrap it for remote execution
        if callable(attr):
            # Check if it's a generator or async generator function
            is_generator = inspect.isgeneratorfunction(attr)
            is_async_generator = inspect.isasyncgenfunction(attr)
            
            @wraps(attr)
            async def remote_method(*args, **kwargs):
                """Execute the method remotely."""
                # Keep args and kwargs separate for proper remote execution
                method_args = list(args)
                
                result = await self._client._execute_remote_method(
                    session_id=self._session_id,
                    method_name=name,
                    method_args=method_args,
                    method_kwargs=kwargs if kwargs else None
                )
                
                # Check if the result is a generator marker
                if isinstance(result, dict) and result.get("__generator__"):
                    # Return a generator proxy instead of the marker
                    generator_id = result["generator_id"]
                    is_async = result.get("is_async", False)
                    
                    # Use batched proxy for better performance
                    return BatchedRemoteGeneratorProxy(
                        self._client.client,  # Access the underlying RemoteExecutionClient
                        generator_id,
                        batch_size=10,
                        is_async=is_async
                    )
                
                return result
            
            # Add metadata to help users understand what happened
            if is_generator:
                remote_method._is_generator_proxy = True
                remote_method._original_is_generator = True
            elif is_async_generator:
                remote_method._is_async_generator_proxy = True
                remote_method._original_is_async_generator = True
            
            # Always return async wrapper - caller should await
            return remote_method
        else:
            # For non-callable attributes (properties), we need to fetch them remotely
            # Return a coroutine that fetches the property value
            async def get_property():
                """Fetch property value remotely."""
                result = await self._client._execute_remote_method(
                    session_id=self._session_id,
                    method_name=name,  # Server will detect this is a property
                    method_args=[]
                )
                return result
            
            # Return the coroutine so it can be awaited
            return get_property()
    
    def __repr__(self):
        return f"<RemoteProxy({type(self._obj).__name__}) at {self._session_id}>"


class RemoteProxyClient:
    """
    Client that creates proxy objects for transparent remote execution.
    
    Usage:
        async with RemoteProxyClient(config) as client:
            # Create a remote proxy for any object
            counter = Counter()
            remote_counter = await client.create_proxy(counter)
            
            # All method calls are automatically executed remotely
            await remote_counter.increment()
            value = await remote_counter.get_value()
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
        # Execute initial object creation remotely and get session ID
        # We need to send the object to the server and establish a session
        # Using execute_object_method which handles the code packaging internally
        # We'll use __init__ as a no-op since the object is already initialized
        result = await self.client.execute_object_method(
            obj=obj,
            method_name="__init__",  # Use __init__ as a safe method that exists
            method_args=[],
            serialization_format=serialization_format,
            dependencies=self.config.pip_packages
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
        Internal method to execute a method remotely using an existing session.
        
        Args:
            session_id: The session ID for the remote object
            method_name: Name of the method to execute
            method_args: Arguments for the method
            method_kwargs: Keyword arguments for the method
            serialization_format: Serialization format to use
            
        Returns:
            The result of the remote method execution
        """
        result = await self.client.execute_object_method(
            obj=None,  # We're using session_id instead
            method_name=method_name,
            method_args=method_args,
            method_kwargs=method_kwargs,
            serialization_format=serialization_format,
            session_id=session_id
        )
        
        return result["result"]
    
    async def __aenter__(self):
        """Async context manager entry."""
        await self.connect()
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.disconnect()


def remote_class(cls):
    """
    Decorator that automatically creates remote proxies for a class.
    
    Usage:
        @remote_class
        class MyProcessor:
            def process(self, data):
                return expensive_operation(data)
        
        # When instantiated with a RemoteProxyClient, it's automatically remote
        async with RemoteProxyClient(config) as client:
            processor = MyProcessor(_remote_client=client)
            result = await processor.process(data)
    """
    original_init = cls.__init__
    
    @wraps(original_init)
    def new_init(self, *args, _remote_client=None, **kwargs):
        if _remote_client is not None:
            # Create a temporary instance for proxying
            temp_instance = object.__new__(cls)
            original_init(temp_instance, *args, **kwargs)
            
            # Create remote proxy (this is async, so we store it for later)
            self._remote_client = _remote_client
            self._temp_instance = temp_instance
            self._proxy_future = asyncio.create_task(
                _remote_client.create_proxy(temp_instance)
            )
        else:
            # Normal local initialization
            original_init(self, *args, **kwargs)
    
    def __getattribute__(self, name):
        # Check if this is a remote instance
        try:
            remote_client = object.__getattribute__(self, '_remote_client')
            if remote_client is not None:
                proxy_future = object.__getattribute__(self, '_proxy_future')
                
                # Get the proxy (will wait if not ready)
                if asyncio.iscoroutine(proxy_future):
                    proxy = asyncio.get_event_loop().run_until_complete(proxy_future)
                else:
                    proxy = proxy_future
                
                # Forward to proxy
                return getattr(proxy, name)
        except AttributeError:
            pass
        
        # Normal attribute access
        return object.__getattribute__(self, name)
    
    cls.__init__ = new_init
    cls.__getattribute__ = __getattribute__
    
    return cls