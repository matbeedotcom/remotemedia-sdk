import remotemedia
import asyncio
import numpy as np

async def test():
    client = remotemedia.RemoteMediaClient(grpc_address="[::1]:50051")
    async with client.stream() as session:
        print("✅ Session started")
        
        # Register pipeline with LFM2
        await session.register_pipeline_yaml("""
pipeline:
  - id: vad
    type: RustVADNode
    params:
      threshold: 0.5
  - id: audio_buffer
    type: RustFormatConverterNode
  - id: resample
    type: RustResampleNode
    params:
      target_rate: 8000
  - id: lfm2
    type: LFM2AudioNode
    inputs: {audio: resample}
    outputs: [features]
connections:
  - from: vad
    to: audio_buffer
  - from: audio_buffer
    to: resample
  - from: resample
    to: lfm2
""")
        print("✅ Pipeline registered")
        
        # Send test audio
        audio = np.random.randn(16000).astype(np.float32) * 0.1
        await session.send_input("vad", remotemedia.AudioData(
            samples=audio,
            sample_rate=16000,
            channels=1,
            session_id="test"
        ))
        print("✅ Audio sent, waiting for processing...")
        
        await asyncio.sleep(5)
        print("✅ Done")

if __name__ == "__main__":
    asyncio.run(test())
