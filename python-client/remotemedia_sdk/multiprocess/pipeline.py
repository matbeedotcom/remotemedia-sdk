"""
Pipeline builder for multiprocess Python nodes.

This module provides a fluent API for building and configuring
multiprocess pipelines with progress tracking.
"""

from typing import Dict, List, Optional, Callable, Any
from dataclasses import dataclass
from .session import Session, InitProgress, SessionStatus
from .node import MultiprocessNode


@dataclass
class NodeConfig:
    """Configuration for a pipeline node."""

    node_id: str
    """Unique node identifier"""

    node_type: str
    """Node type/class name"""

    config: Dict[str, Any]
    """Node-specific configuration"""

    dependencies: List[str]
    """List of node IDs this node depends on"""


@dataclass
class ConnectionConfig:
    """Configuration for a node-to-node connection."""

    from_node: str
    """Source node ID"""

    to_node: str
    """Destination node ID"""

    channel_name: Optional[str] = None
    """Optional channel name (auto-generated if not provided)"""


class Pipeline:
    """
    Builder for multiprocess Python node pipelines.

    Provides a fluent API for configuring and building pipelines
    with multiple Python nodes running in separate processes.

    Example:
        ```python
        from remotemedia_sdk.multiprocess import Pipeline

        # Build a speech-to-speech pipeline
        pipeline = (Pipeline("s2s_pipeline")
            .add_node("vad", "SileroVAD", config={"threshold": 0.5})
            .add_node("s2s", "LFM2Audio", config={"model_path": "./lfm2.onnx"})
            .add_node("tts", "VibeVoice", config={"model_path": "./vibe.onnx"})
            .connect("vad", "s2s")
            .connect("s2s", "tts")
        )

        # Initialize with progress tracking
        def on_progress(progress):
            print(f"[{progress.node_id}] {progress.message} - {progress.progress * 100:.0f}%")

        pipeline.initialize(timeout_secs=30, progress_callback=on_progress)

        # Run the pipeline
        pipeline.run()

        # Cleanup
        pipeline.terminate()
        ```
    """

    def __init__(self, pipeline_id: str):
        """
        Create a new pipeline builder.

        Args:
            pipeline_id: Unique identifier for this pipeline
        """
        self.pipeline_id = pipeline_id
        self._nodes: Dict[str, NodeConfig] = {}
        self._connections: List[ConnectionConfig] = []
        self._session: Optional[Session] = None
        self._global_config: Dict[str, Any] = {
            "max_processes": 10,
            "channel_capacity": 100,
            "init_timeout_secs": 30,
            "enable_backpressure": True,
        }

    def add_node(
        self,
        node_id: str,
        node_type: str,
        config: Optional[Dict[str, Any]] = None,
        dependencies: Optional[List[str]] = None
    ) -> "Pipeline":
        """
        Add a node to the pipeline.

        Args:
            node_id: Unique identifier for this node
            node_type: Node type/class name
            config: Node-specific configuration (optional)
            dependencies: List of node IDs this node depends on (optional)

        Returns:
            Self for method chaining

        Example:
            ```python
            pipeline.add_node(
                "transcriber",
                "WhisperTranscriber",
                config={"model": "base.en"},
                dependencies=["audio_input"]
            )
            ```
        """
        if node_id in self._nodes:
            raise ValueError(f"Node {node_id} already exists in pipeline")

        node_config = NodeConfig(
            node_id=node_id,
            node_type=node_type,
            config=config or {},
            dependencies=dependencies or []
        )

        self._nodes[node_id] = node_config
        return self

    def connect(
        self,
        from_node: str,
        to_node: str,
        channel_name: Optional[str] = None
    ) -> "Pipeline":
        """
        Connect two nodes with a data channel.

        Args:
            from_node: Source node ID
            to_node: Destination node ID
            channel_name: Optional channel name (auto-generated if not provided)

        Returns:
            Self for method chaining

        Raises:
            ValueError: If either node doesn't exist

        Example:
            ```python
            pipeline.connect("microphone", "vad", channel_name="audio_stream")
            ```
        """
        if from_node not in self._nodes:
            raise ValueError(f"Source node {from_node} not found in pipeline")
        if to_node not in self._nodes:
            raise ValueError(f"Destination node {to_node} not found in pipeline")

        connection = ConnectionConfig(
            from_node=from_node,
            to_node=to_node,
            channel_name=channel_name
        )

        self._connections.append(connection)
        return self

    def set_config(self, key: str, value: Any) -> "Pipeline":
        """
        Set a global configuration parameter.

        Args:
            key: Configuration key
            value: Configuration value

        Returns:
            Self for method chaining

        Example:
            ```python
            pipeline.set_config("max_processes", 20)
            pipeline.set_config("init_timeout_secs", 60)
            ```
        """
        self._global_config[key] = value
        return self

    def initialize(
        self,
        timeout_secs: Optional[float] = None,
        progress_callback: Optional[Callable[[InitProgress], None]] = None
    ) -> "Pipeline":
        """
        Initialize the pipeline and all nodes.

        This method:
        1. Creates a new session
        2. Spawns all node processes
        3. Establishes inter-node connections
        4. Waits for all nodes to complete initialization

        Args:
            timeout_secs: Maximum time to wait for initialization (default: from config)
            progress_callback: Optional callback for progress updates

        Returns:
            Self for method chaining

        Raises:
            RuntimeError: If initialization fails or times out

        Example:
            ```python
            def show_progress(p):
                print(f"{p.node_id}: {p.message}")

            pipeline.initialize(timeout_secs=60, progress_callback=show_progress)
            ```
        """
        if self._session is not None:
            raise RuntimeError("Pipeline already initialized")

        # Validate pipeline structure
        self._validate()

        # Create session
        self._session = Session(self.pipeline_id)

        # TODO: Spawn node processes via Rust runtime
        # This would call into the Rust MultiprocessExecutor to:
        # 1. Create the session
        # 2. Spawn each node process
        # 3. Create IPC channels for connections
        # 4. Initialize each node

        # Use timeout from config if not specified
        timeout = timeout_secs or self._global_config.get("init_timeout_secs", 30)

        # Wait for initialization
        if not self._session.wait_for_initialization(
            timeout_secs=timeout,
            progress_callback=progress_callback
        ):
            raise RuntimeError(
                f"Pipeline initialization timeout after {timeout}s"
            )

        return self

    def run(self) -> None:
        """
        Start running the pipeline.

        Raises:
            RuntimeError: If pipeline is not initialized
        """
        if self._session is None:
            raise RuntimeError("Pipeline not initialized. Call initialize() first.")

        self._session.run()

    def terminate(self) -> None:
        """
        Terminate the pipeline and cleanup resources.
        """
        if self._session is not None:
            self._session.terminate()
            self._session = None

    def get_session(self) -> Optional[Session]:
        """
        Get the current session.

        Returns:
            Session object if initialized, None otherwise
        """
        return self._session

    def get_nodes(self) -> Dict[str, NodeConfig]:
        """
        Get all node configurations.

        Returns:
            Dictionary of node ID to NodeConfig
        """
        return self._nodes.copy()

    def get_connections(self) -> List[ConnectionConfig]:
        """
        Get all connection configurations.

        Returns:
            List of connection configurations
        """
        return self._connections.copy()

    def _validate(self) -> None:
        """
        Validate the pipeline structure.

        Raises:
            ValueError: If pipeline structure is invalid
        """
        if not self._nodes:
            raise ValueError("Pipeline has no nodes")

        # Check for circular dependencies
        visited = set()
        rec_stack = set()

        def has_cycle(node_id: str) -> bool:
            visited.add(node_id)
            rec_stack.add(node_id)

            node = self._nodes.get(node_id)
            if node:
                for dep in node.dependencies:
                    if dep not in visited:
                        if has_cycle(dep):
                            return True
                    elif dep in rec_stack:
                        return True

            rec_stack.remove(node_id)
            return False

        for node_id in self._nodes:
            if node_id not in visited:
                if has_cycle(node_id):
                    raise ValueError(
                        f"Circular dependency detected involving node {node_id}"
                    )

        # Validate all connections reference existing nodes
        for conn in self._connections:
            if conn.from_node not in self._nodes:
                raise ValueError(
                    f"Connection references non-existent source node: {conn.from_node}"
                )
            if conn.to_node not in self._nodes:
                raise ValueError(
                    f"Connection references non-existent destination node: {conn.to_node}"
                )

        # Validate dependencies reference existing nodes
        for node_id, node in self._nodes.items():
            for dep in node.dependencies:
                if dep not in self._nodes:
                    raise ValueError(
                        f"Node {node_id} depends on non-existent node: {dep}"
                    )


class PipelineBuilder:
    """
    Alternative builder class with a more explicit API.

    This provides a slightly different API style compared to Pipeline,
    with explicit build() step.
    """

    @staticmethod
    def create(pipeline_id: str) -> Pipeline:
        """
        Create a new pipeline.

        Args:
            pipeline_id: Unique identifier for the pipeline

        Returns:
            New Pipeline instance
        """
        return Pipeline(pipeline_id)
