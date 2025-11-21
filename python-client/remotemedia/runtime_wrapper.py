"""
Python wrapper for remotemedia.runtime FFI with instance support.

This module provides convenience wrappers that enable passing Python Node
instances directly to the Rust runtime, with automatic type detection and
manifest conversion.
"""

import json
import logging
from typing import Any, Dict, List, Union, Optional
from datetime import datetime, timezone

logger = logging.getLogger(__name__)


def _convert_mixed_list_to_manifest(items: List[Union['Node', Dict[str, Any]]]) -> str:
    """
    Convert a mixed list of Node instances and dict manifests to unified manifest JSON.

    T030-T033: Handle mixed lists by:
    - Converting Node instances to manifest dicts via .to_manifest()
    - Preserving dict manifest entries as-is
    - Generating connections between all nodes in sequence

    Args:
        items: List containing Node instances and/or dict manifests

    Returns:
        JSON string of unified manifest

    Raises:
        ValueError: If items are invalid
    """
    from remotemedia.core.node import Node

    # T032: Convert each item to manifest dict
    manifest_nodes = []
    for i, item in enumerate(items):
        if isinstance(item, Node):
            # Convert Node instance to manifest dict
            node_manifest = item.to_manifest(include_capabilities=True)
            # Ensure unique ID with index
            node_manifest['id'] = f"{item.name}_{i}"
            manifest_nodes.append(node_manifest)
        elif isinstance(item, dict):
            # T031: Preserve dict manifest as-is (with ID adjustment)
            node_dict = item.copy()
            if 'id' not in node_dict:
                # Generate ID if missing
                node_type = node_dict.get('node_type', 'unknown')
                node_dict['id'] = f"{node_type}_{i}"
            else:
                # Ensure unique ID with index
                node_dict['id'] = f"{node_dict['id']}_{i}"
            manifest_nodes.append(node_dict)
        else:
            raise ValueError(f"Invalid item type at index {i}: {type(item).__name__}")

    # T033: Generate connections between nodes in sequence
    connections = []
    for i in range(len(manifest_nodes) - 1):
        connections.append({
            "from": manifest_nodes[i]['id'],
            "to": manifest_nodes[i + 1]['id']
        })

    # Build complete manifest
    manifest = {
        "version": "v1",
        "metadata": {
            "name": "mixed-pipeline",
            "created_at": datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z'),
            "description": f"Mixed pipeline with {len(manifest_nodes)} nodes"
        },
        "nodes": manifest_nodes,
        "connections": connections
    }

    return json.dumps(manifest, indent=2)


def _convert_dict_list_to_manifest(items: List[Dict[str, Any]]) -> str:
    """
    Convert a list of dict manifests to unified manifest JSON.

    Args:
        items: List of dict manifests

    Returns:
        JSON string of unified manifest
    """
    # Ensure all dicts have IDs
    manifest_nodes = []
    for i, item in enumerate(items):
        node_dict = item.copy()
        if 'id' not in node_dict:
            node_type = node_dict.get('node_type', 'unknown')
            node_dict['id'] = f"{node_type}_{i}"
        else:
            node_dict['id'] = f"{node_dict['id']}_{i}"
        manifest_nodes.append(node_dict)

    # Generate connections
    connections = []
    for i in range(len(manifest_nodes) - 1):
        connections.append({
            "from": manifest_nodes[i]['id'],
            "to": manifest_nodes[i + 1]['id']
        })

    manifest = {
        "version": "v1",
        "metadata": {
            "name": "dict-list-pipeline",
            "created_at": datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')
        },
        "nodes": manifest_nodes,
        "connections": connections
    }

    return json.dumps(manifest, indent=2)


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

    # Type 2: List of Node instances OR mixed list (T029-T034: US2 support)
    elif isinstance(pipeline_or_manifest, list):
        logger.debug(f"Detected list of {len(pipeline_or_manifest)} items")

        if len(pipeline_or_manifest) == 0:
            raise ValueError("Cannot execute empty pipeline (empty list)")

        # T029: Check if list contains mixed types (Nodes + dicts)
        has_nodes = any(isinstance(item, Node) for item in pipeline_or_manifest)
        has_dicts = any(isinstance(item, dict) for item in pipeline_or_manifest)
        has_invalid = any(not isinstance(item, (Node, dict)) for item in pipeline_or_manifest)

        # T034: Validate - reject invalid types (e.g., raw strings, numbers)
        if has_invalid:
            invalid_items = [(i, type(item).__name__) for i, item in enumerate(pipeline_or_manifest)
                             if not isinstance(item, (Node, dict))]
            raise TypeError(
                f"List items must be Node instances or dict manifests. "
                f"Found invalid types at positions: {invalid_items}"
            )

        # REGISTRY FIX: Use direct instance execution for pure Node lists
        if has_nodes and not has_dicts:
            # Pure Node list - use direct instance execution (bypasses registry)
            logger.debug(f"Using direct instance execution for {len(pipeline_or_manifest)} Nodes")

            if hasattr(_runtime, 'execute_pipeline_with_instances'):
                # New direct execution path (Feature 011 complete integration)
                logger.debug("Calling execute_pipeline_with_instances (bypasses registry)")
                return _runtime.execute_pipeline_with_instances(
                    pipeline_or_manifest,  # Pass instances directly
                    None,  # No initial input
                    enable_metrics
                )
            else:
                # Fallback to manifest (for older runtime versions)
                logger.debug("execute_pipeline_with_instances not available, using manifest fallback")
                temp_pipeline = Pipeline(name="instance-pipeline", nodes=pipeline_or_manifest)
                manifest_json = temp_pipeline.serialize()

        # T030: Handle mixed list - convert to unified manifest
        elif has_nodes and has_dicts:
            logger.debug(f"Detected mixed list: {sum(1 for x in pipeline_or_manifest if isinstance(x, Node))} Nodes, "
                         f"{sum(1 for x in pipeline_or_manifest if isinstance(x, dict))} dicts")
            manifest_json = _convert_mixed_list_to_manifest(pipeline_or_manifest)

        # Pure dict list - convert to manifest
        elif has_dicts and not has_nodes:
            logger.debug("Converting list of dict manifests to unified manifest")
            manifest_json = _convert_dict_list_to_manifest(pipeline_or_manifest)

        else:
            # Empty or edge case
            raise ValueError("Cannot process empty or invalid list")

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


