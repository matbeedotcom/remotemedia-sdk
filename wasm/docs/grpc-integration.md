# gRPC Integration Guide for WASM Modules

## Overview

This guide explains how WASM modules integrate with your existing gRPC remote execution system, enabling seamless communication between edge processing and cloud services.

## Architecture

### Communication Flow

```
┌──────────────────┐     Host Function Call    ┌──────────────────┐
│                  │ ─────────────────────────► │                  │
│   WASM Module    │                            │   Python Host    │
│                  │ ◄───────────────────────── │                  │
└──────────────────┘      Return Result        └────────┬─────────┘
                                                         │
                                                         │ gRPC
                                                         ▼
                                                ┌──────────────────┐
                                                │  Remote Service  │
                                                │  (UltravoxNode,  │
                                                │   KokoroTTS,     │
                                                │   etc.)          │
                                                └──────────────────┘
```

## Implementation

### 1. WASM Module with gRPC Capabilities

```rust
// src/wasm-modules/rust/intelligent_vad/src/lib.rs

use std::slice;
use std::mem;

// Import host functions provided by Python
extern "C" {
    // Call a remote service via gRPC
    fn call_remote_service(
        service_name_ptr: *const u8,
        service_name_len: usize,
        data_ptr: *const u8,
        data_len: usize,
        result_ptr: *mut u8,
        result_len: *mut usize
    ) -> i32;
    
    // Stream data to a remote service
    fn stream_to_remote(
        service_name_ptr: *const u8,
        service_name_len: usize,
        data_ptr: *const u8,
        data_len: usize
    ) -> i32;
    
    // Get or create a remote session
    fn get_remote_session(
        session_id_ptr: *const u8,
        session_id_len: usize,
        session_ptr: *mut u8,
        session_len: *mut usize
    ) -> i32;
}

// Main processing function that can call remote services
#[no_mangle]
pub extern "C" fn process_with_remote(
    input_ptr: *const u8,
    input_len: usize,
    output_ptr: *mut u8,
    output_len: *mut usize
) -> i32 {
    let input_data = unsafe { slice::from_raw_parts(input_ptr, input_len) };
    
    // 1. Perform local processing
    let local_result = perform_local_vad(input_data);
    
    // 2. If uncertain, call remote VAD service
    if should_verify_with_remote(local_result) {
        let mut remote_buffer = vec![0u8; 4096];
        let mut remote_len = remote_buffer.len();
        
        // Call remote VoiceActivityDetector service
        let status = unsafe {
            call_remote_service(
                b"VoiceActivityDetector".as_ptr(),
                22,  // Length of service name
                input_ptr,
                input_len,
                remote_buffer.as_mut_ptr(),
                &mut remote_len
            )
        };
        
        if status == 0 {
            // Success - use remote result
            let remote_result = parse_remote_response(&remote_buffer[..remote_len]);
            return write_output(remote_result, output_ptr, output_len);
        }
    }
    
    // 3. Return local result
    write_output(local_result, output_ptr, output_len)
}

// Helper function to decide if remote verification is needed
fn should_verify_with_remote(local_confidence: f32) -> bool {
    // Low confidence results should be verified remotely
    local_confidence > 0.3 && local_confidence < 0.7
}
```

### 2. Python Host Implementation

