#!/usr/bin/env python3
"""Debug script to test the pipeline flow."""

import asyncio
import sys
import numpy as np
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent / "python-client"))

from remotemedia.core.pipeline import Pipeline
from remotemedia.core.node import Node
from remotemedia.nodes.source import MediaReaderNode, AudioTrackSource
from remotemedia.nodes.audio import AudioTransform, VoiceActivityDetector


class DebugNode(Node):
    """Debug node that prints what it receives."""

    def __init__(self, label="Debug", **kwargs):
        super().__init__(**kwargs)
        self.label = label
        self.count = 0
        self.is_streaming = True

    async def process(self, data_stream):
        print(f"\n{self.label}: Starting to receive data...")
        async for data in data_stream:
            self.count += 1
            print(f"{self.label} [{self.count}]: Received {type(data)}")

            if isinstance(data, tuple):
                print(f"  Tuple length: {len(data)}")
                for i, item in enumerate(data):
                    print(f"  Item {i}: {type(item)}")
                    if isinstance(item, np.ndarray):
                        print(f"    Shape: {item.shape}, dtype: {item.dtype}")
                    elif isinstance(item, dict):
                        print(f"    Keys: {list(item.keys())}")
                    elif isinstance(item, tuple):
                        print(f"    Nested tuple length: {len(item)}")
                        for j, subitem in enumerate(item):
                            print(f"      Subitem {j}: {type(subitem)}")
                            if isinstance(subitem, np.ndarray):
                                print(f"        Shape: {subitem.shape}")

            yield data

        print(f"{self.label}: Done. Processed {self.count} items")


async def main():
    """Test the pipeline flow."""
    pipeline = Pipeline(name="TestFlow")

    # Build pipeline
    pipeline.add_node(MediaReaderNode(
        path="examples/transcribe_demo.wav",
        chunk_size=4096,
        name="MediaReader"
    ))

    pipeline.add_node(DebugNode(label="After MediaReader"))

    pipeline.add_node(AudioTrackSource(name="AudioSource"))

    pipeline.add_node(DebugNode(label="After AudioSource"))

    pipeline.add_node(AudioTransform(
        output_sample_rate=16000,
        output_channels=1,
        name="Resample"
    ))

    pipeline.add_node(DebugNode(label="After Resample"))

    # VAD
    vad = VoiceActivityDetector(
        frame_duration_ms=30,
        filter_mode=False,
        include_metadata=True,
        name="VAD"
    )
    vad.is_streaming = True
    pipeline.add_node(vad)

    pipeline.add_node(DebugNode(label="After VAD"))

    # Run
    print("Starting pipeline...")
    async with pipeline.managed_execution():
        count = 0
        async for result in pipeline.process():
            count += 1
            if count > 5:  # Just process first 5 items
                break
        print(f"\nPipeline produced {count} final outputs")


if __name__ == "__main__":
    asyncio.run(main())
