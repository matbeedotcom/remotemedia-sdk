"""
Data transformation nodes for the RemoteMedia SDK.
"""

from typing import Any, Callable, Union, TypedDict, Tuple
import logging

from ..core.node import Node

logger = logging.getLogger(__name__)


# Type definitions for DataTransform
DataTransformInput = Any
DataTransformOutput = Any


class DataTransformError(TypedDict):
    """Error output structure for DataTransform."""
    error: str
    input: Any
    processed_by: str


# Type definitions for FormatConverter
FormatConverterInput = Any
FormatConverterOutput = Any


class FormatConverterError(TypedDict):
    """Error output structure for FormatConverter."""
    error: str
    input: Any
    processed_by: str


# Type definitions for TextTransformNode
TextTransformInput = Union[str, Tuple[str, ...]]
TextTransformOutput = Union[str, Tuple[str, ...]]


class TextTransformError(TypedDict):
    """Error output structure for TextTransformNode."""
    error: str
    input: Any
    processed_by: str


class DataTransform(Node):
    """Generic data transformation node."""
    
    def process(self, data: Any) -> Union[Any, DataTransformError]:
        """Transform data."""
        try:
            # TODO: Implement data transformation
            logger.debug(f"DataTransform '{self.name}': transforming data")
            return data
        except Exception as e:
            logger.error(f"DataTransform '{self.name}': transformation failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"DataTransform[{self.name}]"
            }


class FormatConverter(Node):
    """Format conversion node."""
    
    def __init__(self, target_format: str = "json", **kwargs):
        super().__init__(**kwargs)
        self.target_format = target_format
    
    def process(self, data: Any) -> Union[Any, FormatConverterError]:
        """Convert data format."""
        try:
            # TODO: Implement format conversion
            logger.debug(f"FormatConverter '{self.name}': converting to {self.target_format}")
            return data
        except Exception as e:
            logger.error(f"FormatConverter '{self.name}': conversion failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"FormatConverter[{self.name}]"
            }


class TextTransformNode(Node):
    """Node for transforming text using a custom function."""
    
    def __init__(self, transform_func: Callable[[str], str], **kwargs):
        """
        Initialize the text transform node.
        
        Args:
            transform_func: Function that takes a string and returns a transformed string
        """
        super().__init__(**kwargs)
        self.transform_func = transform_func
    
    def process(self, data: Union[str, Tuple[str, ...], Any]) -> Union[str, Tuple[str, ...], Any, TextTransformError]:
        """Transform text data using the provided function."""
        try:
            if isinstance(data, str):
                result = self.transform_func(data)
                logger.debug(f"TextTransformNode '{self.name}': transformed '{data}' -> '{result}'")
                return result
            elif isinstance(data, tuple) and len(data) > 0 and isinstance(data[0], str):
                # Handle (text, metadata) tuples
                transformed_text = self.transform_func(data[0])
                result = (transformed_text,) + data[1:]
                logger.debug(f"TextTransformNode '{self.name}': transformed tuple text")
                return result
            else:
                # Pass through non-text data unchanged
                logger.debug(f"TextTransformNode '{self.name}': passing through non-text data: {type(data)}")
                return data
        except Exception as e:
            logger.error(f"TextTransformNode '{self.name}': transformation failed: {e}")
            return {
                "error": str(e),
                "input": data,
                "processed_by": f"TextTransformNode[{self.name}]"
            }


__all__ = ["DataTransform", "FormatConverter", "TextTransformNode"] 