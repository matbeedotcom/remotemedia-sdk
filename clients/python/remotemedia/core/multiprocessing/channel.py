"""
IPC Channel classes for zero-copy data transfer between Python nodes.

Provides Publisher and Subscriber classes that wrap the Rust iceoryx2
shared memory channels for efficient inter-process communication.
"""

from typing import Optional
import struct
from .data import RuntimeData


class Publisher:
    """
    Publisher for sending data to a shared memory channel.

    This class provides a Python interface to the Rust Publisher,
    enabling zero-copy data transfer to other Python nodes.
    """

    def __init__(self, channel_name: str):
        """
        Create a publisher for the given channel.

        Args:
            channel_name: Name of the channel to publish to
        """
        self.channel_name = channel_name
        # TODO: Initialize Rust publisher through FFI
        self._rust_publisher = None

    async def publish(self, data: RuntimeData) -> None:
        """
        Publish data to the channel.

        Args:
            data: RuntimeData to publish

        Raises:
            RuntimeError: If publishing fails
        """
        # Serialize to bytes
        payload = data.to_bytes()

        # TODO: Call Rust publisher through FFI
        # For now, simulate the operation
        if self._rust_publisher is None:
            # Would call: self._rust_publisher.publish(payload)
            pass

    async def try_publish(self, data: RuntimeData) -> bool:
        """
        Try to publish without blocking.

        Returns False if the channel is full and backpressure is enabled.

        Args:
            data: RuntimeData to publish

        Returns:
            True if published successfully, False if would block
        """
        try:
            await self.publish(data)
            return True
        except Exception:
            return False


class Subscriber:
    """
    Subscriber for receiving data from a shared memory channel.

    This class provides a Python interface to the Rust Subscriber,
    enabling zero-copy data reception from other Python nodes.
    """

    def __init__(self, channel_name: str):
        """
        Create a subscriber for the given channel.

        Args:
            channel_name: Name of the channel to subscribe to
        """
        self.channel_name = channel_name
        # TODO: Initialize Rust subscriber through FFI
        self._rust_subscriber = None

    async def receive(self) -> Optional[RuntimeData]:
        """
        Receive data from the channel.

        Returns None if no data is available.

        Returns:
            RuntimeData if available, None otherwise

        Raises:
            RuntimeError: If receiving fails
        """
        # TODO: Call Rust subscriber through FFI
        # For now, return None
        if self._rust_subscriber is None:
            # Would call: bytes_data = self._rust_subscriber.receive()
            return None

        # If we had data, deserialize it
        # return RuntimeData.from_bytes(bytes_data)

    async def receive_blocking(self, timeout_ms: Optional[int] = None) -> Optional[RuntimeData]:
        """
        Receive data from the channel, blocking until data is available.

        Args:
            timeout_ms: Optional timeout in milliseconds

        Returns:
            RuntimeData if available within timeout, None on timeout

        Raises:
            RuntimeError: If receiving fails
        """
        # TODO: Implement blocking receive with timeout
        return await self.receive()


class ChannelStats:
    """Statistics for a channel."""

    def __init__(self):
        self.messages_sent = 0
        self.messages_received = 0
        self.bytes_transferred = 0
        self.last_activity = None


# Helper functions for channel management

async def create_channel(
    name: str,
    capacity: int = 100,
    backpressure: bool = True
) -> tuple[Publisher, Subscriber]:
    """
    Create a bidirectional channel with publisher and subscriber.

    Args:
        name: Unique channel name
        capacity: Maximum number of messages in the buffer
        backpressure: Whether to block on full buffer

    Returns:
        Tuple of (Publisher, Subscriber)

    Raises:
        RuntimeError: If channel creation fails
    """
    publisher = Publisher(name)
    subscriber = Subscriber(name)
    return publisher, subscriber


async def connect_nodes(
    from_node: str,
    to_node: str,
    channel_name: Optional[str] = None
) -> tuple[Publisher, Subscriber]:
    """
    Connect two nodes with a channel.

    Args:
        from_node: Source node ID
        to_node: Destination node ID
        channel_name: Optional channel name (auto-generated if not provided)

    Returns:
        Tuple of (Publisher for from_node, Subscriber for to_node)
    """
    if channel_name is None:
        channel_name = f"{from_node}_to_{to_node}"

    return await create_channel(channel_name)


__all__ = [
    'Publisher',
    'Subscriber',
    'ChannelStats',
    'create_channel',
    'connect_nodes',
]
