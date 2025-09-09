#!/usr/bin/env python3
"""
Quick script to register a test pipeline for JavaScript client testing.
"""

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from remotemedia import Pipeline
from remotemedia.nodes import PassThroughNode, CalculatorNode
from remotemedia.nodes.io_nodes import DataSourceNode, DataSinkNode
from remotemedia.core.pipeline_registry import get_global_registry


async def register_test_pipelines():
    """Register test pipelines for JavaScript client."""
    print("🔧 Registering test pipelines...")
    
    registry = get_global_registry()
    
    # 1. Simple calculator pipeline
    calc_pipeline = Pipeline(name="JavaScriptCalculatorPipeline")
    calc_pipeline.add_node(DataSourceNode(name="js_input", buffer_size=50))
    calc_pipeline.add_node(CalculatorNode(name="calculator", verbose=True))
    calc_pipeline.add_node(DataSinkNode(name="js_output", buffer_output=True))
    
    calc_id = await registry.register_pipeline(
        name="js_calculator",
        definition=calc_pipeline.export_definition(),
        metadata={
            "category": "math",
            "description": "Calculator pipeline for JavaScript clients",
            "input_type": "calculation_request",
            "output_type": "calculation_result"
        },
        category="math"
    )
    
    # 2. Simple text processing pipeline
    text_pipeline = Pipeline(name="JavaScriptTextPipeline")
    text_pipeline.add_node(DataSourceNode(name="text_input", buffer_size=50))
    text_pipeline.add_node(PassThroughNode(name="text_processor"))
    text_pipeline.add_node(DataSinkNode(name="text_output", buffer_output=True))
    
    text_id = await registry.register_pipeline(
        name="js_text_processor",
        definition=text_pipeline.export_definition(),
        metadata={
            "category": "text",
            "description": "Text processing pipeline for JavaScript clients",
            "input_type": "text",
            "output_type": "processed_text"
        },
        category="text"
    )
    
    print(f"✅ Registered calculator pipeline: {calc_id}")
    print(f"✅ Registered text pipeline: {text_id}")
    
    # List all pipelines
    pipelines = registry.list_pipelines()
    print(f"\n📋 Total registered pipelines: {len(pipelines)}")
    for p in pipelines:
        print(f"  - {p['name']} ({p['category']}): {p['description']}")
    
    return [calc_id, text_id]


if __name__ == "__main__":
    asyncio.run(register_test_pipelines())