async def execute_pipeline_with_input(
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

    # Reuse type detection logic from execute_pipeline
    if hasattr(pipeline_or_manifest, 'serialize'):
        logger.debug("Detected Pipeline instance, calling .serialize()")
        manifest_json = pipeline_or_manifest.serialize()

    elif isinstance(pipeline_or_manifest, list):
        logger.debug(f"Detected list of {len(pipeline_or_manifest)} items")

        if len(pipeline_or_manifest) == 0:
            raise ValueError("Cannot execute empty pipeline (empty list)")

        # Check for mixed types (same as execute_pipeline)
        has_nodes = any(isinstance(item, Node) for item in pipeline_or_manifest)
        has_dicts = any(isinstance(item, dict) for item in pipeline_or_manifest)
        has_invalid = any(not isinstance(item, (Node, dict)) for item in pipeline_or_manifest)

        if has_invalid:
            invalid_items = [(i, type(item).__name__) for i, item in enumerate(pipeline_or_manifest)
                             if not isinstance(item, (Node, dict))]
            raise TypeError(
                f"List items must be Node instances or dict manifests. "
                f"Found invalid types at positions: {invalid_items}"
            )

        # REGISTRY FIX: Use direct instance execution for pure Node lists with input
        if has_nodes and not has_dicts:
            logger.debug(f"Using direct instance execution for {len(pipeline_or_manifest)} Nodes with {len(input_data)} inputs")

            if hasattr(_runtime, 'execute_pipeline_with_instances'):
                # Execute with each input item via instance executor (in async context)
                logger.debug("Processing inputs via execute_pipeline_with_instances")

                async def process_with_instances():
                    results = []
                    for input_item in input_data:
                        result = await _runtime.execute_pipeline_with_instances(
                            pipeline_or_manifest,
                            input_item,
                            enable_metrics
                        )
                        if enable_metrics and isinstance(result, dict):
                            results.append(result['outputs'])
                        else:
                            results.append(result)
                    return results

                return await process_with_instances()
            else:
                # Fallback to manifest
                logger.debug("execute_pipeline_with_instances not available, using manifest fallback")
                temp_pipeline = Pipeline(name="instance-pipeline", nodes=pipeline_or_manifest)
                manifest_json = temp_pipeline.serialize()

        # Handle mixed, pure Node, or pure dict lists
        elif has_nodes and has_dicts:
            logger.debug("Detected mixed list")
            manifest_json = _convert_mixed_list_to_manifest(pipeline_or_manifest)
        elif has_dicts and not has_nodes:
            logger.debug("Converting list of dict manifests")
            manifest_json = _convert_dict_list_to_manifest(pipeline_or_manifest)
        else:
            raise ValueError("Cannot process empty or invalid list")

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
