"""
Task Executor for the Remote Execution Service.

This module handles the execution of SDK nodes and user-defined tasks
in a secure, controlled environment.
"""

import asyncio
import inspect
import logging
import time
from typing import Dict, Any, List, Optional
from dataclasses import dataclass

# Import SDK components
try:
    from remotemedia.nodes import (
        PassThroughNode, BufferNode,
        AudioTransform, AudioBuffer, AudioResampler,
        VideoTransform, VideoBuffer, VideoResizer,
        DataTransform, FormatConverter,
        CalculatorNode, CodeExecutorNode, TextProcessorNode,
        SerializedClassExecutorNode
    )
    from remotemedia.nodes.ml import TransformersPipelineNode
    from remotemedia.serialization import JSONSerializer, PickleSerializer
    from remotemedia.core.node import Node
    SDK_AVAILABLE = True
except ImportError:
    # Fallback for development/testing
    logging.warning("RemoteMedia SDK not available, using mock implementations")
    SDK_AVAILABLE = False
    Node = object
    PassThroughNode = BufferNode = None
    AudioTransform = AudioBuffer = AudioResampler = None
    VideoTransform = VideoBuffer = VideoResizer = None
    DataTransform = FormatConverter = None
    CalculatorNode = CodeExecutorNode = TextProcessorNode = None
    SerializedClassExecutorNode = None
    TransformersPipelineNode = None
    JSONSerializer = PickleSerializer = None

from config import ServiceConfig


@dataclass
class ExecutionResult:
    """Result of task execution."""
    output_data: bytes
    input_size: int
    output_size: int
    memory_peak: int
    cpu_time: int
    installed_dependencies: List[str] = None


