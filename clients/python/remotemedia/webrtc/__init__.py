"""
WebRTC communication components for the RemoteMedia SDK.
"""

from .manager import WebRTCManager

# Optional imports: server and pipeline_processor require aiortc
try:
    from .server import WebRTCServer, WebRTCConfig, WebRTCConnection
    from .pipeline_processor import (
        WebRTCPipelineProcessor,
        WebRTCStreamSource,
        WebRTCStreamSink,
        WebRTCDataChannelNode,
        StreamMetadata
    )
    _has_aiortc = True
except ImportError:
    WebRTCServer = None
    WebRTCConfig = None
    WebRTCConnection = None
    WebRTCPipelineProcessor = None
    WebRTCStreamSource = None
    WebRTCStreamSink = None
    WebRTCDataChannelNode = None
    StreamMetadata = None
    _has_aiortc = False

__all__ = [
    "WebRTCManager",
]

if _has_aiortc:
    __all__.extend([
        "WebRTCServer",
        "WebRTCConfig", 
        "WebRTCConnection",
        "WebRTCPipelineProcessor",
        "WebRTCStreamSource",
        "WebRTCStreamSink",
        "WebRTCDataChannelNode",
        "StreamMetadata"
    ]) 