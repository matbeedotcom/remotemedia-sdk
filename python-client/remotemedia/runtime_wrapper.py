"""
Python wrapper for remotemedia.runtime FFI with instance support.

This module provides convenience wrappers that enable passing Python Node
instances directly to the Rust runtime, with automatic type detection and
manifest conversion.
"""

import json
import logging
from typing import Any, Dict, List, Union, Optional

logger = logging.getLogger(__name__)


def execute_pipeline(
    pipeline_or_manifest: Union['Pipeline', List['Node'], str, Dict[str, Any]],
    enable_metrics: bool = False
) -> Any:
    """
    Execute a pipeline using the Rust runtime with support for Node instances.

    This wrapper provides automatic type detection and conversion:
    - Pipeline instance → automatically serialized to manifest JSON
    - List[Node] → converted to Pipeline → serialized to manifest JSON
    - Dict manifest → converted to JSON string
    - JSON string → passed directly to Rust FFI

    Args:
        pipeline_or_manifest: Pipeline instance, list of Node instances,
                              JSON manifest string, or manifest dict
        enable_metrics: Enable performance metrics collection

    Returns:
        Pipeline execution result (type depends on final node output)
        If enable_metrics=True, returns {"outputs": <result>, "metrics": <metrics>}

    Raises:
        TypeError: Invalid input type
        ValueError: Invalid manifest format or empty pipeline
        ImportError: remotemedia.runtime module not available
        RuntimeError: Rust runtime execution failed

    Examples:
        >>> # Option 1: Pass Pipeline instance
        >>> from remotemedia.core.pipeline import Pipeline
        >>> from remotemedia.nodes import PassThroughNode
        >>> pipeline = Pipeline("my-pipeline")
        >>> pipeline.add_node(PassThroughNode(name="pass"))
        >>> result = await execute_pipeline(pipeline)

        >>> # Option 2: Pass list of Node instances
        >>> nodes = [PassThroughNode(name="pass"), CalculatorNode(name="calc")]
        >>> result = await execute_pipeline(nodes)

        >>> # Option 3: Pass manifest dict
        >>> manifest = {"version": "v1", "nodes": [...], "connections": [...]}
        >>> result = await execute_pipeline(manifest)

        >>> # Option 4: Pass JSON string (backward compatible)
        >>> manifest_json = json.dumps(manifest)
        >>> result = await execute_pipeline(manifest_json)
    """
    # T010: Type detection logic
    from remotemedia.core.pipeline import Pipeline
    from remotemedia.core.node import Node

    # Import Rust runtime module
    try:
        import remotemedia.runtime as _runtime
    except ImportError as e:
        raise ImportError(
            f"remotemedia.runtime module not available. "
            f"Install remotemedia-ffi to use Rust acceleration. Error: {e}"
        ) from e

    # Detect input type and convert to manifest JSON
    manifest_json: str

    # Type 1: Pipeline instance
    if hasattr(pipeline_or_manifest, 'serialize'):
        logger.debug("Detected Pipeline instance, calling .serialize()")
        manifest_json = pipeline_or_manifest.serialize()

    # Type 2: List of Node instances
    elif isinstance(pipeline_or_manifest, list):
        logger.debug(f"Detected list of {len(pipeline_or_manifest)} items")

        # Validate all items are Node instances
        if not all(isinstance(item, Node) for item in pipeline_or_manifest):
            invalid_types = [type(item).__name__ for item in pipeline_or_manifest if not isinstance(item, Node)]
            raise TypeError(
                f"All items in list must be Node instances. "
                f"Found invalid types: {invalid_types}"
            )

        if len(pipeline_or_manifest) == 0:
            raise ValueError("Cannot execute empty pipeline (empty list)")

        # Create Pipeline from nodes and serialize
        logger.debug("Converting list of Nodes to Pipeline")
        temp_pipeline = Pipeline(name="instance-pipeline", nodes=pipeline_or_manifest)
        manifest_json = temp_pipeline.serialize()

    # Type 3: Dict manifest
    elif isinstance(pipeline_or_manifest, dict):
        logger.debug("Detected manifest dict, converting to JSON")
        manifest_json = json.dumps(pipeline_or_manifest)

    # Type 4: JSON string (backward compatible)
    elif isinstance(pipeline_or_manifest, str):
        logger.debug("Detected JSON string manifest (backward compatible)")
        manifest_json = pipeline_or_manifest

    # Invalid type
    else:
        raise TypeError(
            f"Expected Pipeline, list of Nodes, dict, or str, "
            f"got {type(pipeline_or_manifest).__name__}"
        )

    # Call Rust FFI with manifest JSON
    logger.debug(f"Calling Rust runtime execute_pipeline (metrics={enable_metrics})")
    return _runtime.execute_pipeline(manifest_json, enable_metrics)


