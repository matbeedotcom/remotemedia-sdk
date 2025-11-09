#!/usr/bin/env python3
"""Test how PyAV returns stereo audio data."""

from aiortc.contrib.media import MediaPlayer
import asyncio

async def test():
    player = MediaPlayer('examples/transcribe_demo.wav')
    if player.audio:
        track = player.audio
        frame = await track.recv()

        print(f'Frame info:')
        print(f'  samples: {frame.samples}')
        print(f'  sample_rate: {frame.sample_rate}')
        print(f'  channels: {len(frame.layout.channels)}')
        print(f'  layout: {frame.layout.name}')
        print(f'  format: {frame.format.name}')

        array = frame.to_ndarray()
        print(f'\nNumPy array:')
        print(f'  shape: {array.shape}')
        print(f'  dtype: {array.dtype}')

        # Check if it's interleaved or planar
        print(f'\nFirst 20 values: {array.flatten()[:20]}')

asyncio.run(test())
