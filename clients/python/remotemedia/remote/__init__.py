"""
Remote execution components for the RemoteMedia SDK.
"""

from .client import RemoteExecutionClient
from .proxy_client import RemoteProxyClient, remote_class

__all__ = ["RemoteExecutionClient", "RemoteProxyClient", "remote_class"] 