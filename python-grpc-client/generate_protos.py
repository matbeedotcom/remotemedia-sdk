#!/usr/bin/env python3
"""
Generate Python gRPC stubs from proto definitions.

This script compiles the proto files from runtime/protos/ into Python
client stubs in the generated/ directory.
"""

import os
import subprocess
import sys
from pathlib import Path

def main():
    # Paths
    script_dir = Path(__file__).parent
    proto_dir = script_dir.parent / "runtime" / "protos"
    output_dir = script_dir / "generated"
    
    # Ensure output directory exists
    output_dir.mkdir(exist_ok=True)
    
    # Create __init__.py for generated package with clean imports
    init_file = output_dir / "__init__.py"
    init_content = '''"""Auto-generated gRPC stubs with clean imports."""

# Clean imports without _pb2 suffix
from .common_pb2 import (
    AudioBuffer,
    AudioFormat,
    ExecutionMetrics,
    NodeMetrics,
    ErrorResponse,
    ErrorType,
    VersionInfo,
    ResourceLimits,
    ExecutionStatus,
    NodeStatus,
    NodeResult,
    AUDIO_FORMAT_F32,
    AUDIO_FORMAT_I16,
    AUDIO_FORMAT_I32,
    ERROR_TYPE_VALIDATION,
    ERROR_TYPE_NODE_EXECUTION,
    ERROR_TYPE_RESOURCE_LIMIT,
    ERROR_TYPE_AUTHENTICATION,
    ERROR_TYPE_VERSION_MISMATCH,
    ERROR_TYPE_INTERNAL,
    EXECUTION_STATUS_SUCCESS,
    EXECUTION_STATUS_PARTIAL_SUCCESS,
    EXECUTION_STATUS_FAILED,
)

from .execution_pb2 import (
    PipelineManifest,
    ManifestMetadata,
    NodeManifest,
    Connection,
    ExecuteRequest,
    ExecuteResponse,
    ExecutionResult,
    VersionRequest,
    VersionResponse,
    CapabilityRequirements,
    GpuRequirement,
    CpuRequirement,
    RuntimeHint,
    RUNTIME_HINT_AUTO,
    RUNTIME_HINT_RUSTPYTHON,
    RUNTIME_HINT_CPYTHON,
    RUNTIME_HINT_CPYTHON_WASM,
)

from .execution_pb2_grpc import (
    PipelineExecutionServiceStub,
    PipelineExecutionServiceServicer,
)

from .streaming_pb2 import (
    StreamRequest,
    StreamResponse,
    StreamInit,
    AudioChunk,
    StreamControl,
    StreamReady,
    ChunkResult,
    StreamMetrics,
    StreamClosed,
    StreamErrorResponse,
    StreamErrorType,
)

from .streaming_pb2_grpc import (
    StreamingPipelineServiceStub,
    StreamingPipelineServiceServicer,
)

__all__ = [
    # Common types
    "AudioBuffer",
    "AudioFormat",
    "ExecutionMetrics",
    "ErrorResponse",
    "ErrorType",
    "VersionInfo",
    
    # Execution types
    "PipelineManifest",
    "ExecuteRequest",
    "ExecuteResponse",
    "ExecutionResult",
    "VersionRequest",
    "VersionResponse",
    
    # Streaming types
    "StreamRequest",
    "StreamResponse",
    "StreamInit",
    "AudioChunk",
    "ChunkResult",
    
    # Service stubs
    "PipelineExecutionServiceStub",
    "StreamingPipelineServiceStub",
]
'''
    init_file.write_text(init_content)
    
    # Proto files to compile
    proto_files = [
        "common.proto",
        "execution.proto",
        "streaming.proto"
    ]
    
    print(f"Generating Python gRPC stubs from {proto_dir}")
    print(f"Output directory: {output_dir}")
    print()
    
    # Compile each proto file
    for proto_file in proto_files:
        proto_path = proto_dir / proto_file
        
        if not proto_path.exists():
            print(f"❌ Proto file not found: {proto_path}")
            sys.exit(1)
        
        print(f"Compiling {proto_file}...")
        
        # Run protoc
        cmd = [
            sys.executable, "-m", "grpc_tools.protoc",
            f"--proto_path={proto_dir}",
            f"--python_out={output_dir}",
            f"--grpc_python_out={output_dir}",
            str(proto_path)
        ]
        
        try:
            result = subprocess.run(
                cmd,
                check=True,
                capture_output=True,
                text=True
            )
            print(f"  ✅ {proto_file} compiled successfully")
            
        except subprocess.CalledProcessError as e:
            print(f"  ❌ Failed to compile {proto_file}")
            print(f"     Error: {e.stderr}")
            sys.exit(1)
    
    print()
    print("✅ All proto files compiled successfully!")
    print()
    print(f"Generated files in {output_dir}:")
    for f in sorted(output_dir.glob("*.py")):
        print(f"  - {f.name}")
    
    # Fix imports in generated files (grpc_tools generates incorrect relative imports)
    print()
    print("Fixing imports in generated files...")
    fix_imports(output_dir)
    print("✅ Imports fixed")
    print()
    print("Ready to use! Import with clean names:")
    print("  from generated import AudioBuffer, PipelineManifest, StreamRequest")
    print("  from generated import PipelineExecutionServiceStub")

def fix_imports(output_dir: Path):
    """
    Fix imports in generated _pb2.py files.
    
    grpc_tools generates: import common_pb2
    We need: from . import common_pb2
    """
    for pb2_file in output_dir.glob("*_pb2.py"):
        content = pb2_file.read_text()
        
        # Fix common_pb2 imports
        if "import common_pb2" in content and "from . import" not in content:
            content = content.replace(
                "import common_pb2 as common__pb2",
                "from . import common_pb2 as common__pb2"
            )
            content = content.replace(
                "import execution_pb2 as execution__pb2",
                "from . import execution_pb2 as execution__pb2"
            )
            pb2_file.write_text(content)
    
    # Fix _grpc.py files too
    for grpc_file in output_dir.glob("*_pb2_grpc.py"):
        content = grpc_file.read_text()
        
        content = content.replace(
            "import common_pb2 as common__pb2",
            "from . import common_pb2 as common__pb2"
        )
        content = content.replace(
            "import execution_pb2 as execution__pb2",
            "from . import execution_pb2 as execution__pb2"
        )
        content = content.replace(
            "import streaming_pb2 as streaming__pb2",
            "from . import streaming_pb2 as streaming__pb2"
        )
        
        grpc_file.write_text(content)

if __name__ == "__main__":
    main()
