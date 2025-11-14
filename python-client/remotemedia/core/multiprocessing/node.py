"""
Base class for multiprocess nodes.

This module provides the abstract base class that all Python nodes
must inherit from to run in separate processes.

Inherits from remotemedia.core.node.Node to maintain compatibility with
existing pipeline infrastructure while adding multiprocess capabilities.
"""

from abc import abstractmethod
from enum import Enum
from typing import Optional, Dict, Any, List, AsyncGenerator, Union
import asyncio
import logging
import signal
import sys
from dataclasses import dataclass

# Import base Node class from core
from remotemedia.core.node import Node as BaseNode

try:
    from remotemedia_runtime.runtime_data import RuntimeData
    HAS_RUNTIME_DATA = True
except ImportError:
    RuntimeData = None  # type: ignore
    HAS_RUNTIME_DATA = False


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


class MultiprocessNode(BaseNode):
    """
    Base class for nodes running in separate processes.

    Inherits from remotemedia.core.node.Node and adds multiprocess-specific
    functionality like IPC channels, status tracking, and process lifecycle management.

    All custom Python AI nodes should inherit from this class to enable
    concurrent execution in separate processes with independent GILs.
    """

    def __init__(self, config: Union[NodeConfig, Dict[str, Any]], **kwargs):
        """
        Initialize the multiprocess node.

        Args:
            config: Either a NodeConfig instance or a dict with node_id, node_type, params
            **kwargs: Additional parameters (for compatibility with base Node class)
        """
        # Handle both NodeConfig and dict inputs for flexibility
        if isinstance(config, NodeConfig):
            node_id = config.node_id
            node_type = config.node_type
            params = config.params
            session_id = config.session_id
            log_level = config.log_level
        else:
            # Dict-based config
            node_id = config.get('node_id', kwargs.get('node_id', 'unknown'))
            node_type = config.get('node_type', self.__class__.__name__)
            params = config.get('params', {})
            session_id = config.get('session_id', None)
            log_level = config.get('log_level', 'INFO')

        # Initialize base Node class
        super().__init__(name=node_id, **kwargs)

        # Multiprocess-specific attributes
        self.node_id = node_id
        self.node_type = node_type
        self.config = params
        self.session_id = session_id or f"default_{self.node_id}"

        # Node state (multiprocess-specific)
        self._status = NodeStatus.IDLE
        self._stop_event = asyncio.Event()
        self._init_complete = asyncio.Event()

        # Setup logging
        self._setup_logging(log_level)

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
    async def process(self, data: RuntimeData) -> Union[RuntimeData, AsyncGenerator[RuntimeData, None], None]:
        """
        Process incoming data and return output.

        This method is called for each incoming data item.
        It should process the input and return the output,
        or None if no output is produced.

        For streaming nodes, this can return an AsyncGenerator that yields multiple outputs.

        Args:
            data: Input runtime data

        Returns:
            Output runtime data, AsyncGenerator of runtime data, or None

        Raises:
            Exception: If processing fails
        """
        pass

    def process_sync(self, data: Any) -> Any:
        """
        Synchronous process method for compatibility with base Node class.

        This is a bridge that allows MultiprocessNode to work with code
        expecting the base Node interface.

        Args:
            data: Input data (converted to RuntimeData if needed)

        Returns:
            Processed data
        """
        if not HAS_RUNTIME_DATA:
            raise ImportError("RuntimeData not available - please install remotemedia_runtime")

        # Convert to RuntimeData if needed
        if isinstance(data, RuntimeData):
            runtime_data = data
        else:
            # Attempt to wrap in RuntimeData
            runtime_data = RuntimeData.json(data) if isinstance(data, dict) else RuntimeData.text(str(data))

        # Run async process method
        loop = asyncio.get_event_loop()
        result = loop.run_until_complete(self.process(runtime_data))

        return result

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

    async def _process_loop_only(self) -> None:
        """
        Start the process loop without re-initializing.

        Used by runner when initialization is done separately before connecting channels.
        This ensures READY signal is only sent when truly ready to process.
        """
        try:
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

    async def _process_loop_with_init(self) -> None:
        """
        Initialize node resources and then start the process loop.

        This allows messages to be queued while model initialization is happening,
        ensuring no data loss even if initialization takes a long time.
        """
        # Queue for messages that arrive during initialization
        init_queue = []

        try:
            self.logger.info(f"Initializing node {self.node_id} (models may take time to load)")

            # Start a background task to queue incoming messages during initialization
            async def queue_messages_during_init():
                self.logger.info(f"🔵 Queuing task started for {self.node_id}, will poll for messages during initialization")
                poll_count = 0
                while self.status == NodeStatus.INITIALIZING:
                    poll_count += 1
                    data = await self._receive_input()
                    if data is not None:
                        self.logger.info(f"✅ Queuing message during initialization: {data.data_type()}")
                        init_queue.append(data)
                    else:
                        # Log first 5 polls to confirm we're actively polling
                        if poll_count <= 5:
                            self.logger.info(f"🔵 Queue poll #{poll_count}: No data yet")
                        await asyncio.sleep(0)
                self.logger.info(f"🔵 Queuing task finished for {self.node_id}, queued {len(init_queue)} messages")

            # Start queuing task
            queue_task = asyncio.create_task(queue_messages_during_init())

            # CRITICAL: Yield control to let the queuing task start before we block on model init
            # This ensures the subscriber is actively polling BEFORE Rust sends data
            self.logger.info(f"Yielding to allow queuing task to start...")
            await asyncio.sleep(0.1)  # 100ms to ensure queuing task is running
            self.logger.info(f"Queuing task should be active now, proceeding with model initialization...")

            # Initialize (load models, etc.) - can take a long time
            await self.initialize()

            self.status = NodeStatus.READY
            self._init_complete.set()

            # Stop the queuing task
            queue_task.cancel()
            try:
                await queue_task
            except asyncio.CancelledError:
                pass

            self.logger.info(f"Node {self.node_id} ready, processing {len(init_queue)} queued messages")

            # Process any queued messages first
            for data in init_queue:
                try:
                    await self._process_single_message(data)
                except Exception as e:
                    self.logger.error(f"Error processing queued message: {e}", exc_info=True)

            # Now start the normal process loop
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

    async def _process_single_message(self, data: RuntimeData) -> None:
        """
        Process a single message (used for both queued and real-time messages).

        Args:
            data: The input data to process
        """
        self.logger.info(f"Processing data in node {self.node_id} {data.data_type()}")

        # Process data
        self.status = NodeStatus.PROCESSING
        start_time = asyncio.get_event_loop().time()

        # Call process() - it may return a single value, an async generator, or None
        result = self.process(data)

        # Check if it's an async generator (streaming node)
        import inspect
        if inspect.isasyncgen(result):
            # Streaming: iterate over async generator and send each output
            self.logger.debug(f"Node {self.node_id} returned async generator (streaming)")
            output_count = 0
            async for output in result:
                if output is not None:
                    await self._send_output(output)
                    output_count += 1

            # Track statistics (count one "message processed" per input, regardless of N outputs)
            self.messages_processed += 1
            self.processing_time_total += asyncio.get_event_loop().time() - start_time
            self.logger.debug(f"Streaming node produced {output_count} outputs")
        else:
            # Non-streaming: await the result and send if not None
            output = await result

            # Track statistics
            self.messages_processed += 1
            self.processing_time_total += asyncio.get_event_loop().time() - start_time

            # Send output if produced
            if output is not None:
                await self._send_output(output)

        self.status = NodeStatus.READY

    async def _handle_control_message(self, data: RuntimeData) -> None:
        """
        Handle control messages for flow control (spec 007).

        Control messages allow the pipeline to:
        - Cancel speculative segments (CancelSpeculation)
        - Flush buffers (FlushBuffer)
        - Adjust batching policies (BatchHint)
        - Handle deadline warnings (DeadlineWarning)

        Args:
            data: Control message RuntimeData

        Default behavior:
        - CancelSpeculation: Cancel any ongoing processing for the segment
        - Other messages: Log and ignore

        Nodes can override process_control_message() to implement custom handling.
        """
        try:
            from remotemedia.core.multiprocessing.data import ControlMessageType, ControlMessageMetadata

            if not isinstance(data.metadata, ControlMessageMetadata):
                self.logger.warning(f"Control message received but metadata is not ControlMessageMetadata: {type(data.metadata)}")
                return

            msg_type = data.metadata.message_type
            segment_id = data.metadata.segment_id

            if msg_type == ControlMessageType.CANCEL_SPECULATION:
                # Cancellation: stop any ongoing processing for this segment
                self.logger.info(
                    f"Received CancelSpeculation for segment {segment_id} "
                    f"(from={data.metadata.from_timestamp}, to={data.metadata.to_timestamp})"
                )

                # Default behavior: log the cancellation
                # Nodes that maintain segment state should override process_control_message()
                # to actually cancel/discard the segment

                # Call custom handler if defined
                if hasattr(self, 'process_control_message'):
                    await self.process_control_message(data)

            elif msg_type == ControlMessageType.BATCH_HINT:
                self.logger.debug(f"Received BatchHint: suggested_batch_size={data.metadata.suggested_batch_size}")
                # Batching hints can be used by nodes that accumulate inputs
                if hasattr(self, 'process_control_message'):
                    await self.process_control_message(data)

            elif msg_type == ControlMessageType.DEADLINE_WARNING:
                self.logger.debug(f"Received DeadlineWarning: deadline_us={data.metadata.deadline_us}")
                # Nodes can prioritize processing when deadline approaches
                if hasattr(self, 'process_control_message'):
                    await self.process_control_message(data)

            else:
                self.logger.warning(f"Unknown control message type: {msg_type}")

        except Exception as e:
            self.logger.error(f"Error handling control message: {e}", exc_info=True)
            # Don't raise - control message errors shouldn't stop the pipeline

    async def _process_loop(self) -> None:
        """
        Main processing loop.

        Continuously processes incoming data until stopped.
        """
        self.logger.info(f"🟢 STARTING PROCESS LOOP FOR {self.node_id}, stopped? {self._stop_event.is_set()}")
        loop_iteration = 0

        while not self._stop_event.is_set():
            try:
                loop_iteration += 1
                if loop_iteration % 100 == 0:  # Log every 100 iterations to avoid spam
                    self.logger.debug(f"Process loop iteration {loop_iteration}")

                # Get input data (this will be connected to IPC channels)
                data = await self._receive_input()

                if data is None:
                    # No data available, yield to event loop without artificial delay
                    await asyncio.sleep(0)
                    continue

                # Check if this is a control message (spec 007)
                if hasattr(data, 'is_control_message') and data.is_control_message():
                    # Handle control message instead of regular processing
                    await self._handle_control_message(data)
                    continue

                # Process the message
                await self._process_single_message(data)

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
        Receive input data from iceoryx2 IPC channels.

        Returns:
            Input data or None if no data available
        """
        # Track call count to reduce log spam
        if not hasattr(self, '_receive_call_count'):
            self._receive_call_count = 0
        self._receive_call_count += 1

        if not hasattr(self, '_input_subscriber'):
            if self._receive_call_count == 1:
                self.logger.error("❌ No _input_subscriber available!")
            return None

        try:
            # Log first 5 attempts and then every 100 attempts to diagnose receive issues
            if self._receive_call_count <= 5:
                self.logger.debug(f"📡 [IPC Receive] Polling attempt #{self._receive_call_count} for node '{self.node_id}'")
            elif self._receive_call_count % 100 == 0:
                self.logger.debug(f"[IPC Receive] Polling for input (attempt {self._receive_call_count})")
            
            sample = self._input_subscriber.receive()
            if sample is None:
                # Log first 5 "no data" responses
                if self._receive_call_count <= 5:
                    self.logger.debug(f"📡 [IPC Receive] Attempt #{self._receive_call_count}: No data available yet")
                # No data available
                return None

            # Get payload as bytes
            payload_bytes = bytes(sample.payload())
            self.logger.debug(f"🔵 [IPC Receive] Node '{self.node_id}' received {len(payload_bytes)} bytes from input channel")
            self.logger.debug(f"🔵 First 20 bytes (hex): {payload_bytes[:20].hex() if len(payload_bytes) >= 20 else payload_bytes.hex()}")

            # Deserialize using IPC RuntimeData format matching data_transfer.rs
            # Format: type (1 byte) | session_len (2 bytes) | session | timestamp (8 bytes) | payload_len (4 bytes) | payload
            if len(payload_bytes) < 15:
                self.logger.error(f"Invalid IPC data: too short ({len(payload_bytes)} bytes)")
                return None

            pos = 0

            # Data type (1 byte)
            data_type = payload_bytes[pos]
            pos += 1

            # Session ID (2 bytes length + data)
            session_len = int.from_bytes(payload_bytes[pos:pos+2], 'little')
            pos += 2
            session_id = payload_bytes[pos:pos+session_len].decode('utf-8')
            pos += session_len

            # Timestamp (8 bytes) - not used currently
            timestamp = int.from_bytes(payload_bytes[pos:pos+8], 'little')
            pos += 8

            # Payload length (4 bytes)
            payload_len = int.from_bytes(payload_bytes[pos:pos+4], 'little')
            pos += 4
            payload = payload_bytes[pos:pos+payload_len]

            self.logger.debug(f"Deserialized IPC data: type={data_type}, session={session_id}, payload_len={payload_len}")

            # Store session_id from incoming data for use in outputs
            if session_id:
                self._current_session_id = session_id

            # Convert to RuntimeData based on type
            if HAS_RUNTIME_DATA:
                import numpy as np
                from remotemedia_runtime.runtime_data import RuntimeData as RustRD, numpy_to_audio

                if data_type == 1:  # Audio
                    # Payload is f32 audio samples
                    audio_samples = np.frombuffer(payload, dtype=np.float32)
                    self.logger.info(f"Received audio via IPC: {len(audio_samples)} samples")
                    # Convert to RuntimeData.Audio (assume 24kHz mono for now)
                    rd = numpy_to_audio(audio_samples, 24000, channels=1)
                    rd.session_id = session_id  # Preserve session_id
                    return rd
                elif data_type == 3:  # Text
                    text = payload.decode('utf-8')
                    # Check for ping test message
                    if text == "PING_TEST":
                        self.logger.info(f"✅ 🎯 RECEIVED PING TEST MESSAGE! IPC communication is working! ✅")
                        rd = RustRD.text(text)
                        rd.session_id = session_id  # Preserve session_id
                        return rd
                    self.logger.info(f"Received text via IPC: '{text[:50]}...'")
                    rd = RustRD.text(text)
                    rd.session_id = session_id  # Preserve session_id
                    return rd
                else:
                    self.logger.warning(f"Unsupported IPC data type: {data_type}")
                    return None
            return None
        except Exception as e:
            self.logger.error(f"Error receiving from IPC: {e}", exc_info=True)
            return None

    async def _send_output(self, data: RuntimeData) -> None:
        """
        Send output data to iceoryx2 IPC channels.

        Args:
            data: Output data to send
        """
        if not hasattr(self, '_output_publisher'):
            self.logger.warning("No output publisher - data will be dropped")
            return

        try:
            # Serialize RuntimeData to IPC format matching data_transfer.rs
            # Format: type (1 byte) | session_len (2 bytes) | session | timestamp (8 bytes) | payload_len (4 bytes) | payload
            import time
            import numpy as np

            session_id = getattr(data, 'session_id', None) or 'default'
            timestamp = int(time.time() * 1_000_000)  # microseconds

            # Determine data type and extract payload
            if data.is_text():
                data_type = 3  # Text
                payload = data.as_text().encode('utf-8')
            elif data.is_audio():
                data_type = 1  # Audio
                samples_bytes, sample_rate, channels, format_str, num_samples = data.as_audio()
                payload = samples_bytes
            else:
                self.logger.warning(f"Unsupported data type for IPC send: {data.data_type()}")
                return

            # Build IPC message
            session_bytes = session_id.encode('utf-8')
            message = bytearray()
            message.append(data_type)  # Type (1 byte)
            message.extend(len(session_bytes).to_bytes(2, 'little'))  # Session length (2 bytes)
            message.extend(session_bytes)  # Session ID
            message.extend(timestamp.to_bytes(8, 'little'))  # Timestamp (8 bytes)
            message.extend(len(payload).to_bytes(4, 'little'))  # Payload length (4 bytes)
            message.extend(payload)  # Payload

            self.logger.debug(f"Sending IPC message: type={data_type}, session={session_id}, payload_len={len(payload)}, total={len(message)} bytes")

            # Loan slice and write payload
            import ctypes
            sample = self._output_publisher.loan_slice_uninit(len(message))
            sample_payload = sample.payload()
            for i, byte_val in enumerate(message):
                sample_payload[i] = ctypes.c_uint8(byte_val)
            sample = sample.assume_init()
            sample.send()

            self.logger.debug(f"Successfully sent {len(message)} bytes via iceoryx2")

        except Exception as e:
            self.logger.error(f"Error sending to IPC: {e}", exc_info=True)

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