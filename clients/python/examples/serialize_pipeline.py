"""
Example: Serializing pipelines to Rust-compatible manifests

This example demonstrates how to use Pipeline.serialize() to create
JSON manifests that can be executed by the Rust runtime.
"""

import json
from remotemedia.core.pipeline import Pipeline
from remotemedia.nodes.calculator import CalculatorNode
from remotemedia.nodes.io_nodes import DataSourceNode, DataSinkNode
from remotemedia.nodes.base import PassThroughNode


def main():
    # Create a simple pipeline
    pipeline = Pipeline(name="example-pipeline")
    pipeline.add_node(DataSourceNode(name="input"))
    pipeline.add_node(CalculatorNode(name="multiply", operation="multiply", operand=2))
    pipeline.add_node(CalculatorNode(name="add", operation="add", operand=10))
    pipeline.add_node(DataSinkNode(name="output"))

    print("=" * 60)
    print("Pipeline Serialization Example")
    print("=" * 60)
    print(f"\nPipeline: {pipeline.name}")
    print(f"Nodes: {len(pipeline.nodes)}")
    print()

    # Serialize to manifest
    manifest_json = pipeline.serialize(
        description="Example pipeline demonstrating serialization",
        include_capabilities=True
    )

    # Parse and pretty-print
    manifest_dict = json.loads(manifest_json)

    print("Generated Manifest:")
    print("-" * 60)
    print(json.dumps(manifest_dict, indent=2))
    print()

    # Show specific sections
    print("Metadata:")
    print(f"  Name: {manifest_dict['metadata']['name']}")
    print(f"  Description: {manifest_dict['metadata'].get('description', 'N/A')}")
    print(f"  Created: {manifest_dict['metadata']['created_at']}")
    print()

    print("Nodes:")
    for node in manifest_dict['nodes']:
        print(f"  - {node['id']}: {node['node_type']}")
        if node.get('params'):
            print(f"    Params: {node['params']}")
    print()

    print("Connections:")
    for conn in manifest_dict['connections']:
        print(f"  {conn['from']} -> {conn['to']}")
    print()

    print("=" * 60)
    print("✓ Manifest generated successfully!")
    print("✓ This manifest can be executed by the Rust runtime")
    print("=" * 60)


if __name__ == "__main__":
    main()
