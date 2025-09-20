"""
Base Node class and remote execution configuration.
"""

from abc import ABC, abstractmethod
from typing import Any, Dict, Optional, Union, TypeVar, Generic, List
from dataclasses import dataclass, field
import logging
import asyncio
from collections import defaultdict
from datetime import datetime, timedelta

from .exceptions import ConfigurationError

logger = logging.getLogger(__name__)

# Type variable for state data
T = TypeVar('T')


@dataclass
class SessionState:
    """Container for session-specific state data."""
    session_id: str
    data: Dict[str, Any] = field(default_factory=dict)
    created_at: datetime = field(default_factory=datetime.now)
    last_accessed: datetime = field(default_factory=datetime.now)
    metadata: Dict[str, Any] = field(default_factory=dict)
    
    def update_access_time(self):
        """Update the last accessed timestamp."""
        self.last_accessed = datetime.now()
    
    def get(self, key: str, default: Any = None) -> Any:
        """Get a value from the state."""
        self.update_access_time()
        return self.data.get(key, default)
    
    def set(self, key: str, value: Any) -> None:
        """Set a value in the state."""
        self.update_access_time()
        self.data[key] = value
    
    def update(self, updates: Dict[str, Any]) -> None:
        """Update multiple values in the state."""
        self.update_access_time()
        self.data.update(updates)
    
    def clear(self) -> None:
        """Clear all state data."""
        self.data.clear()
        self.update_access_time()


class StateManager:
    """
    Manages session states for a node.
    
    This class provides:
    - Per-session state isolation
    - Automatic session expiration
    - Thread-safe state access
    - State persistence hooks (for future implementation)
    """
    
    def __init__(
        self,
        default_ttl: Optional[timedelta] = None,
        max_sessions: Optional[int] = None,
        enable_persistence: bool = False
    ):
        """
        Initialize the state manager.
        
        Args:
            default_ttl: Default time-to-live for sessions (None = no expiration)
            max_sessions: Maximum number of concurrent sessions (None = unlimited)
            enable_persistence: Whether to enable state persistence (future feature)
        """
        self._states: Dict[str, SessionState] = {}
        self._lock = asyncio.Lock()
        self.default_ttl = default_ttl or timedelta(hours=24)
        self.max_sessions = max_sessions
        self.enable_persistence = enable_persistence
        self._cleanup_task: Optional[asyncio.Task] = None
        
    async def get_or_create_session(self, session_id: str) -> SessionState:
        """Get an existing session or create a new one."""
        async with self._lock:
            if session_id not in self._states:
                # Check session limit
                if self.max_sessions and len(self._states) >= self.max_sessions:
                    # Remove oldest session
                    oldest_id = min(
                        self._states.keys(),
                        key=lambda k: self._states[k].last_accessed
                    )
                    del self._states[oldest_id]
                    logger.info(f"StateManager: Evicted oldest session {oldest_id} due to limit")
                
                # Create new session
                self._states[session_id] = SessionState(session_id=session_id)
                logger.debug(f"StateManager: Created new session {session_id}")
            
            return self._states[session_id]
    
    async def get_session(self, session_id: str) -> Optional[SessionState]:
        """Get an existing session, returns None if not found."""
        async with self._lock:
            return self._states.get(session_id)
    
    async def delete_session(self, session_id: str) -> bool:
        """Delete a session. Returns True if deleted, False if not found."""
        async with self._lock:
            if session_id in self._states:
                del self._states[session_id]
                logger.debug(f"StateManager: Deleted session {session_id}")
                return True
            return False
    
    async def get_all_sessions(self) -> Dict[str, SessionState]:
        """Get all active sessions."""
        async with self._lock:
            return self._states.copy()
    
    async def clear_all_sessions(self) -> None:
        """Clear all sessions."""
        async with self._lock:
            self._states.clear()
            logger.info("StateManager: Cleared all sessions")
    
    async def cleanup_expired_sessions(self) -> int:
        """Remove expired sessions. Returns number of sessions removed."""
        async with self._lock:
            now = datetime.now()
            expired = []
            
            for session_id, state in self._states.items():
                if now - state.last_accessed > self.default_ttl:
                    expired.append(session_id)
            
            for session_id in expired:
                del self._states[session_id]
            
            if expired:
                logger.info(f"StateManager: Cleaned up {len(expired)} expired sessions")
            
            return len(expired)
    
    async def start_cleanup_task(self, interval: timedelta = None) -> None:
        """Start periodic cleanup of expired sessions."""
        if self._cleanup_task and not self._cleanup_task.done():
            return
        
        interval = interval or timedelta(minutes=5)
        
        async def cleanup_loop():
            while True:
                try:
                    await asyncio.sleep(interval.total_seconds())
                    await self.cleanup_expired_sessions()
                except asyncio.CancelledError:
                    break
                except Exception as e:
                    logger.error(f"StateManager cleanup error: {e}")
        
        self._cleanup_task = asyncio.create_task(cleanup_loop())
    
    async def stop_cleanup_task(self) -> None:
        """Stop the periodic cleanup task."""
        if self._cleanup_task and not self._cleanup_task.done():
            self._cleanup_task.cancel()
            try:
                await self._cleanup_task
            except asyncio.CancelledError:
                pass


