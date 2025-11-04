"""
Base class for multiprocess nodes.

This module provides the abstract base class that all Python nodes
must inherit from to run in separate processes.
"""

from abc import ABC, abstractmethod
from enum import Enum
from typing import Optional, Dict, Any, List
import asyncio
import logging
import signal
import sys
from dataclasses import dataclass

from .data import RuntimeData


class NodeStatus(Enum):
    """Node execution status."""
    IDLE = "idle"
    INITIALIZING = "initializing"
    READY = "ready"
    PROCESSING = "processing"
    ERROR = "error"
    STOPPING = "stopping"
    STOPPED = "stopped"


@dataclass
class NodeConfig:
    """Configuration for a node instance."""
    node_id: str
    node_type: str
    params: Dict[str, Any]
    session_id: Optional[str] = None
    log_level: str = "INFO"


class MultiprocessNode(ABC):
    """
    Base class for nodes running in separate processes.

    All custom nodes must inherit from this class and implement
    the abstract methods for initialization, processing, and cleanup.
    """

    def __init__(self, config: NodeConfig):
        """
        Initialize the node.

        Args:
            config: Node configuration
        """
        self.node_id = config.node_id
        self.node_type = config.node_type
        self.config = config.params
        self.session_id = config.session_id or f"default_{self.node_id}"

        # Node state
        self._status = NodeStatus.IDLE
        self._stop_event = asyncio.Event()
        self._init_complete = asyncio.Event()

        # Setup logging
        self._setup_logging(config.log_level)

        # Input/output channels (will be set by the runner)
        self.input_channels: Dict[str, Any] = {}
        self.output_channels: Dict[str, Any] = {}

        # Statistics
        self.messages_processed = 0
        self.messages_failed = 0
        self.processing_time_total = 0.0

    def _setup_logging(self, log_level: str):
        """Setup logging for the node."""
        self.logger = logging.getLogger(f"node.{self.node_type}.{self.node_id}")
        self.logger.setLevel(getattr(logging, log_level.upper(), logging.INFO))

        # Add handler if not already present
        if not self.logger.handlers:
            handler = logging.StreamHandler()
            formatter = logging.Formatter(
                '%(asctime)s - %(name)s - %(levelname)s - %(message)s'
            )
            handler.setFormatter(formatter)
            self.logger.addHandler(handler)

    @property
    def status(self) -> NodeStatus:
        """Get current node status."""
        return self._status

    @status.setter
    def status(self, value: NodeStatus):
        """Set node status and log the change."""
        if value != self._status:
            self.logger.info(f"Status change: {self._status.value} -> {value.value}")
            self._status = value

    @abstractmethod
    async def initialize(self) -> None:
        """
        Initialize node resources (load models, etc).

        This method is called once when the node starts up.
        Override this to perform any initialization tasks like:
        - Loading ML models
        - Setting up connections
        - Allocating resources

        Raises:
            Exception: If initialization fails
        """
        pass

    @abstractmethod
    async def process(self, data: RuntimeData) -> Optional[RuntimeData]:
        """
        Process incoming data and return output.

        This method is called for each incoming data item.
        It should process the input and return the output,
        or None if no output is produced.

        Args:
            data: Input runtime data

        Returns:
            Output runtime data or None

        Raises:
            Exception: If processing fails
        """
        pass

    @abstractmethod
    async def cleanup(self) -> None:
        """
        Clean up resources before shutdown.

        This method is called once when the node is shutting down.
        Override this to perform cleanup tasks like:
        - Releasing resources
        - Saving state
        - Closing connections

        Raises:
            Exception: If cleanup fails
        """
        pass

    async def run(self) -> None:
        """
        Main execution loop.

        This method is called by the framework to run the node.
        It handles the lifecycle: initialize -> process loop -> cleanup.
        """
        try:
            # Initialize
            self.status = NodeStatus.INITIALIZING
            self.logger.info(f"Initializing node {self.node_id}")

            await self.initialize()

            self.status = NodeStatus.READY
            self._init_complete.set()
            self.logger.info(f"Node {self.node_id} ready")

            # Process loop
            await self._process_loop()

        except Exception as e:
            self.status = NodeStatus.ERROR
            self.logger.error(f"Node error: {e}", exc_info=True)
            raise

        finally:
            # Cleanup
            self.status = NodeStatus.STOPPING
            self.logger.info(f"Stopping node {self.node_id}")

            try:
                await self.cleanup()
            except Exception as e:
                self.logger.error(f"Cleanup error: {e}", exc_info=True)

            self.status = NodeStatus.STOPPED
            self.logger.info(f"Node {self.node_id} stopped")

    async def _process_loop(self) -> None:
        """
        Main processing loop.

        Continuously processes incoming data until stopped.
        """
        self.logger.info(f"Starting process loop for {self.node_id}")

        while not self._stop_event.is_set():
            try:
                # Get input data (this will be connected to IPC channels)
                data = await self._receive_input()

                if data is None:
                    # No data available, check for stop
                    await asyncio.sleep(0.01)
                    continue

                # Process data
                self.status = NodeStatus.PROCESSING
                start_time = asyncio.get_event_loop().time()

                output = await self.process(data)

                # Track statistics
                self.messages_processed += 1
                self.processing_time_total += asyncio.get_event_loop().time() - start_time

                # Send output if produced
                if output is not None:
                    await self._send_output(output)

                self.status = NodeStatus.READY

            except asyncio.CancelledError:
                # Normal shutdown
                break

            except Exception as e:
                self.messages_failed += 1
                self.logger.error(f"Processing error: {e}", exc_info=True)

                # Continue processing unless critical
                if self._is_critical_error(e):
                    self.status = NodeStatus.ERROR
                    raise

    async def _receive_input(self) -> Optional[RuntimeData]:
        """
        Receive input data from IPC channels.

        Returns:
            Input data or None if no data available
        """
        # This will be implemented by the runner to connect to actual IPC
        # For now, return None to indicate no data
        return None

    async def _send_output(self, data: RuntimeData) -> None:
        """
        Send output data to IPC channels.

        Args:
            data: Output data to send
        """
        # This will be implemented by the runner to connect to actual IPC
        self.logger.debug(f"Output produced: {data.type.value}")

    def _is_critical_error(self, error: Exception) -> bool:
        """
        Determine if an error is critical and should stop the node.

        Args:
            error: The exception that occurred

        Returns:
            True if the error is critical
        """
        # Override this method to customize error handling
        return isinstance(error, (MemoryError, SystemError))

    async def stop(self) -> None:
        """Request the node to stop processing."""
        self.logger.info(f"Stop requested for {self.node_id}")
        self._stop_event.set()

    async def wait_ready(self, timeout: Optional[float] = None) -> bool:
        """
        Wait for the node to be ready.

        Args:
            timeout: Maximum time to wait in seconds

        Returns:
            True if node became ready, False if timeout
        """
        try:
            await asyncio.wait_for(
                self._init_complete.wait(),
                timeout=timeout
            )
            return True
        except asyncio.TimeoutError:
            return False

    def get_stats(self) -> Dict[str, Any]:
        """
        Get node statistics.

        Returns:
            Dictionary of statistics
        """
        avg_time = (self.processing_time_total / self.messages_processed
                   if self.messages_processed > 0 else 0.0)

        return {
            "node_id": self.node_id,
            "node_type": self.node_type,
            "status": self.status.value,
            "messages_processed": self.messages_processed,
            "messages_failed": self.messages_failed,
            "average_processing_time": avg_time,
            "total_processing_time": self.processing_time_total,
        }


def setup_signal_handlers(node: MultiprocessNode):
    """
    Setup signal handlers for graceful shutdown.

    Args:
        node: The node instance to handle signals for
    """
    def signal_handler(sig, frame):
        """Handle shutdown signals."""
        node.logger.info(f"Received signal {sig}, initiating shutdown")
        asyncio.create_task(node.stop())

    # Register handlers
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)

    # Windows-specific
    if sys.platform == "win32":
        signal.signal(signal.SIGBREAK, signal_handler)