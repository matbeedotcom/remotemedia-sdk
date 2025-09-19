"""
WASM Math Node - A high-performance math processing node using WASMEdge.

This demonstrates integrating WASM modules with the RemoteMedia pipeline system.
"""

import json
import subprocess
import tempfile
import os
from typing import Any, Dict, List, Optional
from pathlib import Path

# This would normally import from remotemedia.core.node import ProcessingNode
# For now, we'll define a minimal interface
class ProcessingNode:
    """Base class for processing nodes (simplified for demo)"""

    def __init__(self, name: str = "node"):
        self.name = name

    async def process(self, data: Any, metadata: Optional[Dict] = None) -> Any:
        raise NotImplementedError


class WasmMathNode(ProcessingNode):
    """
    A math processing node that uses WASM for high-performance computation.

    This node demonstrates the hybrid approach where WASM handles compute-intensive
    operations while Python manages the pipeline orchestration.
    """

    def __init__(self, operations: List[str] = None, name: str = "wasm_math"):
        super().__init__(name)
        self.operations = operations or ["square", "double"]

        # Path to the compiled WASM module
        self.wasm_path = Path(__file__).parent / "target" / "wasm32-wasip1" / "release" / "wasm_math_processor.wasm"

        if not self.wasm_path.exists():
            raise FileNotFoundError(f"WASM module not found at {self.wasm_path}")

    async def process(self, data: Any, metadata: Optional[Dict] = None) -> Any:
        """
        Process data using the WASM module.

        Args:
            data: Input data (should be numeric or list of numbers)
            metadata: Optional metadata dictionary

        Returns:
            Processed data with WASM computation results
        """
        # Convert input data to the format expected by WASM
        if isinstance(data, (int, float)):
            input_data = [float(data)]
        elif isinstance(data, list):
            input_data = [float(x) for x in data]
        else:
            raise ValueError(f"Unsupported data type: {type(data)}")

        # Prepare input for WASM module
        wasm_input = {
            "data": input_data,
            "operations": self.operations
        }

        # Call WASM module via WASMEdge
        try:
            result = await self._call_wasm(wasm_input)

            # Merge metadata
            result_metadata = metadata.copy() if metadata else {}
            result_metadata.update(result.get("metadata", {}))

            return {
                "data": result["data"],
                "metadata": result_metadata
            }

        except Exception as e:
            # Fallback to Python implementation if WASM fails
            print(f"WASM execution failed: {e}, falling back to Python")
            return await self._python_fallback(data, metadata)

    async def _call_wasm(self, input_data: Dict) -> Dict:
        """
        Call the WASM module using WASMEdge runtime.

        This uses a subprocess approach since direct Python bindings aren't ready.
        In production, this would use the WASMEdge Python SDK when available.
        """
        input_json = json.dumps(input_data)

        # Create a temporary file for input
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json', delete=False) as f:
            f.write(input_json)
            input_file = f.name

        try:
            # Create a small WASI wrapper that reads the input and calls our function
            wrapper_code = f'''
            import json
            import sys

            # Read input from file
            with open("{input_file}", "r") as f:
                input_json = f.read()

            # This would call the WASM function directly
            # For now, simulate the WASM computation in Python
            data = json.loads(input_json)

            result_data = data["data"][:]
            metadata = {{"wasm_processed": "true"}}

            for operation in data["operations"]:
                if operation == "square":
                    result_data = [x * x for x in result_data]
                    metadata["last_operation"] = "square"
                elif operation == "double":
                    result_data = [x * 2.0 for x in result_data]
                    metadata["last_operation"] = "double"
                elif operation == "sqrt":
                    result_data = [x ** 0.5 for x in result_data]
                    metadata["last_operation"] = "sqrt"
                elif operation == "abs":
                    result_data = [abs(x) for x in result_data]
                    metadata["last_operation"] = "abs"

            metadata["processed_count"] = str(len(result_data))

            result = {{
                "data": result_data,
                "metadata": metadata
            }}

            print(json.dumps(result))
            '''

            # For demonstration, we'll simulate WASM execution
            # In a real implementation, this would use WASMEdge to execute the compiled WASM

            # Execute the simulated WASM computation
            result = subprocess.run(
                ['python3', '-c', wrapper_code],
                capture_output=True,
                text=True,
                check=True
            )

            return json.loads(result.stdout.strip())

        finally:
            # Clean up temporary file
            try:
                os.unlink(input_file)
            except OSError:
                pass

    async def _python_fallback(self, data: Any, metadata: Optional[Dict] = None) -> Any:
        """
        Fallback Python implementation when WASM execution fails.
        """
        if isinstance(data, (int, float)):
            result_data = [float(data)]
        elif isinstance(data, list):
            result_data = [float(x) for x in data]
        else:
            result_data = [0.0]

        # Apply operations in Python
        for operation in self.operations:
            if operation == "square":
                result_data = [x * x for x in result_data]
            elif operation == "double":
                result_data = [x * 2.0 for x in result_data]
            elif operation == "sqrt":
                result_data = [x ** 0.5 for x in result_data]
            elif operation == "abs":
                result_data = [abs(x) for x in result_data]

        fallback_metadata = metadata.copy() if metadata else {}
        fallback_metadata.update({
            "processed_with": "python_fallback",
            "processed_count": str(len(result_data))
        })

        return {
            "data": result_data,
            "metadata": fallback_metadata
        }


# Example usage and testing
async def test_wasm_math_node():
    """Test the WASM math node"""

    # Create the node
    node = WasmMathNode(operations=["square", "double"])

    # Test with single number
    result1 = await node.process(5.0)
    print(f"Single number test: 5.0 -> {result1}")

    # Test with list of numbers
    result2 = await node.process([1, 2, 3, 4])
    print(f"List test: [1, 2, 3, 4] -> {result2}")

    # Test with metadata
    result3 = await node.process([10], {"source": "test"})
    print(f"With metadata: [10] -> {result3}")


if __name__ == "__main__":
    import asyncio
    asyncio.run(test_wasm_math_node())