@dataclass
class RemoteExecutorConfig:
    """Configuration for remote execution of a node."""
    
    host: str
    port: int
    protocol: str = "grpc"
    auth_token: Optional[str] = None
    timeout: float = 30.0
    max_retries: int = 3
    ssl_enabled: bool = True
    pip_packages: Optional[List[str]] = None
    
    def __post_init__(self):
        """Validate configuration after initialization."""
        if self.protocol not in ["grpc", "http"]:
            raise ConfigurationError(f"Unsupported protocol: {self.protocol}")
        
        if self.port <= 0 or self.port > 65535:
            raise ConfigurationError(f"Invalid port: {self.port}")
        
        if self.timeout <= 0:
            raise ConfigurationError(f"Invalid timeout: {self.timeout}")

    def __call__(self, target: Union[str, "Node"], **kwargs) -> "Node":
        """
        Create a remote execution node from this configuration.

        This method acts as a factory. When you call an instance of this class,
        it will construct and return a specialized remote node wrapper.

        Example:
            config = RemoteExecutorConfig(host="localhost", port=50052)
            
            # To run a pre-registered node on the server by its class name:
            remote_node = config("MyNodeClassName", node_config={"param": "value"})

            # To run a local node object on the server:
            local_obj = MyNodeClass()
            remote_node = config(local_obj)

        Args:
            target (Union[str, "Node"]): The node class name (str) to be executed
                remotely, or a local Node instance to be serialized and sent to
                the server.
            **kwargs: Additional arguments for the remote node's constructor
                      (e.g., `node_config` for `RemoteExecutionNode`).

        Returns:
            A `RemoteExecutionNode` or `RemoteObjectExecutionNode` instance,
            configured for remote execution.
        """
        # Local import to avoid circular dependencies
        from ..nodes.remote import RemoteExecutionNode, RemoteObjectExecutionNode

        if isinstance(target, str):
            return RemoteExecutionNode(
                node_to_execute=target,
                remote_config=self,
                **kwargs
            )
        elif isinstance(target, Node):
            return RemoteObjectExecutionNode(
                obj_to_execute=target,
                remote_config=self,
                **kwargs
            )
        else:
            raise TypeError(
                "Target for remote execution must be a node class name (str) or a Node object."
            )


