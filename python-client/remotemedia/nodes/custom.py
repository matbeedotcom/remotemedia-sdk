"""
A collection of custom, stateful node-like classes for testing purposes.

These classes are not part of the core node library but are used in tests
to verify the remote execution of custom, user-defined, cloudpickle-serialized objects.
"""

from remotemedia.core.node import Node
from typing import AsyncGenerator, Any, Tuple
import asyncio

class StatefulCounter(Node):
    """A simple stateful streaming node for testing remote execution."""
    
    def __init__(self, initial_value=0, **kwargs):
        super().__init__(**kwargs)
        self.value = initial_value
        self.is_streaming = True

    async def initialize(self):
        await super().initialize()
        # In a real node, this is where you'd acquire resources
        pass

    async def cleanup(self):
        # In a real node, this is where you'd release resources
        await super().cleanup()
        pass

    async def process(self, data_stream: AsyncGenerator[Any, None]) -> AsyncGenerator[Tuple[int], None]:
        """
        Increments the internal value for each item in the stream.
        Ignores the actual data and just counts.
        """
        async for _ in data_stream:
            yield (self.value,)
            self.value += 1
            await asyncio.sleep(0.01) # Simulate work

__all__ = ["StatefulCounter"] 