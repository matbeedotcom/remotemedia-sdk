"""
Code Executor node for executing Python code.

WARNING: This node executes arbitrary Python code and is INSECURE!
Only use in trusted environments with proper sandboxing.
"""

from typing import Any, Dict, Optional, List, Union, TypedDict
import logging

from ..core.node import Node

logger = logging.getLogger(__name__)


# Type definitions for CodeExecutorNode
class CodeExecutorInput(TypedDict):
    """Input data structure for CodeExecutorNode."""
    code: str
    input: Optional[Any]


class CodeExecutorOutput(TypedDict):
    """Output data structure for CodeExecutorNode."""
    executed_code: str
    input: Any
    result: Any
    processed_by: str
    node_config: Dict[str, Any]


class CodeExecutorError(TypedDict):
    """Error output structure for CodeExecutorNode."""
    error: str
    code: Optional[str]
    input: Optional[Any]
    processed_by: str


class CodeExecutorNode(Node):
    """
    Code Executor node - executes arbitrary Python code.
    
    WARNING: This is INSECURE and should only be used in trusted environments!
    
    Expects input data in the format:
    {
        "code": "python_code_string",
        "input": optional_input_data
    }
    """
    
    def __init__(
        self,
        code: Optional[str] = None,
        input_data: Any = None,
        enable_safe_imports: bool = False,
        enable_pickle: bool = False,
        enable_cloudpickle: bool = False,
        allowed_modules: Optional[List[str]] = None,
        **kwargs
    ):
        """
        Initializes the CodeExecutorNode.
        
        Args:
            code (str, optional): Python code to execute. Can be provided at initialization or runtime.
            input_data (Any, optional): Input data to make available during code execution.
            enable_safe_imports (bool): Whether to enable safe module imports like math, json.
            enable_pickle (bool): Whether to enable pickle module (DANGEROUS).
            enable_cloudpickle (bool): Whether to enable cloudpickle module for advanced serialization.
            allowed_modules (List[str], optional): List of additional safe modules to allow.
            **kwargs: Additional node parameters.
        """
        super().__init__(**kwargs)
        self.default_code = code
        self.default_input_data = input_data
        self.enable_safe_imports = enable_safe_imports
        self.enable_pickle = enable_pickle
        self.enable_cloudpickle = enable_cloudpickle
        self.allowed_modules = allowed_modules or []
    
    def process(self, data: Union[CodeExecutorInput, None, Any]) -> Union[CodeExecutorOutput, CodeExecutorError]:
        """
        Execute Python code from input data.
        
        Args:
            data: Dictionary with code and optional input, or None to use defaults
            
        Returns:
            Dictionary with execution result or error
        """
        logger.warning(f"CodeExecutorNode '{self.name}': Executing user code - THIS IS INSECURE!")
        
        # Handle case where data is None and we use defaults
        if data is None:
            if self.default_code is None:
                return {
                    "error": "No code provided and no default code configured",
                    "processed_by": f"CodeExecutorNode[{self.name}]"
                }
            code = self.default_code
            input_data = self.default_input_data
        elif isinstance(data, dict):
            code = data.get('code', self.default_code)
            input_data = data.get('input', self.default_input_data)
            
            if code is None:
                return {
                    "error": "Input must contain 'code' key or have default code configured",
                    "input": data,
                    "processed_by": f"CodeExecutorNode[{self.name}]"
                }
        else:
            return {
                "error": "Input must be a dictionary or None",
                "input": data,
                "processed_by": f"CodeExecutorNode[{self.name}]"
            }
        
        try:
            result = self._execute_code(code, input_data)
            
            return {
                'executed_code': code,
                'input': input_data,
                'result': result,
                'processed_by': f'CodeExecutorNode[{self.name}]',
                'node_config': self.config
            }
            
        except Exception as e:
            logger.error(f"CodeExecutorNode '{self.name}': execution failed: {e}")
            return {
                'error': str(e),
                'code': code,
                'input': input_data,
                'processed_by': f'CodeExecutorNode[{self.name}]'
            }
    
    def _execute_code(self, code: str, input_data: Any) -> Any:
        """
        Execute Python code in a restricted environment.
        
        Args:
            code: Python code to execute
            input_data: Input data available as 'input_data' variable
            
        Returns:
            Value of 'result' variable after execution
            
        Raises:
            Exception: Any exception from code execution
        """
        # Create a restricted globals environment
        safe_globals = {
            '__builtins__': {
                # Basic types
                'len': len,
                'str': str,
                'int': int,
                'float': float,
                'bool': bool,
                'list': list,
                'dict': dict,
                'tuple': tuple,
                'set': set,
                
                # Basic functions
                'print': print,
                'range': range,
                'enumerate': enumerate,
                'zip': zip,
                'sorted': sorted,
                'reversed': reversed,
                
                # Math functions
                'sum': sum,
                'max': max,
                'min': min,
                'abs': abs,
                'round': round,
                
                # String methods
                'ord': ord,
                'chr': chr,
                
                # Type checking
                'isinstance': isinstance,
                'hasattr': hasattr,
                'getattr': getattr,
                'setattr': setattr,
            }
        }
        
        # Add safe modules if enabled in config
        if self.enable_safe_imports:
            safe_globals.update(self._get_safe_modules())
        
        # Execute the code
        local_vars = {'input_data': input_data}
        exec(code, safe_globals, local_vars)
        
        # Return the result
        return local_vars.get('result', 'No result variable set')
    
    def _get_safe_modules(self) -> Dict[str, Any]:
        """Get safe modules that can be imported."""
        safe_modules = {}
        
        # Math module
        try:
            import math
            safe_modules['math'] = math
        except ImportError:
            pass
        
        # JSON module
        try:
            import json
            safe_modules['json'] = json
        except ImportError:
            pass
        
        # Base64 module (for serialization)
        try:
            import base64
            safe_modules['base64'] = base64
        except ImportError:
            pass
        
        # Pickle module (for serialization) - DANGEROUS but needed for some tests
        if self.enable_pickle:
            try:
                import pickle
                safe_modules['pickle'] = pickle
            except ImportError:
                pass
        
        # Cloudpickle module (for advanced serialization) - needed for Phase 3
        if self.enable_cloudpickle:
            try:
                import cloudpickle
                safe_modules['cloudpickle'] = cloudpickle
            except ImportError:
                pass
        
        # Add any additional allowed modules
        for module_name in self.allowed_modules:
            try:
                module = __import__(module_name)
                safe_modules[module_name] = module
            except ImportError:
                logger.warning(f"Could not import allowed module: {module_name}")
                pass
        
        return safe_modules
    
    def get_security_info(self) -> Dict[str, Any]:
        """Get information about security settings."""
        return {
            "security_level": "MINIMAL - INSECURE",
            "safe_imports_enabled": self.enable_safe_imports,
            "pickle_enabled": self.enable_pickle,
            "cloudpickle_enabled": self.enable_cloudpickle,
            "allowed_modules": self.allowed_modules,
            "warning": "This node executes arbitrary code and is NOT SECURE!"
        }


__all__ = ["CodeExecutorNode"] 