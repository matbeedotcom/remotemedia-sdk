"""
Runner script for multiprocess nodes.

This module is executed as the main entry point for Python nodes
running in separate processes. It handles:
- Command-line argument parsing
- Node instantiation
- IPC channel setup
- Main execution loop
"""

import argparse
import asyncio
import json
import logging
import sys
import signal
from typing import Optional, Dict, Any

from .node import MultiprocessNode, NodeConfig, NodeStatus, setup_signal_handlers
from .data import RuntimeData
from . import get_node_class


# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class NodeRunner:
    """
    Runner for executing a node in a separate process.

    Handles the lifecycle of a node from initialization through
    processing to cleanup.
    """

    def __init__(self, config: NodeConfig):
        """
        Initialize the node runner.

        Args:
            config: Node configuration
        """
        self.config = config
        self.node: Optional[MultiprocessNode] = None
        self._shutdown_event = asyncio.Event()

    async def initialize(self) -> None:
        """Initialize the node instance."""
        try:
            # Get the node class
            node_class = get_node_class(self.config.node_type)

            # Create node instance
            self.node = node_class(self.config)

            # Setup signal handlers for graceful shutdown
            setup_signal_handlers(self.node)

            logger.info(f"Initialized node: {self.config.node_id} ({self.config.node_type})")

        except KeyError:
            logger.error(f"Node type '{self.config.node_type}' not registered")
            raise
        except Exception as e:
            logger.error(f"Failed to initialize node: {e}", exc_info=True)
            raise

    async def connect_channels(self) -> None:
        """
        Connect to IPC channels for data transfer.

        This will be implemented to connect to iceoryx2 channels
        via the Rust runtime.
        """
        # TODO: Implement IPC channel connection
        # This will use ctypes or similar to call into the Rust library
        # For now, just log
        logger.info(f"Connecting IPC channels for node {self.config.node_id}")

    async def run(self) -> None:
        """
        Main execution loop for the node.

        Runs the node lifecycle: initialize, process, cleanup.
        """
        try:
            # Initialize node
            await self.initialize()

            # Connect IPC channels
            await self.connect_channels()

            # Run the node
            logger.info(f"Starting node execution: {self.config.node_id}")
            await self.node.run()

        except asyncio.CancelledError:
            logger.info(f"Node {self.config.node_id} execution cancelled")
        except Exception as e:
            logger.error(f"Node execution failed: {e}", exc_info=True)
            sys.exit(1)
        finally:
            logger.info(f"Node {self.config.node_id} execution completed")

    async def shutdown(self) -> None:
        """Shutdown the node gracefully."""
        if self.node:
            await self.node.stop()
        self._shutdown_event.set()


def parse_arguments() -> argparse.Namespace:
    """
    Parse command-line arguments.

    Returns:
        Parsed arguments
    """
    parser = argparse.ArgumentParser(
        description="Run a RemoteMedia multiprocess node"
    )

    parser.add_argument(
        "--node-type",
        required=True,
        help="Type of node to run (must be registered)"
    )

    parser.add_argument(
        "--node-id",
        required=True,
        help="Unique identifier for this node instance"
    )

    parser.add_argument(
        "--params",
        type=json.loads,
        default={},
        help="JSON string of node parameters"
    )

    parser.add_argument(
        "--session-id",
        help="Session ID for this pipeline execution"
    )

    parser.add_argument(
        "--log-level",
        default="INFO",
        choices=["DEBUG", "INFO", "WARNING", "ERROR"],
        help="Logging level"
    )

    parser.add_argument(
        "--ipc-config",
        type=json.loads,
        help="IPC channel configuration (JSON)"
    )

    return parser.parse_args()


async def main() -> None:
    """Main entry point for the node runner."""
    # Parse arguments
    args = parse_arguments()

    # Configure logging
    logging.getLogger().setLevel(getattr(logging, args.log_level))

    # Create node configuration
    config = NodeConfig(
        node_id=args.node_id,
        node_type=args.node_type,
        params=args.params,
        session_id=args.session_id,
        log_level=args.log_level
    )

    logger.info(f"Starting node runner for {config.node_type}:{config.node_id}")

    # Create and run the node
    runner = NodeRunner(config)

    # Setup shutdown handler
    def shutdown_handler(sig, frame):
        logger.info(f"Received signal {sig}, shutting down")
        asyncio.create_task(runner.shutdown())

    signal.signal(signal.SIGINT, shutdown_handler)
    signal.signal(signal.SIGTERM, shutdown_handler)

    try:
        await runner.run()
    except KeyboardInterrupt:
        logger.info("Interrupted by user")
    except Exception as e:
        logger.error(f"Runner failed: {e}", exc_info=True)
        sys.exit(1)


def run() -> None:
    """
    Entry point for the runner module.

    This is called when running: python -m remotemedia_sdk.multiprocess.runner
    """
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("Runner interrupted")
        sys.exit(0)
    except Exception as e:
        logger.error(f"Runner error: {e}", exc_info=True)
        sys.exit(1)


if __name__ == "__main__":
    run()