class TaskExecutor:
    """
    Handles execution of SDK nodes and custom tasks.
    """
    
    def __init__(self, config: ServiceConfig, custom_node_registry: Dict[str, type] = None):
        """
        Initialize the task executor.
        
        Args:
            config: Service configuration
            custom_node_registry: Optional dictionary of custom nodes to register
        """
        self.config = config
        self.logger = logging.getLogger(__name__)
        
        # Initialize serializers
        self.serializers = {
            'json': JSONSerializer() if JSONSerializer else self._get_json_serializer(),
            'pickle': PickleSerializer() if PickleSerializer else self._get_pickle_serializer(),
        }
        
        # Build node registry from SDK
        self.node_registry = self._build_sdk_node_registry()
        
        # Register custom nodes if provided
        if custom_node_registry:
            self._register_custom_nodes(custom_node_registry)
        
        self.logger.info(f"TaskExecutor initialized with {len(self.node_registry)} node types")
    
    def _build_sdk_node_registry(self) -> Dict[str, type]:
        """Build registry of available SDK nodes."""
        registry = {}
        
        if SDK_AVAILABLE:
            # Base nodes
            if PassThroughNode:
                registry['PassThroughNode'] = PassThroughNode
            if BufferNode:
                registry['BufferNode'] = BufferNode
            
            # Audio nodes
            if AudioTransform:
                registry['AudioTransform'] = AudioTransform
            if AudioBuffer:
                registry['AudioBuffer'] = AudioBuffer
            if AudioResampler:
                registry['AudioResampler'] = AudioResampler
            
            # Video nodes
            if VideoTransform:
                registry['VideoTransform'] = VideoTransform
            if VideoBuffer:
                registry['VideoBuffer'] = VideoBuffer
            if VideoResizer:
                registry['VideoResizer'] = VideoResizer
            
            # Transform nodes
            if DataTransform:
                registry['DataTransform'] = DataTransform
            if FormatConverter:
                registry['FormatConverter'] = FormatConverter
            
            # Utility nodes
            if CalculatorNode:
                registry['CalculatorNode'] = CalculatorNode
            if CodeExecutorNode:
                registry['CodeExecutorNode'] = CodeExecutorNode
            if TextProcessorNode:
                registry['TextProcessorNode'] = TextProcessorNode
            if SerializedClassExecutorNode:
                registry['SerializedClassExecutorNode'] = SerializedClassExecutorNode
            
            # ML nodes
            if TransformersPipelineNode:
                registry['TransformersPipelineNode'] = TransformersPipelineNode
            
            self.logger.info(f"Registered {len(registry)} SDK nodes")
        else:
            self.logger.warning("SDK not available, no nodes registered")
        
        return registry
    
    def _register_custom_nodes(self, custom_node_registry: Dict[str, type]) -> None:
        """
        Register custom nodes in the node registry.
        
        Args:
            custom_node_registry: Dictionary mapping node names to node classes
        """
        for node_name, node_class in custom_node_registry.items():
            # Validate that it's a proper node class
            if not issubclass(node_class, Node):
                self.logger.warning(f"Skipping {node_name}: not a valid Node subclass")
                continue
                
            # Check for name conflicts
            if node_name in self.node_registry:
                self.logger.warning(f"Custom node {node_name} will override SDK node")
                
            self.node_registry[node_name] = node_class
            self.logger.info(f"Registered custom node: {node_name}")
    
    def _get_json_serializer(self):
        """Get primitive JSON serializer."""
        import json
        
        class PrimitiveJSONSerializer:
            def serialize(self, data):
                return json.dumps(data).encode('utf-8')
            
            def deserialize(self, data):
                return json.loads(data.decode('utf-8'))
        
        return PrimitiveJSONSerializer()
    
    def _get_pickle_serializer(self):
        """Get primitive Pickle serializer."""
        import pickle
        
        class PrimitivePickleSerializer:
            def serialize(self, data):
                return pickle.dumps(data)
            
            def deserialize(self, data):
                return pickle.loads(data)
        
        return PrimitivePickleSerializer()
    
    async def execute_sdk_node(
        self,
        node_type: str,
        config: Dict[str, Any],
        input_data: bytes,
        serialization_format: str,
        options: Any
    ) -> ExecutionResult:
        """
        Execute a predefined SDK node.
        
        Args:
            node_type: Type of SDK node to execute
            config: Node configuration parameters
            input_data: Serialized input data
            serialization_format: Format used for serialization
            options: Execution options
            
        Returns:
            Execution result
            
        Raises:
            ValueError: If node type is not available
            RuntimeError: If execution fails
        """
        start_time = time.time()
        
        self.logger.info(f"Executing SDK node: {node_type}")
        
        # Check if node type is available
        if node_type not in self.node_registry:
            available = list(self.node_registry.keys())
            raise ValueError(f"Unknown node type: {node_type}. Available: {available}")
        
        # Get serializer
        serializer = self.serializers.get(serialization_format)
        if not serializer:
            raise ValueError(f"Unknown serialization format: {serialization_format}")
        
        node = None
        try:
            # Deserialize input data
            input_obj = serializer.deserialize(input_data)
            input_size = len(input_data)
            
            # Create and configure node using SDK
            node_class = self.node_registry[node_type]
            
            # Parse config values - handle JSON strings and type conversions
            import json
            import ast
            parsed_config = {}
            for key, value in config.items():
                if isinstance(value, str):
                    # Try to parse as JSON first (for dicts/lists)
                    try:
                        parsed_value = json.loads(value)
                        parsed_config[key] = parsed_value
                    except json.JSONDecodeError:
                        # Try to parse as Python literal (numbers, booleans)
                        try:
                            parsed_value = ast.literal_eval(value)
                            parsed_config[key] = parsed_value
                        except (ValueError, SyntaxError):
                            # Keep as string if all parsing fails
                            parsed_config[key] = value
                else:
                    parsed_config[key] = value
            
            # Create node 
            node = node_class(name=f"remote_{node_type}", **parsed_config)
            
            # Initialize node (if method exists)
            if hasattr(node, 'initialize'):
                try:
                    await node.initialize()
                except Exception as init_error:
                    self.logger.error(f"Failed to initialize {node_type}: {init_error}")
                    # Try cleanup before re-raising
                    if hasattr(node, 'cleanup'):
                        try:
                            await node.cleanup()
                        except:
                            pass
                    raise RuntimeError(f"Node initialization failed: {init_error}") from init_error
            
            # Execute node
            import asyncio
            try:
                if asyncio.iscoroutinefunction(node.process):
                    output_obj = await node.process(input_obj)
                else:
                    output_obj = node.process(input_obj)
            except Exception as process_error:
                self.logger.error(f"Failed to process data with {node_type}: {process_error}")
                raise RuntimeError(f"Node processing failed: {process_error}") from process_error
            
            # Serialize output
            try:
                output_data = serializer.serialize(output_obj)
                output_size = len(output_data)
            except Exception as serialize_error:
                self.logger.error(f"Failed to serialize output from {node_type}: {serialize_error}")
                raise RuntimeError(f"Output serialization failed: {serialize_error}") from serialize_error
            
            # Clean up node (if method exists)
            if hasattr(node, 'cleanup'):
                try:
                    await node.cleanup()
                except Exception as cleanup_error:
                    self.logger.warning(f"Cleanup failed for {node_type}: {cleanup_error}")
                    # Don't fail the entire operation for cleanup issues
            
            execution_time = int((time.time() - start_time) * 1000)
            
            self.logger.info(f"Successfully executed {node_type} in {execution_time}ms")
            
            return ExecutionResult(
                output_data=output_data,
                input_size=input_size,
                output_size=output_size,
                memory_peak=0,  # TODO: Implement memory tracking
                cpu_time=execution_time
            )
            
        except Exception as e:
            # Enhanced error logging with more context
            self.logger.error(f"Error executing SDK node {node_type}: {e}", exc_info=True)
            
            # Try emergency cleanup if node was created
            if node is not None and hasattr(node, 'cleanup'):
                try:
                    await node.cleanup()
                    self.logger.info(f"Emergency cleanup completed for {node_type}")
                except Exception as cleanup_error:
                    self.logger.warning(f"Emergency cleanup failed for {node_type}: {cleanup_error}")
            
            # Clear CUDA cache on error if available
            try:
                import torch
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
                    self.logger.info("Cleared CUDA cache after error")
            except ImportError:
                pass
            
            # Re-raise with the original error but preserve the chain
            if isinstance(e, RuntimeError) and "failed" in str(e).lower():
                # Already a well-formatted error from above
                raise e
            else:
                # Generic error, wrap it
                raise RuntimeError(f"Node execution failed: {e}") from e
    
    async def execute_custom_task(
        self,
        code_package: bytes,
        entry_point: str,
        input_data: bytes,
        serialization_format: str,
        dependencies: List[str],
        options: Any
    ) -> ExecutionResult:
        """
        Execute user-defined code (Phase 3 feature).
        
        Args:
            code_package: Packaged user code
            entry_point: Entry point function/method
            input_data: Serialized input data
            serialization_format: Serialization format
            dependencies: Required Python packages
            options: Execution options
            
        Returns:
            Execution result
            
        Raises:
            NotImplementedError: This feature is not yet implemented
        """
        self.logger.warning("Custom task execution not yet implemented (Phase 3)")
        raise NotImplementedError("Custom task execution will be implemented in Phase 3")
    
    async def get_available_nodes(self, category: Optional[str] = None) -> List[Any]:
        """
        Get list of available SDK nodes.
        
        Args:
            category: Optional category filter
            
        Returns:
            List of available nodes
        """
        nodes = []
        
        for node_type, node_class in self.node_registry.items():
            # Determine category based on node type
            if 'Audio' in node_type:
                node_category = 'audio'
            elif 'Video' in node_type:
                node_category = 'video'
            elif 'Transform' in node_type or 'Converter' in node_type:
                node_category = 'transform'
            elif 'Calculator' in node_type:
                node_category = 'math'
            elif 'Code' in node_type or 'Serialized' in node_type:
                node_category = 'execution'
            elif 'Text' in node_type:
                node_category = 'text'
            else:
                node_category = 'base'
            
            # Apply category filter
            if category and node_category != category:
                continue
            
            # Extract parameters using introspection
            parameters = self._extract_node_parameters(node_class)
            
            # Extract method signatures
            methods = self._extract_node_methods(node_class)
            
            # Extract TypedDict classes for this node
            typeddict_classes = self._extract_node_typeddict_classes(node_class)
            
            # Create node info
            node_info = {
                'node_type': node_type,
                'category': node_category,
                'description': getattr(node_class, '__doc__', f"{node_type} processing node") or f"{node_type} processing node",
                'parameters': parameters,
                'methods': methods,
                'types': typeddict_classes  # Include TypedDict classes
            }
            nodes.append(node_info)
        
        return nodes
    
    def _extract_node_parameters(self, node_class: type) -> List[Dict[str, Any]]:
        """
        Extract parameter information from a node class using introspection.
        
        Args:
            node_class: Node class to inspect
            
        Returns:
            List of parameter information
        """
        import inspect
        import typing
        from typing import get_type_hints, get_origin, get_args
        
        parameters = []
        
        try:
            # Get all __init__ methods in the MRO (Method Resolution Order)
            # This ensures we capture parameters from parent classes too
            all_parameters = {}
            all_descriptions = {}
            
            for cls in node_class.__mro__:
                if cls == object:  # Skip object class
                    continue
                    
                init_method = getattr(cls, '__init__', None)
                if not init_method:
                    continue
                
                # Get signature and type hints for this class
                try:
                    sig = inspect.signature(init_method)
                    
                    # Try to get type hints
                    try:
                        type_hints = get_type_hints(init_method)
                    except Exception as e:
                        self.logger.debug(f"Could not get type hints for {cls.__name__}: {e}")
                        type_hints = {}
                    
                    # Parse docstring for parameter descriptions
                    docstring = inspect.getdoc(init_method) or ""
                    param_descriptions = self._parse_docstring_params(docstring)
                    all_descriptions.update(param_descriptions)
                    
                    # Process parameters for this class
                    for param_name, param in sig.parameters.items():
                        # Skip 'self' and any **kwargs style parameters
                        if param_name == 'self' or param.kind == inspect.Parameter.VAR_KEYWORD:
                            continue
                        
                        # Only add if we haven't seen this parameter yet (subclass takes precedence)
                        if param_name not in all_parameters:
                            # Determine parameter type
                            param_type = type_hints.get(param_name, param.annotation)
                            type_str = self._get_type_string(param_type)
                            
                            # Determine if required (no default value)
                            required = param.default == inspect.Parameter.empty
                            default_value = None if required else param.default
                            
                            # Get description from docstring
                            description = param_descriptions.get(param_name, "")
                            
                            # Handle special types
                            allowed_values = None
                            if hasattr(param_type, '__args__') and hasattr(param_type, '__origin__'):
                                # Handle Union types (like Optional)
                                if get_origin(param_type) is typing.Union:
                                    args = get_args(param_type)
                                    # Check if it's Optional (Union with None)
                                    if len(args) == 2 and type(None) in args:
                                        type_str = self._get_type_string(args[0] if args[1] is type(None) else args[1])
                                        if required and default_value is None:
                                            required = False
                                            default_value = None
                            
                            param_info = {
                                'name': param_name,
                                'type': type_str,
                                'required': required,
                                'description': description,
                                'source_class': cls.__name__
                            }
                            
                            if default_value is not None:
                                param_info['default_value'] = default_value
                                
                            if allowed_values:
                                param_info['allowed_values'] = allowed_values
                                
                            all_parameters[param_name] = param_info
                            
                except Exception as e:
                    self.logger.debug(f"Failed to process {cls.__name__}.__init__: {e}")
                    continue
            
            # Convert to list and sort (put node-specific parameters first)
            for param_info in all_parameters.values():
                parameters.append(param_info)
                self.logger.debug(f"Added parameter {param_info['name']} from {param_info['source_class']} for {node_class.__name__}: {param_info}")
            
            # Sort parameters: node-specific first, then base class parameters
            def sort_key(param):
                if param['source_class'] == node_class.__name__:
                    return (0, param['name'])  # Node-specific parameters first
                else:
                    return (1, param['name'])  # Base class parameters second
            
            parameters.sort(key=sort_key)
                
        except Exception as e:
            self.logger.warning(f"Failed to extract parameters for {node_class.__name__}: {e}")
            import traceback
            self.logger.debug(f"Full traceback: {traceback.format_exc()}")
            
        return parameters
    
    def _extract_node_typeddict_classes(self, node_class: type) -> List[Dict[str, Any]]:
        """
        Extract TypedDict classes from a node module for TypeScript generation.
        
        Args:
            node_class: Node class to inspect
            
        Returns:
            List of TypedDict class information
        """
        import inspect
        from typing import get_type_hints, Union
        
        typeddict_classes = []
        
        try:
            # Get the module where the node class is defined
            node_module = inspect.getmodule(node_class)
            if not node_module:
                return typeddict_classes
            
            # Look for TypedDict classes in the module
            for name, obj in inspect.getmembers(node_module):
                # Check if it's a TypedDict class
                if (hasattr(obj, '__annotations__') and 
                    hasattr(obj, '__total__') and
                    hasattr(obj, '__doc__') and
                    name not in ['TypedDict']):  # Exclude the base TypedDict import
                    
                    try:
                        # Get type hints for the TypedDict
                        annotations = getattr(obj, '__annotations__', {})
                        
                        # Extract field information
                        fields = []
                        for field_name, field_type in annotations.items():
                            field_info = {
                                'name': field_name,
                                'type': self._get_type_string(field_type),
                                'required': True  # TypedDict fields are required by default unless Optional
                            }
                            
                            # Check if it's optional (Union with None)
                            if hasattr(field_type, '__origin__') and hasattr(field_type, '__args__'):
                                from typing import get_origin, get_args
                                if get_origin(field_type) is Union:
                                    args = get_args(field_type)
                                    if len(args) == 2 and type(None) in args:
                                        field_info['required'] = False
                                        # Get the non-None type
                                        non_none_type = args[0] if args[1] is type(None) else args[1]
                                        field_info['type'] = self._get_type_string(non_none_type)
                            
                            fields.append(field_info)
                        
                        typeddict_info = {
                            'name': name,
                            'description': getattr(obj, '__doc__', '').strip() or f"{name} interface",
                            'fields': fields,
                            'total': getattr(obj, '__total__', True)  # Whether all fields are required
                        }
                        
                        typeddict_classes.append(typeddict_info)
                        self.logger.debug(f"Extracted TypedDict {name} with {len(fields)} fields for {node_class.__name__}")
                        
                    except Exception as e:
                        self.logger.debug(f"Failed to process TypedDict {name}: {e}")
                        continue
                        
        except Exception as e:
            self.logger.warning(f"Failed to extract TypedDict classes for {node_class.__name__}: {e}")
        
        return typeddict_classes

    def _extract_node_methods(self, node_class: type) -> List[Dict[str, Any]]:
        """
        Extract method signatures from a node class.
        
        Args:
            node_class: Node class to inspect
            
        Returns:
            List of method information
        """
        import inspect
        from typing import get_type_hints
        
        methods = []
        
        try:
            # Get all public methods that are not inherited from basic object class
            for method_name in dir(node_class):
                # Skip private methods, magic methods (except process), and basic object methods
                if (method_name.startswith('_') and method_name not in ['__init__']) or \
                   method_name in ['__class__', '__doc__', '__module__', '__dict__', '__weakref__']:
                    continue
                
                method = getattr(node_class, method_name)
                
                # Only process callable methods that are defined in this class or its parents (not object)
                if not callable(method):
                    continue
                
                # Get the method from the class that actually defines it
                defining_class = None
                for cls in node_class.__mro__:
                    if cls == object:
                        continue
                    if hasattr(cls, method_name) and method_name in cls.__dict__:
                        defining_class = cls
                        break
                
                if not defining_class:
                    continue
                
                try:
                    # Get method signature
                    sig = inspect.signature(method)
                    
                    # Get type hints
                    try:
                        type_hints = get_type_hints(method)
                    except Exception as e:
                        self.logger.debug(f"Could not get type hints for {method_name}: {e}")
                        type_hints = {}
                    
                    # Parse method docstring
                    method_doc = inspect.getdoc(method) or ""
                    
                    # Extract parameters (excluding 'self')
                    method_params = []
                    for param_name, param in sig.parameters.items():
                        if param_name == 'self':
                            continue
                        
                        param_type = type_hints.get(param_name, param.annotation)
                        type_str = self._get_type_string(param_type)
                        
                        required = param.default == inspect.Parameter.empty
                        default_value = None if required else param.default
                        
                        param_info = {
                            'name': param_name,
                            'type': type_str,
                            'required': required
                        }
                        
                        if default_value is not None:
                            param_info['default_value'] = default_value
                        
                        method_params.append(param_info)
                    
                    # Get return type
                    return_type = type_hints.get('return', sig.return_annotation)
                    return_type_str = self._get_type_string(return_type)
                    
                    method_info = {
                        'name': method_name,
                        'description': method_doc,
                        'parameters': method_params,
                        'return_type': return_type_str,
                        'defining_class': defining_class.__name__
                    }
                    
                    methods.append(method_info)
                    
                except Exception as e:
                    self.logger.debug(f"Failed to process method {method_name}: {e}")
                    continue
        
        except Exception as e:
            self.logger.warning(f"Failed to extract methods for {node_class.__name__}: {e}")
        
        return methods
    
    def _parse_docstring_params(self, docstring: str) -> Dict[str, str]:
        """
        Parse parameter descriptions from docstring.
        
        Args:
            docstring: The docstring to parse
            
        Returns:
            Dictionary mapping parameter names to descriptions
        """
        param_descriptions = {}
        
        # Look for Args: section
        lines = docstring.split('\n')
        in_args_section = False
        current_param = None
        current_desc = []
        
        for line in lines:
            line = line.strip()
            
            if line.startswith('Args:'):
                in_args_section = True
                continue
            elif line.startswith(('Returns:', 'Yields:', 'Raises:', 'Note:', 'Example:')):
                in_args_section = False
                if current_param:
                    param_descriptions[current_param] = ' '.join(current_desc).strip()
                break
                
            if in_args_section and line:
                # Check if this is a parameter definition
                if ':' in line and not line.startswith(' '):
                    # Save previous parameter
                    if current_param:
                        param_descriptions[current_param] = ' '.join(current_desc).strip()
                    
                    # Start new parameter
                    parts = line.split(':', 1)
                    param_part = parts[0].strip()
                    # Extract parameter name (remove type hints)
                    if '(' in param_part:
                        current_param = param_part.split('(')[0].strip()
                    else:
                        current_param = param_part
                    
                    current_desc = [parts[1].strip()] if len(parts) > 1 else []
                elif current_param and line.startswith(' '):
                    # Continuation of parameter description
                    current_desc.append(line.strip())
        
        # Save last parameter
        if current_param:
            param_descriptions[current_param] = ' '.join(current_desc).strip()
            
        return param_descriptions
    
    def _get_type_string(self, type_annotation) -> str:
        """
        Convert Python type annotation to string representation.
        
        Args:
            type_annotation: The type annotation
            
        Returns:
            String representation of the type
        """
        import typing
        from typing import get_origin, get_args
        
        if type_annotation == inspect.Parameter.empty:
            return 'any'
            
        # Handle basic types
        if type_annotation in (int, float, str, bool):
            return type_annotation.__name__
        
        # Handle None type
        if type_annotation is type(None):
            return 'null'
            
        # Handle Any type
        if type_annotation is typing.Any:
            return 'any'
            
        # Handle Union types (including Optional)
        origin = get_origin(type_annotation)
        if origin is typing.Union:
            args = get_args(type_annotation)
            # Check if it's Optional (Union with None)
            if len(args) == 2 and type(None) in args:
                non_none_type = args[0] if args[1] is type(None) else args[1]
                return f"{self._get_type_string(non_none_type)} | null"
            else:
                # Regular Union
                union_types = [self._get_type_string(arg) for arg in args]
                return " | ".join(union_types)
        
        # Handle Literal types
        if hasattr(typing, 'Literal') and origin is typing.Literal:
            args = get_args(type_annotation)
            literal_values = [f'"{arg}"' if isinstance(arg, str) else str(arg) for arg in args]
            return " | ".join(literal_values)
        
        # Handle List types
        if origin is list:
            args = get_args(type_annotation)
            if args:
                item_type = self._get_type_string(args[0])
                return f"Array<{item_type}>"
            return "Array<any>"
        
        # Handle Dict types
        if origin is dict:
            args = get_args(type_annotation)
            if len(args) >= 2:
                key_type = self._get_type_string(args[0])
                value_type = self._get_type_string(args[1])
                return f"Record<{key_type}, {value_type}>"
            return "Record<string, any>"
        
        # Handle Tuple types
        if origin is tuple:
            args = get_args(type_annotation)
            if args:
                tuple_types = [self._get_type_string(arg) for arg in args]
                return f"[{', '.join(tuple_types)}]"
            return "any[]"
        
        # Handle TypedDict and other classes
        if hasattr(type_annotation, '__name__'):
            # Check if it's a TypedDict
            if hasattr(type_annotation, '__annotations__') and hasattr(type_annotation, '__total__'):
                # It's a TypedDict - we'll represent it as a structured object
                return f"TypedDict<{type_annotation.__name__}>"
            return type_annotation.__name__
        elif hasattr(type_annotation, '__origin__'):
            # Handle other generic types
            origin = type_annotation.__origin__
            return str(origin.__name__ if hasattr(origin, '__name__') else origin)
        else:
            return str(type_annotation) 