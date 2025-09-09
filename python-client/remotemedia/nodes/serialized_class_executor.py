"""
Serialized Class Executor node for executing cloudpickle-serialized Python classes.

This node handles the execution of user-defined Python classes that have been
serialized using cloudpickle, as specified in the development strategy document.
"""

from typing import Any, Dict, Union, TypedDict, List, Optional, Tuple
import logging
import base64
import cloudpickle

from ..core.node import Node

logger = logging.getLogger(__name__)


# Type definitions for SerializedClassExecutorNode
class SerializedClassExecutorInput(TypedDict):
    """Input data structure for SerializedClassExecutorNode."""
    serialized_object: str
    method_name: str
    method_args: Optional[List[Any]]
    method_kwargs: Optional[Dict[str, Any]]


class SerializedClassExecutorOutput(TypedDict):
    """Output data structure for SerializedClassExecutorNode."""
    result: Any
    updated_serialized_object: str
    processed_by: str


class SerializedClassExecutorError(TypedDict):
    """Error output structure for SerializedClassExecutorNode."""
    error: str
    error_type: str
    method_name: Optional[str]
    processed_by: str


class SerializedClassExecutorNode(Node):
    """
    Serialized Class Executor node - executes cloudpickle-serialized Python classes.
    
    This node implements the Phase 3 requirement for executing user-defined Python
    classes with local dependencies using cloudpickle serialization.
    
    Expects input data in the format:
    {
        "serialized_object": "base64_encoded_cloudpickle_data",
        "method_name": "method_to_call",
        "method_args": [args],
        "method_kwargs": {kwargs}
    }
    """
    
    def process(self, data: Union[SerializedClassExecutorInput, Any]) -> Union[SerializedClassExecutorOutput, SerializedClassExecutorError]:
        """
        Execute a method on a cloudpickle-serialized Python object.
        
        Args:
            data: Dictionary with serialized object and method call info
            
        Returns:
            Dictionary with execution result or error
        """
        logger.info(f"SerializedClassExecutorNode '{self.name}': Executing serialized class method")
        
        if not isinstance(data, dict):
            return {
                "error": "Input must be a dictionary",
                "input": data,
                "processed_by": f"SerializedClassExecutorNode[{self.name}]"
            }
        
        required_keys = ['serialized_object', 'method_name']
        for key in required_keys:
            if key not in data:
                return {
                    "error": f"Input must contain '{key}' key",
                    "input": data,
                    "processed_by": f"SerializedClassExecutorNode[{self.name}]"
                }
        
        serialized_object = data['serialized_object']
        method_name = data['method_name']
        method_args = data.get('method_args', [])
        method_kwargs = data.get('method_kwargs', {})
        
        try:
            # Import cloudpickle dynamically
            try:
                import cloudpickle
            except ImportError:
                raise ImportError("cloudpickle is required for serialized class execution")

            result, updated_obj = self._execute_serialized_method(
                serialized_object, method_name, method_args, method_kwargs, cloudpickle
            )
            
            # Re-serialize the updated object to return its new state
            updated_serialized_obj = base64.b64encode(cloudpickle.dumps(updated_obj)).decode('ascii')

            return {
                'result': result,
                'updated_serialized_object': updated_serialized_obj,
                'processed_by': f'SerializedClassExecutorNode[{self.name}]',
            }
            
        except (ValueError, AttributeError, TypeError) as e:
            # These are expected errors (bad input, missing method, etc.)
            logger.info(f"SerializedClassExecutorNode '{self.name}': {type(e).__name__}: {e}")
            return {
                'error': str(e),
                'error_type': type(e).__name__,
                'method_name': method_name,
                'processed_by': f'SerializedClassExecutorNode[{self.name}]'
            }
        except RuntimeError as e:
            # This includes method execution failures (like division by zero)
            logger.info(f"SerializedClassExecutorNode '{self.name}': Method execution error: {e}")
            return {
                'error': str(e),
                'error_type': 'RuntimeError',
                'method_name': method_name,
                'processed_by': f'SerializedClassExecutorNode[{self.name}]'
            }
        except Exception as e:
            # Unexpected errors should be logged as warnings
            logger.warning(f"SerializedClassExecutorNode '{self.name}': Unexpected error: {e}")
            return {
                'error': f"Unexpected error: {str(e)}",
                'error_type': type(e).__name__,
                'method_name': method_name,
                'processed_by': f'SerializedClassExecutorNode[{self.name}]'
            }
    
    def _execute_serialized_method(self, serialized_object: str, method_name: str, 
                                 method_args: list, method_kwargs: dict, pickle_lib: Any) -> Tuple[Any, Any]:
        """
        Deserialize object, execute method, and return result and modified object.
        
        Args:
            serialized_object: Base64-encoded cloudpickle data
            method_name: Name of method to call
            method_args: Arguments for method
            method_kwargs: Keyword arguments for method
            pickle_lib: The cloudpickle library instance
            
        Returns:
            A tuple of (result_of_method_execution, modified_object)
            
        Raises:
            Exception: Any exception from deserialization or method execution
        """
        # Decode and deserialize the object
        try:
            serialized_data = base64.b64decode(serialized_object.encode('ascii'))
            obj = pickle_lib.loads(serialized_data)
        except Exception as e:
            raise ValueError(f"Failed to deserialize object: {e}")
        
        # Check if method exists
        if not hasattr(obj, method_name):
            raise AttributeError(f"Object does not have method '{method_name}'")
        
        # Get the method
        method = getattr(obj, method_name)
        
        # Check if it's callable
        if not callable(method):
            raise TypeError(f"'{method_name}' is not callable")
        
        # Execute the method
        try:
            result = method(*method_args, **method_kwargs)
            logger.debug(f"Method '{method_name}' executed successfully")
            return result, obj
        except Exception as e:
            # Log the specific exception type for debugging
            logger.debug(f"Method '{method_name}' raised {type(e).__name__}: {e}")
            raise RuntimeError(f"Method execution failed: {e}")
    
    def get_security_info(self) -> Dict[str, Any]:
        """Get information about security settings."""
        return {
            "security_level": "MODERATE - Cloudpickle deserialization",
            "serialization_method": "cloudpickle",
            "warning": "This node deserializes and executes user objects - use with caution!"
        }


__all__ = ["SerializedClassExecutorNode"] 