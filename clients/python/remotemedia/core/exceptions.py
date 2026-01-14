"""
Custom exceptions for the RemoteMedia SDK.
"""


class RemoteMediaError(Exception):
    """Base exception for all RemoteMedia SDK errors."""
    pass


class PipelineError(RemoteMediaError):
    """Exception raised for pipeline-related errors."""
    pass


class NodeError(RemoteMediaError):
    """Exception raised for node-related errors."""
    pass


class RemoteExecutionError(RemoteMediaError):
    """Exception raised for remote execution errors."""
    
    def __init__(self, message: str, remote_traceback: str = None):
        super().__init__(message)
        self.remote_traceback = remote_traceback


class WebRTCError(RemoteMediaError):
    """Exception raised for WebRTC-related errors."""
    pass


class SerializationError(RemoteMediaError):
    """Exception raised for serialization/deserialization errors."""
    pass


class ConfigurationError(RemoteMediaError):
    """Exception raised for configuration-related errors."""
    pass 