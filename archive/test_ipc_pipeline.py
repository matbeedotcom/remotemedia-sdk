#!/usr/bin/env python3
"""Test script to trigger IPC pipeline and check iceoryx2 services."""

import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), 'python-grpc-client'))

import grpc
from generated import streaming_pb2, streaming_pb2_grpc, common_pb2
import numpy as np
import time

def main():
    # Connect to server
    channel = grpc.insecure_channel('[::1]:50051')
    stub = streaming_pb2_grpc.StreamingPipelineServiceStub(channel)
    
    print("📡 Connecting to gRPC server...")
    
    def request_generator():
        # Send init message
        yield streaming_pb2.StreamRequest(
            init=streaming_pb2.StreamInit(
                session_id='test_ipc_123',
                node_configs=[
                    streaming_pb2.NodeConfig(
                        node_id='audio_buffer',
                        node_type='RustVADNode',
                        params='{}'
                    ),
                    streaming_pb2.NodeConfig(
                        node_id='vad_to_buffer_resampler',
                        node_type='RustResampleNode',
                        params='{"input_sample_rate": 16000, "output_sample_rate": 24000}'
                    ),
                    streaming_pb2.NodeConfig(
                        node_id='lfm2_audio',
                        node_type='LFM2AudioNode',
                        params='{}'
                    )
                ],
                connections=[
                    streaming_pb2.Connection(
                        from_node='audio_buffer',
                        to_node='vad_to_buffer_resampler'
                    ),
                    streaming_pb2.Connection(
                        from_node='vad_to_buffer_resampler',
                        to_node='lfm2_audio'
                    )
                ]
            )
        )
        
        print("✅ Sent init message, waiting for nodes to initialize...")
        time.sleep(2)
        
        # Send some audio data
        print("🎵 Sending audio data...")
        audio_samples = np.random.randn(1600).astype(np.float32)
        yield streaming_pb2.StreamRequest(
            data=common_pb2.RuntimeData(
                audio=common_pb2.AudioBuffer(
                    samples=audio_samples.tobytes(),
                    sample_rate=16000,
                    channels=1,
                    format='f32le'
                )
            )
        )
        
        print("⏳ Waiting for processing...")
        time.sleep(1)
    
    try:
        print("🚀 Starting streaming request...")
        responses = stub.ProcessStream(request_generator())
        
        for response in responses:
            msg_type = response.WhichOneof('message')
            print(f"📨 Received: {msg_type}")
            if msg_type == 'data':
                data_type = response.data.WhichOneof('payload')
                print(f"   Data type: {data_type}")
        
        print("✅ Stream completed successfully")
        
    except grpc.RpcError as e:
        print(f"❌ RPC Error: {e.code()} - {e.details()}")
    except Exception as e:
        print(f"❌ Error: {e}")
    finally:
        channel.close()

if __name__ == '__main__':
    main()
