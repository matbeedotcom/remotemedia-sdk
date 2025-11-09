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

from remotemedia.core import MultiprocessNode, NodeConfig, NodeStatus
from remotemedia.core.multiprocessing import get_node_class
# Signal handler setup
def setup_signal_handlers(node: MultiprocessNode):
    """Setup signal handlers for graceful shutdown."""
    import signal
    def signal_handler(sig, frame):
        node.logger.info(f"Received signal {sig}, initiating shutdown")
        import asyncio
        asyncio.create_task(node.stop())

    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    if hasattr(signal, 'SIGBREAK'):  # Windows
        signal.signal(signal.SIGBREAK, signal_handler)


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

            # Create node instance (pass config as keyword argument)
            self.node = node_class(config=self.config)

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
        Connect to IPC channels for data transfer using iceoryx2.
        """
        try:
            import iceoryx2 as iox2
            import ctypes

            # Set log level
            iox2.set_log_level_from_env_or(iox2.LogLevel.Info)
            
            # Create iceoryx2 node
            node = iox2.NodeBuilder.new().create(iox2.ServiceType.Ipc)
            
            # Create control channel service for READY signal using dynamic slice
            # Include session_id to avoid conflicts between sessions
            control_service_name = iox2.ServiceName.new(f"control/{self.config.session_id}_{self.config.node_id}")
            control_service = (
                node.service_builder(control_service_name)
                .publish_subscribe(iox2.Slice[ctypes.c_uint8])
                .open_or_create()
            )
            
            # Create publisher for control channel with dynamic allocation
            control_publisher = (
                control_service.publisher_builder()
                .initial_max_slice_len(16)  # Small initial size for control messages
                .allocation_strategy(iox2.AllocationStrategy.PowerOfTwo)
                .create()
            )
            
            # Create input/output data channels for RuntimeData BEFORE sending READY
            # This ensures Python is fully prepared to receive data when Rust sends it
            
            # Input channel: {session_id}_{node_id}_input - Wait for Rust to create it first, then open
            logger.info(f"Opening input channel: {self.config.session_id}_{self.config.node_id}_input (waiting for Rust to create it)")
            input_channel_name = iox2.ServiceName.new(f"{self.config.session_id}_{self.config.node_id}_input")
            
            # Retry opening the service - Rust creates it first when publisher is created
            max_retries = 50
            retry_delay = 0.1  # 100ms
            input_service = None
            
            for attempt in range(max_retries):
                try:
                    input_service = (
                        node.service_builder(input_channel_name)
                        .publish_subscribe(iox2.Slice[ctypes.c_uint8])
                        .open()  # Must use open() to connect to Rust's service with proper history config
                    )
                    logger.info(f"✅ Successfully opened input service on attempt {attempt + 1}")
                    break
                except Exception as e:
                    if attempt < max_retries - 1:
                        import time
                        time.sleep(retry_delay)
                    else:
                        # Last attempt failed, try creating with history config
                        logger.warn(f"Failed to open input service after {max_retries} attempts, creating with open_or_create()")
                        input_service = (
                            node.service_builder(input_channel_name)
                            .publish_subscribe(iox2.Slice[ctypes.c_uint8])
                            .history_size(100)  # Enable history to match Rust service
                            .subscriber_max_buffer_size(100)
                            .open_or_create()
                        )
                        logger.info(f"✅ Created input service with open_or_create() and history enabled")
                        break
            
            logger.info(f"Creating subscriber for input channel with buffer_size=100")
            input_subscriber = input_service.subscriber_builder().buffer_size(100).create()
            logger.info(f"✅ Input subscriber created successfully with history enabled")
            
            # Output channel: {session_id}_{node_id}_output - use .open() to connect to Rust's pre-created service
            logger.info(f"Opening output channel: {self.config.session_id}_{self.config.node_id}_output")
            output_channel_name = iox2.ServiceName.new(f"{self.config.session_id}_{self.config.node_id}_output")
            output_service = (
                node.service_builder(output_channel_name)
                .publish_subscribe(iox2.Slice[ctypes.c_uint8])
                .open()  # Must use open() to connect to Rust's service with proper config
            )
            logger.info(f"Creating publisher for output channel")
            output_publisher = (
                output_service.publisher_builder()
                .initial_max_slice_len(1024)  # Start with 1KB, will grow as needed
                .allocation_strategy(iox2.AllocationStrategy.PowerOfTwo)
                .create()
            )
            logger.info(f"✅ Output publisher created successfully")
            
            logger.info(f"✅ Created IPC data channels: {self.config.session_id}_{self.config.node_id}_input (sub), {self.config.session_id}_{self.config.node_id}_output (pub)")
            
            # NOW send READY signal - Python is fully prepared to receive data
            logger.info(f"Sending READY signal to Rust (subscriber is ready to receive)...")
            ready_msg = b"READY"
            sample = control_publisher.loan_slice_uninit(len(ready_msg))
            for i, byte_val in enumerate(ready_msg):
                sample.payload()[i] = byte_val
            sample = sample.assume_init()
            sample.send()
            
            logger.info(f"✅ Sent READY signal via iceoryx2 control channel: control/{self.config.session_id}_{self.config.node_id}")
            
            # Store node and services for later use
            self.node._iox2_node = node
            self.node._control_publisher = control_publisher
            self.node._input_subscriber = input_subscriber
            self.node._output_publisher = output_publisher

        except ImportError:
            logger.error("iceoryx2 Python package not installed. Install with: pip install iceoryx2")
            raise
        except Exception as e:
            logger.error(f"Failed to connect IPC channels: {e}", exc_info=True)
            raise

    async def run(self) -> None:
        """
        Main execution loop for the node.

        Runs the node lifecycle: initialize, process, cleanup.
        """
        try:
            # Initialize node (creates the node instance)
            await self.initialize()

            # Connect IPC channels and send READY signal FIRST
            # This allows Rust to proceed with pipeline setup immediately
            await self.connect_channels()
            logger.info(f"Sent READY signal, now initializing node resources...")

            # IMPORTANT: Start the process loop in a background task BEFORE model initialization
            # This ensures we're actively polling for messages even while models are loading
            from remotemedia.core.multiprocessing.node import NodeStatus
            self.node.status = NodeStatus.INITIALIZING

            # Start background task for the process loop
            process_loop_task = asyncio.create_task(self.node._process_loop_with_init())

            # Wait for the process loop to complete
            await process_loop_task

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
        default=None,
        help="JSON string of node parameters"
    )

    parser.add_argument(
        "--params-stdin",
        action="store_true",
        help="Read parameters from stdin instead of --params argument"
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

    # Read params from stdin if requested
    if args.params_stdin:
        try:
            params_json = sys.stdin.read()
            params = json.loads(params_json) if params_json.strip() else {}
        except json.JSONDecodeError as e:
            logger.error(f"Failed to parse params from stdin: {e}")
            sys.exit(1)
    else:
        params = args.params if args.params is not None else {}

    # Create node configuration
    config = NodeConfig(
        node_id=args.node_id,
        node_type=args.node_type,
        params=params,
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

    This is called when running: python -m remotemedia.core.multiprocess.runner
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