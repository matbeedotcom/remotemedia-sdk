"""
Shared protobuf definitions for RemoteMedia

This package contains the generated protobuf and gRPC files
that are shared between the client and service packages.
"""

from . import execution_pb2
from . import types_pb2

# Import gRPC files conditionally to handle version compatibility
try:
    from . import execution_pb2_grpc
    from . import types_pb2_grpc
    _grpc_available = True
except RuntimeError as e:
    if "grpc package installed is at version" in str(e):
        # gRPC version mismatch - make gRPC imports optional
        execution_pb2_grpc = None
        types_pb2_grpc = None
        _grpc_available = False
    else:
        raise

__all__ = [
    'execution_pb2',
    'execution_pb2_grpc', 
    'types_pb2',
    'types_pb2_grpc',
]