"""
Simple echo node for IPC integration testing.

This node receives data via IPC and echoes it back, with optional transformations.
"""

import logging
from typing import Any
from remotemedia.core.multiprocessing import MultiprocessNode

logger = logging.getLogger(__name__)


class EchoNode(MultiprocessNode):
    """
    Simple echo node that receives data and sends it back.

    Useful for testing IPC communication without complex processing.
    """

    def __init__(self, node_id=None, config=None, **kwargs):
        # Handle the specific pattern from runner.py:
        # node_class(self.config.node_id, config=self.config)
        # This means node_id is positional and config is keyword

        if node_id is not None and config is not None:
            # Runner passes both node_id and config
            # Just use the config (it already has the node_id)
            super().__init__(config=config, **kwargs)
        elif config is not None:
            # Config-only initialization
            super().__init__(config=config, **kwargs)
        elif node_id is not None:
            # node_id only - create a minimal config
            config = {
                'node_id': node_id,
                'node_type': 'EchoNode',
                'params': {},
                'session_id': kwargs.get('session_id')
            }
            super().__init__(config=config, **kwargs)
        else:
            # No arguments - error
            raise ValueError("Either node_id or config must be provided")

        self.echo_count = 0
        # Use self.node_id which is set by parent class
        logger.info(f"EchoNode {self.node_id} initialized")

    async def initialize(self) -> None:
        """Initialize the echo node."""
        await super().initialize()
        self.echo_count = 0
        logger.info(f"EchoNode {self.node_id} ready to echo")

    async def process(self, data: Any) -> Any:
        """
        Echo the received data back with a counter.

        Args:
            data: Input data to echo

        Returns:
            The same data with echo metadata
        """
        self.echo_count += 1
        logger.info(f"🔊 EchoNode received data (echo #{self.echo_count}): {data}")

        # Echo back the data
        if data.is_text():
            text = data.as_text()
            response_text = f"Echo #{self.echo_count}: {text}"
            logger.info(f"Echoing text: {response_text}")

            # Import RuntimeData
            try:
                from remotemedia.core.multiprocessing.data import RuntimeData
                return RuntimeData.text(response_text)
            except ImportError:
                logger.error("RuntimeData not available")
                return None

        elif data.is_audio():
            samples_bytes, sample_rate, channels, format_str, num_samples = data.as_audio()
            logger.info(f"Echoing audio: {num_samples} samples @ {sample_rate}Hz, {channels}ch")
            # Just echo back the audio unchanged
            return data

        else:
            logger.warning(f"Unsupported data type: {data.data_type()}")
            return None

    async def cleanup(self) -> None:
        """Cleanup resources."""
        logger.info(f"EchoNode {self.node_id} cleanup (echoed {self.echo_count} messages)")
        await super().cleanup()