def execute_pipeline_with_input(
    pipeline_or_manifest: Union['Pipeline', List['Node'], str, Dict[str, Any]],
    input_data: List[Any],
    enable_metrics: bool = False
) -> Any:
    """
    Execute a pipeline with input data, supporting Node instances.

    Args:
        pipeline_or_manifest: Pipeline instance, list of Node instances,
                              JSON manifest string, or manifest dict
        input_data: List of input items to process through the pipeline
        enable_metrics: Enable performance metrics collection

    Returns:
        List of pipeline execution results (one per input item)
        If enable_metrics=True, returns {"outputs": <results>, "metrics": <metrics>}

    Raises:
        TypeError: Invalid input type or empty input_data
        ValueError: Invalid manifest format or empty pipeline
        ImportError: remotemedia.runtime module not available
        RuntimeError: Rust runtime execution failed

    Examples:
        >>> nodes = [CalculatorNode(name="calc", operation="multiply", operand=2)]
        >>> input_data = [1, 2, 3, 4, 5]
        >>> results = await execute_pipeline_with_input(nodes, input_data)
        >>> print(results)  # [2, 4, 6, 8, 10]
    """
    from remotemedia.core.pipeline import Pipeline
    from remotemedia.core.node import Node

    # Import Rust runtime module
    try:
        import remotemedia.runtime as _runtime
    except ImportError as e:
        raise ImportError(
            f"remotemedia.runtime module not available. "
            f"Install remotemedia-ffi to use Rust acceleration. Error: {e}"
        ) from e

    # Validate input_data
    if not isinstance(input_data, list):
        raise TypeError(f"input_data must be a list, got {type(input_data).__name__}")

    if len(input_data) == 0:
        raise ValueError("input_data cannot be empty")

    # Detect pipeline type and convert to manifest JSON (same logic as execute_pipeline)
    manifest_json: str

    if hasattr(pipeline_or_manifest, 'serialize'):
        logger.debug("Detected Pipeline instance, calling .serialize()")
        manifest_json = pipeline_or_manifest.serialize()
    elif isinstance(pipeline_or_manifest, list):
        logger.debug(f"Detected list of {len(pipeline_or_manifest)} Node instances")

        if not all(isinstance(item, Node) for item in pipeline_or_manifest):
            invalid_types = [type(item).__name__ for item in pipeline_or_manifest if not isinstance(item, Node)]
            raise TypeError(
                f"All items in list must be Node instances. "
                f"Found invalid types: {invalid_types}"
            )

        if len(pipeline_or_manifest) == 0:
            raise ValueError("Cannot execute empty pipeline (empty list)")

        temp_pipeline = Pipeline(name="instance-pipeline", nodes=pipeline_or_manifest)
        manifest_json = temp_pipeline.serialize()
    elif isinstance(pipeline_or_manifest, dict):
        logger.debug("Detected manifest dict, converting to JSON")
        manifest_json = json.dumps(pipeline_or_manifest)
    elif isinstance(pipeline_or_manifest, str):
        logger.debug("Detected JSON string manifest (backward compatible)")
        manifest_json = pipeline_or_manifest
    else:
        raise TypeError(
            f"Expected Pipeline, list of Nodes, dict, or str, "
            f"got {type(pipeline_or_manifest).__name__}"
        )

    # Call Rust FFI with manifest JSON and input data
    logger.debug(f"Calling Rust runtime execute_pipeline_with_input (metrics={enable_metrics}, inputs={len(input_data)})")
    return _runtime.execute_pipeline_with_input(manifest_json, input_data, enable_metrics)


__all__ = [
    'execute_pipeline',
    'execute_pipeline_with_input',
]