```python
# src/python/nodes/hybrid_wasm_node.py

from typing import Any, Dict, Optional, AsyncGenerator
import asyncio
import json
from dataclasses import dataclass

from remotemedia.core.node import Node, RemoteExecutorConfig
from remotemedia.remote.client import RemoteExecutionClient
from remotemedia.nodes.wasm import WasmEdgeNode, WasmConfig

class HybridWasmNode(WasmEdgeNode):
    """
    WASM node with gRPC communication capabilities.
    
    This node allows WASM modules to call your existing remote services
    through host functions, maintaining session state and streaming support.
    """
    
    def __init__(
        self,
        wasm_config: WasmConfig,
        remote_config: RemoteExecutorConfig,
        available_services: Dict[str, str] = None,
        **kwargs
    ):
        """
        Initialize hybrid WASM node with gRPC capabilities.
        
        Args:
            wasm_config: WASM module configuration
            remote_config: gRPC connection configuration
            available_services: Map of service names to node types
        """
        super().__init__(wasm_config=wasm_config, **kwargs)
        
        self.remote_config = remote_config
        self.remote_client: Optional[RemoteExecutionClient] = None
        
        # Map service names to actual node implementations
        self.available_services = available_services or {
            "VoiceActivityDetector": "remotemedia.nodes.audio.VoiceActivityDetector",
            "UltravoxNode": "remotemedia.nodes.ml.UltravoxNode",
            "KokoroTTSNode": "remotemedia.nodes.ml.KokoroTTSNode",
        }
        
        # Track active remote sessions
        self._remote_sessions: Dict[str, str] = {}
    
    async def initialize(self) -> None:
        """Initialize both WASM runtime and gRPC client."""
        await super().initialize()
        
        # Connect to remote service
        self.remote_client = RemoteExecutionClient(self.remote_config)
        await self.remote_client.connect()
        
        logger.info(f"HybridWasmNode '{self.name}' connected to gRPC service")
    
    async def cleanup(self) -> None:
        """Clean up WASM and gRPC resources."""
        # Clean up remote sessions
        for session_id in self._remote_sessions:
            try:
                await self._cleanup_remote_session(session_id)
            except Exception as e:
                logger.warning(f"Failed to cleanup remote session {session_id}: {e}")
        
        # Disconnect gRPC client
        if self.remote_client:
            await self.remote_client.disconnect()
        
        await super().cleanup()
    
    async def _create_wasm_vm(self) -> 'wasmedge.VM':
        """Create WASM VM with host functions for gRPC communication."""
        vm = await super()._create_wasm_vm()
        
        # Register host functions that WASM can call
        self._register_host_functions(vm)
        
        return vm
    
    def _register_host_functions(self, vm: 'wasmedge.VM') -> None:
        """Register host functions for WASM → gRPC communication."""
        
        # Function: call_remote_service
        @wasmedge.host_function
        def call_remote_service(
            service_name_ptr: int, service_name_len: int,
            data_ptr: int, data_len: int,
            result_ptr: int, result_len_ptr: int
        ) -> int:
            """Host function to call a remote service via gRPC."""
            try:
                # Extract service name from WASM memory
                service_name = self._read_wasm_string(vm, service_name_ptr, service_name_len)
                
                # Extract input data from WASM memory
                input_data = self._read_wasm_bytes(vm, data_ptr, data_len)
                
                # Call remote service (blocking in WASM context)
                result = asyncio.run(self._call_remote_service_async(service_name, input_data))
                
                # Write result back to WASM memory
                self._write_wasm_bytes(vm, result_ptr, result_len_ptr, result)
                
                return 0  # Success
            except Exception as e:
                logger.error(f"Remote service call failed: {e}")
                return -1  # Error
        
        # Function: stream_to_remote
        @wasmedge.host_function
        def stream_to_remote(
            service_name_ptr: int, service_name_len: int,
            data_ptr: int, data_len: int
        ) -> int:
            """Host function to stream data to a remote service."""
            try:
                service_name = self._read_wasm_string(vm, service_name_ptr, service_name_len)
                input_data = self._read_wasm_bytes(vm, data_ptr, data_len)
                
                # Queue data for streaming
                asyncio.run(self._stream_to_remote_async(service_name, input_data))
                
                return 0  # Success
            except Exception as e:
                logger.error(f"Stream to remote failed: {e}")
                return -1  # Error
        
        # Function: get_remote_session
        @wasmedge.host_function
        def get_remote_session(
            session_id_ptr: int, session_id_len: int,
            session_ptr: int, session_len_ptr: int
        ) -> int:
            """Host function to get or create a remote session."""
            try:
                session_id = self._read_wasm_string(vm, session_id_ptr, session_id_len)
                
                # Get or create remote session
                remote_session_id = asyncio.run(self._get_remote_session_async(session_id))
                
                # Write session ID back to WASM
                self._write_wasm_string(vm, session_ptr, session_len_ptr, remote_session_id)
                
                return 0  # Success
            except Exception as e:
                logger.error(f"Get remote session failed: {e}")
                return -1  # Error
        
        # Register functions with the VM
        vm.register_host_function("env", "call_remote_service", call_remote_service)
        vm.register_host_function("env", "stream_to_remote", stream_to_remote)
        vm.register_host_function("env", "get_remote_session", get_remote_session)
    
    async def _call_remote_service_async(self, service_name: str, data: bytes) -> bytes:
        """Call a remote service via gRPC."""
        if not self.remote_client:
            raise RuntimeError("Remote client not initialized")
        
        # Get the node type for this service
        node_type = self.available_services.get(service_name)
        if not node_type:
            raise ValueError(f"Unknown service: {service_name}")
        
        # Deserialize input data
        input_data = self._deserialize_input(data)
        
        # Call remote service using existing infrastructure
        result = await self.remote_client.execute_node(
            node_type=node_type,
            config={},
            input_data=input_data,
            serialization_format="pickle"
        )
        
        # Serialize result for WASM
        return self._serialize_output(result)
    
    async def _stream_to_remote_async(self, service_name: str, data: bytes) -> None:
        """Stream data to a remote service."""
        if not self.remote_client:
            raise RuntimeError("Remote client not initialized")
        
        node_type = self.available_services.get(service_name)
        if not node_type:
            raise ValueError(f"Unknown service: {service_name}")
        
        # Create streaming generator
        async def data_generator():
            yield self._deserialize_input(data)
        
        # Stream to remote service
        async for _ in self.remote_client.stream_node(
            node_type=node_type,
            config={},
            input_stream=data_generator()
        ):
            pass  # Process streaming results if needed
    
    async def _get_remote_session_async(self, local_session_id: str) -> str:
        """Get or create a remote session."""
        if local_session_id in self._remote_sessions:
            return self._remote_sessions[local_session_id]
        
        # Create new remote session
        # This would integrate with your session management
        remote_session_id = f"remote_{local_session_id}_{id(self)}"
        self._remote_sessions[local_session_id] = remote_session_id
        
        return remote_session_id
    
    def _deserialize_input(self, data: bytes) -> Any:
        """Deserialize input data from WASM format."""
        try:
            # Try JSON first
            return json.loads(data.decode('utf-8'))
        except (json.JSONDecodeError, UnicodeDecodeError):
            # Fallback to raw bytes (e.g., audio data)
            return data
    
    def _serialize_output(self, result: Any) -> bytes:
        """Serialize output data for WASM consumption."""
        if isinstance(result, bytes):
            return result
        elif isinstance(result, str):
            return result.encode('utf-8')
        else:
            # JSON serialize complex objects
            return json.dumps(result).encode('utf-8')
```

