#!/usr/bin/env python3
"""
Comprehensive test suite for Python gRPC client.

Tests all major functionality:
- Connection and version check
- ExecutePipeline (unary RPC)
- StreamPipeline (bidirectional streaming)
- Error handling
"""

import asyncio
import sys
import struct
import math
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from remotemedia_client import (
    RemoteMediaClient,
    RemoteMediaError,
    AudioBuffer,
    AudioFormat,
    ErrorType
)


class TestResults:
    """Track test results."""
    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.errors = []
    
    def success(self, test_name: str):
        self.passed += 1
        print(f"✅ {test_name}")
    
    def failure(self, test_name: str, error: str):
        self.failed += 1
        self.errors.append((test_name, error))
        print(f"❌ {test_name}: {error}")
    
    def summary(self):
        total = self.passed + self.failed
        print(f"\n{'='*60}")
        print(f"Test Summary: {self.passed}/{total} passed")
        if self.errors:
            print(f"\nFailed tests:")
            for name, error in self.errors:
                print(f"  - {name}: {error}")
        return self.failed == 0


async def test_connection(client: RemoteMediaClient, results: TestResults):
    """Test basic connection."""
    try:
        await client.connect()
        results.success("Connection established")
    except Exception as e:
        results.failure("Connection", str(e))
        raise


async def test_get_version(client: RemoteMediaClient, results: TestResults):
    """Test GetVersion RPC."""
    try:
        version = await client.get_version()
        
        # Validate version info
        assert version.protocol_version, "Protocol version missing"
        assert version.runtime_version, "Runtime version missing"
        assert len(version.supported_node_types) > 0, "No supported nodes"
        
        results.success(f"GetVersion (protocol: {version.protocol_version})")
        return version
    except Exception as e:
        results.failure("GetVersion", str(e))
        raise


async def test_execute_pipeline(client: RemoteMediaClient, results: TestResults):
    """Test ExecutePipeline RPC."""
    try:
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "test_execution",
                "description": "Test pipeline",
                "created_at": "2025-10-28T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "passthrough",
                    "node_type": "PassThrough",
                    "params": "{}",
                    "is_streaming": False
                }
            ],
            "connections": []
        }
        
        # Generate test audio
        samples = [struct.pack('<f', 0.5) for _ in range(1600)]
        audio_buffer = AudioBuffer(
            samples=b''.join(samples),
            sample_rate=16000,
            channels=1,
            format=AudioFormat.F32,
            num_samples=1600
        )
        
        result = await client.execute_pipeline(
            manifest=manifest,
            audio_inputs={"passthrough": audio_buffer}
        )
        
        # Validate result
        assert result.status == "EXECUTION_STATUS_SUCCESS", f"Status: {result.status}"
        assert result.metrics.wall_time_ms > 0, "No wall time recorded"
        
        # Check if we got audio output
        has_audio_output = len(result.audio_outputs) > 0
        
        results.success(
            f"ExecutePipeline ({result.metrics.wall_time_ms:.2f}ms, "
            f"audio_outputs={has_audio_output})"
        )
        return result
    except Exception as e:
        results.failure("ExecutePipeline", str(e))
        raise


async def test_stream_pipeline(client: RemoteMediaClient, results: TestResults):
    """Test StreamPipeline RPC."""
    try:
        manifest = {
            "version": "v1",
            "metadata": {
                "name": "test_streaming",
                "description": "Test streaming",
                "created_at": "2025-10-28T00:00:00Z"
            },
            "nodes": [
                {
                    "id": "source",
                    "node_type": "PassThrough",
                    "params": "{}",
                    "is_streaming": False
                }
            ],
            "connections": []
        }
        
        CHUNK_SIZE = 1600
        NUM_CHUNKS = 5
        
        async def generate_chunks():
            for seq in range(NUM_CHUNKS):
                samples = [struct.pack('<f', 0.5) for _ in range(CHUNK_SIZE)]
                buffer = AudioBuffer(
                    samples=b''.join(samples),
                    sample_rate=16000,
                    channels=1,
                    format=AudioFormat.F32,
                    num_samples=CHUNK_SIZE
                )
                yield ("source", buffer, seq)
        
        # Collect results
        chunk_count = 0
        latencies = []
        
        async for chunk_result in client.stream_pipeline(
            manifest=manifest,
            audio_chunks=generate_chunks(),
            expected_chunk_size=CHUNK_SIZE
        ):
            chunk_count += 1
            latencies.append(chunk_result.processing_time_ms)
            assert chunk_result.sequence < NUM_CHUNKS, "Invalid sequence"
        
        # Validate
        assert chunk_count == NUM_CHUNKS, f"Expected {NUM_CHUNKS} chunks, got {chunk_count}"
        avg_latency = sum(latencies) / len(latencies)
        
        results.success(
            f"StreamPipeline ({chunk_count} chunks, {avg_latency:.2f}ms avg)"
        )
        return avg_latency
    except Exception as e:
        results.failure("StreamPipeline", str(e))
        raise


