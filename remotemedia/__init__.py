"""
RemoteMedia - Distributed audio/video/data processing framework

This is the root namespace package for the RemoteMedia mono-repo.
The actual implementations are in:
- remotemedia.client: Client SDK for interacting with remote processing services
- remotemedia.service: Backend service implementation for distributed processing
"""

__version__ = "0.1.0"

# This is a namespace package
__path__ = __import__('pkgutil').extend_path(__path__, __name__)