### 3. Usage Example

```python
# examples/hybrid_vad_example.py

import asyncio
import numpy as np
from pathlib import Path

from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import RemoteExecutorConfig
from remotemedia.nodes.wasm import HybridWasmNode, WasmConfig

async def main():
    """
    Example of using a WASM VAD module that can call remote VAD service
    for verification when confidence is low.
    """
    
    # Configure remote connection
    remote_config = RemoteExecutorConfig(
        host="localhost",
        port=50052,
        ssl_enabled=False
    )
    
    # Create hybrid WASM node
    hybrid_vad = HybridWasmNode(
        wasm_config=WasmConfig(
            wasm_path="modules/audio/intelligent_vad.wasm",
            function_name="process_with_remote",
            memory_limit=64 * 1024 * 1024  # 64MB
        ),
        remote_config=remote_config,
        available_services={
            "VoiceActivityDetector": "remotemedia.nodes.audio.VoiceActivityDetector",
        },
        name="HybridVAD"
    )
    
    # Create pipeline
    pipeline = Pipeline(name="HybridVADPipeline")
    pipeline.add_node(hybrid_vad)
    
    # Process audio
    async with pipeline.managed_execution():
        # Simulate audio stream
        async def audio_stream():
            # Generate test audio chunks
            for i in range(10):
                # Create audio chunk (1 second at 16kHz)
                audio_chunk = np.random.randn(16000).astype(np.float32)
                
                # Add metadata
                yield (audio_chunk, {"chunk_id": i, "session_id": "test_session"})
        
        # Process through hybrid pipeline
        results = []
        async for result in pipeline.process(audio_stream()):
            results.append(result)
            print(f"VAD Result: {result}")
        
        print(f"Processed {len(results)} audio chunks")
        print("Edge processing with remote verification when needed!")

if __name__ == "__main__":
    asyncio.run(main())
```

