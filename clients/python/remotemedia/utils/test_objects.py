from remotemedia.core.node import Node


class TestStreamingObject(Node):
    """A simple stateful streaming object for testing remote execution."""

    def __init__(self, initial_value=0, **kwargs):
        super().__init__(**kwargs)
        self.value = initial_value
        self.is_streaming = True

    async def initialize(self):
        # In a real node, this is where you'd acquire resources
        pass

    async def cleanup(self):
        # In a real node, this is where you'd release resources
        pass

    async def process(self, data_stream):
        """
        Increments the internal value for each item received in the stream.
        """
        async for item, in data_stream:
            self.value += item
            yield (self.value,) 