async def test_error_handling(client: RemoteMediaClient, results: TestResults):
    """Test error handling with invalid manifest."""
    try:
        # Create invalid manifest (nonexistent node type)
        manifest = {
            "version": "v1",
            "metadata": {"name": "invalid"},
            "nodes": [
                {
                    "id": "invalid",
                    "node_type": "NonExistentNode",
                    "params": "{}",
                    "is_streaming": False
                }
            ],
            "connections": []
        }
        
        try:
            await client.execute_pipeline(manifest=manifest)
            results.failure("Error handling", "Should have raised error for invalid node")
        except RemoteMediaError as e:
            # Expected error
            assert e.error_type is not None, "Error type missing"
            results.success(f"Error handling (caught {e.error_type.name})")
    except Exception as e:
        results.failure("Error handling", str(e))


async def test_performance_targets(
    exec_result,
    stream_avg_latency: float,
    results: TestResults
):
    """Validate performance targets."""
    try:
        # ExecutePipeline target: <5ms (SC-001)
        exec_time = exec_result.metrics.wall_time_ms
        if exec_time < 5.0:
            results.success(f"ExecutePipeline latency target ({exec_time:.2f}ms < 5ms)")
        else:
            results.failure(
                "ExecutePipeline latency",
                f"{exec_time:.2f}ms >= 5ms target"
            )
        
        # StreamPipeline target: <50ms per chunk
        if stream_avg_latency < 50.0:
            results.success(
                f"StreamPipeline latency target ({stream_avg_latency:.2f}ms < 50ms)"
            )
        else:
            results.failure(
                "StreamPipeline latency",
                f"{stream_avg_latency:.2f}ms >= 50ms target"
            )
    except Exception as e:
        results.failure("Performance validation", str(e))


async def main():
    print("="*60)
    print("Python gRPC Client Test Suite")
    print("="*60)
    print()
    
    results = TestResults()
    client = RemoteMediaClient("localhost:50051")
    
    try:
        # Test 1: Connection
        print("Test 1: Connection")
        await test_connection(client, results)
        print()
        
        # Test 2: GetVersion
        print("Test 2: GetVersion")
        version = await test_get_version(client, results)
        print(f"  Protocol: {version.protocol_version}")
        print(f"  Runtime: {version.runtime_version}")
        print(f"  Nodes: {len(version.supported_node_types)}")
        print()
        
        # Test 3: ExecutePipeline
        print("Test 3: ExecutePipeline")
        exec_result = await test_execute_pipeline(client, results)
        print(f"  Latency: {exec_result.metrics.wall_time_ms:.2f}ms")
        print()
        
        # Test 4: StreamPipeline
        print("Test 4: StreamPipeline")
        stream_latency = await test_stream_pipeline(client, results)
        print(f"  Average latency: {stream_latency:.2f}ms")
        print()
        
        # Test 5: Error handling
        print("Test 5: Error Handling")
        await test_error_handling(client, results)
        print()
        
        # Test 6: Performance targets
        print("Test 6: Performance Targets")
        await test_performance_targets(exec_result, stream_latency, results)
        print()
        
    except Exception as e:
        print(f"\n⚠️ Test suite aborted: {e}")
    finally:
        await client.disconnect()
    
    # Summary
    success = results.summary()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    asyncio.run(main())