## Communication Patterns

### Pattern 1: Simple Remote Call
```
WASM → call_remote_service() → gRPC → Remote Service → Response → WASM
```

### Pattern 2: Streaming to Remote
```
WASM → stream_to_remote() → gRPC Stream → Remote Service
```

### Pattern 3: Session-Based Communication
```
WASM → get_remote_session() → Session ID
WASM → call_remote_service(session_id, ...) → Stateful Processing
```

### Pattern 4: Fallback Pattern
```
WASM → Local Processing → Low Confidence? → call_remote_service() → Use Remote Result
                       ↓
                  High Confidence → Use Local Result
```

## Serialization Formats

### Supported Data Types

1. **Primitive Types**
   - Integers, Floats, Booleans
   - Strings (UTF-8)
   - Byte arrays

2. **Complex Types**
   - JSON objects
   - NumPy arrays (as bytes)
   - Pickle-serialized Python objects

3. **Audio/Video Data**
   - PCM audio (float32/int16)
   - Raw video frames
   - Compressed formats

### Serialization Examples

```python
# Audio data serialization
audio_np = np.array([...], dtype=np.float32)
audio_bytes = audio_np.tobytes()
# Send audio_bytes to WASM

# JSON serialization
config = {"threshold": 0.5, "model": "vad_v2"}
config_bytes = json.dumps(config).encode('utf-8')
# Send config_bytes to WASM

# Complex object serialization
import pickle
complex_obj = {"audio": audio_np, "metadata": {...}}
obj_bytes = pickle.dumps(complex_obj)
# Send obj_bytes to WASM
```

## Error Handling

### WASM Side
```rust
let status = unsafe { call_remote_service(...) };
match status {
    0 => {
        // Success - process result
    },
    -1 => {
        // Network error - use local fallback
    },
    -2 => {
        // Service unavailable - retry or fallback
    },
    _ => {
        // Unknown error
    }
}
```

### Python Side
```python
try:
    result = await self._call_remote_service_async(service_name, data)
except grpc.RpcError as e:
    if e.code() == grpc.StatusCode.UNAVAILABLE:
        # Service unavailable
        return self._create_error_response("service_unavailable")
    else:
        # Other gRPC error
        raise
except Exception as e:
    logger.error(f"Unexpected error calling remote service: {e}")
    return self._create_error_response("internal_error")
```

## Performance Considerations

### Optimization Strategies

1. **Batch Remote Calls**: Group multiple requests to reduce overhead
2. **Cache Remote Results**: Store frequently used results locally
3. **Async Processing**: Use async/await for non-blocking operations
4. **Connection Pooling**: Reuse gRPC connections across calls
5. **Selective Calling**: Only call remote when necessary

### Benchmarks

```python
# Measure overhead of remote calls from WASM
import time

# Local WASM processing only
start = time.time()
local_result = await wasm_node.process(data)
local_time = time.time() - start

# WASM with remote call
start = time.time()
hybrid_result = await hybrid_wasm_node.process(data)
hybrid_time = time.time() - start

print(f"Local only: {local_time*1000:.2f}ms")
print(f"With remote: {hybrid_time*1000:.2f}ms")
print(f"Overhead: {(hybrid_time - local_time)*1000:.2f}ms")
```

## Security Considerations

### Access Control
```python
# Whitelist allowed services
ALLOWED_SERVICES = {
    "VoiceActivityDetector",
    "AudioTransform",
    # Explicitly list allowed services
}

if service_name not in ALLOWED_SERVICES:
    raise SecurityError(f"Service {service_name} not allowed")
```

### Authentication
```python
# Use existing gRPC authentication
self.remote_client = RemoteExecutionClient(
    config=RemoteExecutorConfig(
        host="localhost",
        port=50052,
        auth_token=os.environ.get("GRPC_AUTH_TOKEN")
    )
)
```

This integration enables your WASM modules to leverage the full power of your existing gRPC remote services while maintaining the performance benefits of edge processing.