"""Pipeline executor using FFI."""

import logging
import time
from dataclasses import dataclass
from typing import Any, Optional

logger = logging.getLogger(__name__)


@dataclass
class ExecutionResult:
    """Result of pipeline execution."""

    output_type: str
    data: Optional[bytes]
    metadata: dict[str, Any]
    timing_ms: float


class PipelineExecutor:
    """Executes pipelines using the RemoteMedia runtime."""

    def __init__(self, pipeline: dict[str, Any]) -> None:
        self.pipeline = pipeline
        self.manifest_json = self._to_json(pipeline)

    def _to_json(self, pipeline: dict[str, Any]) -> str:
        """Convert pipeline manifest to JSON."""
        import json

        return json.dumps(pipeline)

    async def execute(
        self,
        input_data: Optional[bytes],
        config: Optional[dict[str, Any]] = None,
    ) -> ExecutionResult:
        """Execute the pipeline with input data."""
        start_time = time.time()

        try:
            # TODO: Use remotemedia FFI for actual execution
            # from remotemedia_runtime import execute_pipeline
            # result = execute_pipeline(self.manifest_json, input_data)

            # For now, return placeholder
            logger.info(f"Executing pipeline: {self.pipeline.get('name')}")

            # Simulate processing
            output_type = self._detect_output_type()
            output_data = self._simulate_output(input_data, output_type)

            timing_ms = (time.time() - start_time) * 1000

            return ExecutionResult(
                output_type=output_type,
                data=output_data,
                metadata={
                    "pipeline": self.pipeline.get("name"),
                    "config": config or {},
                },
                timing_ms=timing_ms,
            )

        except Exception as e:
            logger.error(f"Pipeline execution failed: {e}")
            raise

    def _detect_output_type(self) -> str:
        """Detect output type from pipeline."""
        nodes = self.pipeline.get("nodes", [])
        if not nodes:
            return "unknown"

        last_node = nodes[-1]
        node_type = last_node.get("node_type", "")

        if "Audio" in node_type or "TTS" in node_type:
            return "audio"
        if "Text" in node_type or "STT" in node_type or "Whisper" in node_type:
            return "text"
        if "Image" in node_type:
            return "image"

        return "unknown"

    def _simulate_output(
        self, input_data: Optional[bytes], output_type: str
    ) -> Optional[bytes]:
        """Simulate pipeline output for testing."""
        if output_type == "text":
            return b"[Simulated transcription output]"
        if output_type == "audio":
            # Return empty audio (placeholder)
            return b"\x00" * 1000
        return input_data