class Node(ABC):
    """
    Base class for all processing nodes in the pipeline.
    
    A Node represents a single processing step. The core logic is in the
    `process` method. Nodes are chained together in a `Pipeline` to create
    complex data flows.
    
    State Management:
        Each node has a StateManager that allows storing session-specific data.
        This enables multi-user scenarios where each session maintains its own state.
        
        Example:
            # In your node's process method
            session_state = await self.state.get_or_create_session(session_id)
            
            # Store data
            session_state.set('user_name', 'Alice')
            session_state.set('conversation_history', [])
            
            # Retrieve data
            user_name = session_state.get('user_name')
            history = session_state.get('conversation_history', [])
    """
    
    def __init__(
        self,
        name: Optional[str] = None,
        enable_state: bool = True,
        state_ttl: Optional[timedelta] = None,
        max_sessions: Optional[int] = None,
        **kwargs
    ):
        """
        Initialize a processing node.
        
        Args:
            name: Optional name for the node (defaults to class name)
            enable_state: Whether to enable state management (default: True)
            state_ttl: Time-to-live for session states (default: 24 hours)
            max_sessions: Maximum number of concurrent sessions (default: None/unlimited)
            **kwargs: Additional node-specific parameters
        """
        self.name = name or self.__class__.__name__
        self.config = kwargs
        self._is_initialized = False
        self.logger = logging.getLogger(self.__class__.__name__)
        
        # Initialize state manager if enabled
        self.enable_state = enable_state
        if enable_state:
            self.state = StateManager(
                default_ttl=state_ttl,
                max_sessions=max_sessions
            )
        else:
            self.state = None
        
        # Current session context (can be set during processing)
        self._current_session_id: Optional[str] = None
        
        self.logger.debug(f"Created node: {self.name}")
    
    @abstractmethod
    def process(self, data: Any) -> Any:
        """
        Process input data and return the result.
        
        This method must be implemented by all concrete node classes.
        
        Args:
            data: Input data to process
            
        Returns:
            Processed data
            
        Raises:
            NodeError: If processing fails
        """
        pass
    
    async def initialize(self) -> None:
        """
        Initialize the node before processing.
        
        This method is called once before the first process() call.
        Override this method to perform any setup required by the node.
        For remote nodes, this method runs on the remote server.
        """
        if self._is_initialized:
            return
            
        self.logger.debug(f"Initializing node: {self.name}")
        
        # Start state cleanup task if state is enabled
        if self.state:
            await self.state.start_cleanup_task()
        
        self._is_initialized = True
    
    async def cleanup(self) -> None:
        """
        Clean up resources used by the node.
        
        This method is called when the node is no longer needed.
        Override this method to perform any cleanup required by the node.
        """
        self.logger.debug(f"Cleaning up node: {self.name}")
        
        # Stop state cleanup task and clear states
        if self.state:
            await self.state.stop_cleanup_task()
            await self.state.clear_all_sessions()
        
        self._is_initialized = False
    
    def set_session_id(self, session_id: str) -> None:
        """
        Set the current session ID for state management.
        
        This is typically called by the pipeline or wrapper nodes
        to establish session context.
        """
        self._current_session_id = session_id
        self.logger.debug(f"Node {self.name}: Session ID set to {session_id}")
    
    def get_session_id(self) -> Optional[str]:
        """Get the current session ID."""
        return self._current_session_id
    
    async def get_session_state(self, session_id: Optional[str] = None) -> Optional[SessionState]:
        """
        Get the session state for the given session ID.
        
        Args:
            session_id: Session ID to get state for. If None, uses current session ID.
            
        Returns:
            SessionState object or None if state management is disabled or session not found.
        """
        if not self.state:
            return None
        
        session_id = session_id or self._current_session_id
        if not session_id:
            return None
        
        return await self.state.get_or_create_session(session_id)
    
    def extract_session_id(self, data: Any) -> Optional[str]:
        """
        Extract session ID from input data.
        
        This method looks for session_id in common data formats.
        Override this method to customize session ID extraction.
        
        Args:
            data: Input data that might contain session ID
            
        Returns:
            Extracted session ID or None
        """
        # Handle dict with session_id
        if isinstance(data, dict) and 'session_id' in data:
            return data['session_id']
        
        # Handle tuple with metadata dict
        if isinstance(data, tuple) and len(data) >= 2:
            # Check if last element is metadata dict
            metadata = data[-1]
            if isinstance(metadata, dict) and 'session_id' in metadata:
                return metadata['session_id']
        
        # No session ID found
        return None
    
    def split_data_metadata(self, data: Any) -> tuple[Any, Optional[Dict[str, Any]]]:
        """
        Split data into content and metadata components.
        
        This helper method standardizes data extraction across nodes.
        Supports common formats:
        - data (no metadata)
        - (data, metadata_dict)
        - (audio, sample_rate, metadata_dict) for audio data
        
        Args:
            data: Input data that might contain metadata
            
        Returns:
            Tuple of (data_without_metadata, metadata_dict_or_none)
        """
        if isinstance(data, tuple):
            # Check if last element is a metadata dict
            if len(data) >= 2 and isinstance(data[-1], dict):
                # Check if it looks like metadata (has common keys or session_id)
                potential_metadata = data[-1]
                if any(key in potential_metadata for key in ['session_id', 'metadata', 'user_name', 'timestamp']):
                    # Return data without metadata and the metadata
                    if len(data) == 2:
                        return data[0], potential_metadata
                    else:
                        # For tuples like (audio, sample_rate, metadata)
                        return data[:-1], potential_metadata
            
            # No metadata found, return as is
            return data, None
        else:
            # Non-tuple data has no metadata
            return data, None
    
    def merge_data_metadata(self, data: Any, metadata: Optional[Dict[str, Any]]) -> Any:
        """
        Merge processed data with metadata.
        
        This helper method ensures metadata is preserved through processing.
        It maintains the original data format while adding/preserving metadata.
        
        Args:
            data: Processed data
            metadata: Metadata to attach (can be None)
            
        Returns:
            Data with metadata attached in appropriate format
        """
        if metadata is None:
            return data
        
        if isinstance(data, tuple):
            # Add metadata to tuple
            return data + (metadata,)
        else:
            # For non-tuple data, create a tuple with metadata
            return (data, metadata)
    
    @property
    def is_initialized(self) -> bool:
        """Check if this node has been initialized."""
        return self._is_initialized
    
    def get_config(self) -> Dict[str, Any]:
        """Get the node configuration."""
        return {
            "name": self.name,
            "class": self.__class__.__name__,
            "config": self.config,
        }
    
    def __repr__(self) -> str:
        """String representation of the node."""
        return f"{self.__class__.__name__}(name='{self.name}')" 