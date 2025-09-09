"""
Calculator node for mathematical operations.
"""

from typing import Any, Dict, List, Literal, Union, TypedDict
import logging

from ..core.node import Node

logger = logging.getLogger(__name__)


# Type definitions for CalculatorNode
CalculatorOperation = Literal["add", "multiply", "subtract", "divide", "power", "modulo"]


class CalculatorInput(TypedDict):
    """Input data structure for CalculatorNode."""
    operation: CalculatorOperation
    args: List[Union[int, float]]


class CalculatorOutput(TypedDict):
    """Output data structure for CalculatorNode."""
    operation: CalculatorOperation
    args: List[Union[int, float]]
    result: Union[int, float]
    processed_by: str
    node_config: Dict[str, Any]


class CalculatorError(TypedDict):
    """Error output structure for CalculatorNode."""
    error: str
    operation: Union[CalculatorOperation, str, None]
    args: Union[List[Union[int, float]], Any, None]
    processed_by: str


class CalculatorNode(Node):
    """
    Calculator node - performs mathematical operations.
    
    Expects input data in the format:
    {
        "operation": "add|multiply|subtract|divide|power|modulo",
        "args": [number1, number2, ...]
    }
    """
    
    def process(self, data: Union[CalculatorInput, Any]) -> Union[CalculatorOutput, CalculatorError]:
        """
        Perform mathematical operations on input data.
        
        Args:
            data: Dictionary with operation and args, should match CalculatorInput structure
            
        Returns:
            Dictionary with operation result (CalculatorOutput) or error (CalculatorError)
        """
        logger.info(f"CalculatorNode '{self.name}': processing {data}")
        
        if not isinstance(data, dict):
            return {
                "error": "Input must be a dictionary",
                "input": data,
                "processed_by": f"CalculatorNode[{self.name}]"
            }
        
        if 'operation' not in data or 'args' not in data:
            return {
                "error": "Input must contain 'operation' and 'args' keys",
                "input": data,
                "processed_by": f"CalculatorNode[{self.name}]"
            }
        
        operation = data['operation']
        args = data['args']
        
        try:
            result = self._perform_operation(operation, args)
            
            return {
                'operation': operation,
                'args': args,
                'result': result,
                'processed_by': f'CalculatorNode[{self.name}]',
                'node_config': self.config
            }
            
        except Exception as e:
            logger.error(f"CalculatorNode '{self.name}': operation failed: {e}")
            return {
                "error": str(e),
                "operation": operation,
                "args": args,
                "processed_by": f"CalculatorNode[{self.name}]"
            }
    
    def _perform_operation(self, operation: CalculatorOperation, args: List[Union[int, float]]) -> Union[int, float]:
        """
        Perform the specified mathematical operation.
        
        Args:
            operation: Operation name (one of the supported CalculatorOperation values)
            args: List of numeric arguments (int or float)
            
        Returns:
            Numeric result of the operation (int or float)
            
        Raises:
            ValueError: If operation is unknown or args are invalid
        """
        if len(args) < 2:
            raise ValueError(f"Operation '{operation}' requires at least 2 arguments")
        
        a, b = args[0], args[1]
        
        if operation == 'add':
            return a + b
        elif operation == 'multiply':
            return a * b
        elif operation == 'subtract':
            return a - b
        elif operation == 'divide':
            if b == 0:
                raise ValueError("Division by zero")
            return a / b
        elif operation == 'power':
            return a ** b
        elif operation == 'modulo':
            if b == 0:
                raise ValueError("Modulo by zero")
            return a % b
        else:
            raise ValueError(f"Unknown operation: {operation}")
    
    def get_supported_operations(self) -> List[CalculatorOperation]:
        """Get list of supported operations."""
        return ['add', 'multiply', 'subtract', 'divide', 'power', 'modulo']


__all__ = ["CalculatorNode"] 