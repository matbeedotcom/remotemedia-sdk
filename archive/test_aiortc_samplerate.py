#!/usr/bin/env python3
"""Test what sample rate aiortc MediaPlayer outputs."""

from aiortc.contrib.media import MediaPlayer
import asyncio

async def test():
    player = MediaPlayer('examples/transcribe_demo.wav')
    if player.audio:
        track = player.audio
        frame = await track.recv()
        print(f'Frame properties:')
        print(f'  Sample rate: {frame.sample_rate}Hz')
        print(f'  Format: {frame.format.name}')
        print(f'  Samples: {frame.samples}')
        print(f'  Channels: {len(frame.layout.channels)}')

asyncio.run(test())
