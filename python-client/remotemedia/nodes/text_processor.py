"""
Text Processor node for text manipulation operations.
"""

from typing import Any, Dict, Union, TypedDict, List
import logging

from ..core.node import Node

logger = logging.getLogger(__name__)


# Type definitions for TextProcessorNode
class TextProcessorInput(TypedDict):
    """Input data structure for TextProcessorNode."""
    text: str
    operations: List[str]


class TextProcessorOutput(TypedDict):
    """Output data structure for TextProcessorNode."""
    original_text: str
    operations: List[str]
    results: Dict[str, Any]
    processed_by: str
    node_config: Dict[str, Any]


class TextProcessorError(TypedDict):
    """Error output structure for TextProcessorNode."""
    error: str
    text: str
    operations: List[str]
    processed_by: str


class TextProcessorNode(Node):
    """
    Text Processor node - performs various text processing operations.
    
    Expects input data in the format:
    {
        "text": "string_to_process",
        "operations": ["uppercase", "lowercase", "reverse", "word_count", "char_count"]
    }
    """
    
    def process(self, data: Union[TextProcessorInput, Any]) -> Union[TextProcessorOutput, TextProcessorError]:
        """
        Perform text processing operations on input data.
        
        Args:
            data: Dictionary with text and operations
            
        Returns:
            Dictionary with processing results
        """
        logger.info(f"TextProcessorNode '{self.name}': processing {data}")
        
        if not isinstance(data, dict):
            return {
                "error": "Input must be a dictionary",
                "input": data,
                "processed_by": f"TextProcessorNode[{self.name}]"
            }
        
        if 'text' not in data:
            return {
                "error": "Input must contain 'text' key",
                "input": data,
                "processed_by": f"TextProcessorNode[{self.name}]"
            }
        
        text = data['text']
        operations = data.get('operations', ['uppercase'])
        
        try:
            results = {}
            
            for operation in operations:
                if operation == 'uppercase':
                    results['uppercase'] = text.upper()
                elif operation == 'lowercase':
                    results['lowercase'] = text.lower()
                elif operation == 'reverse':
                    results['reverse'] = text[::-1]
                elif operation == 'word_count':
                    results['word_count'] = len(text.split())
                elif operation == 'char_count':
                    results['char_count'] = len(text)
                elif operation == 'title_case':
                    results['title_case'] = text.title()
                elif operation == 'remove_spaces':
                    results['remove_spaces'] = text.replace(' ', '')
                elif operation == 'first_word':
                    words = text.split()
                    results['first_word'] = words[0] if words else ""
                elif operation == 'last_word':
                    words = text.split()
                    results['last_word'] = words[-1] if words else ""
                else:
                    results[f'unknown_operation_{operation}'] = f"Unknown operation: {operation}"
            
            return {
                'original_text': text,
                'operations': operations,
                'results': results,
                'processed_by': f'TextProcessorNode[{self.name}]',
                'node_config': self.config
            }
            
        except Exception as e:
            logger.error(f"TextProcessorNode '{self.name}': processing failed: {e}")
            return {
                "error": str(e),
                "text": text,
                "operations": operations,
                "processed_by": f"TextProcessorNode[{self.name}]"
            }
    
    def get_supported_operations(self) -> list:
        """Get list of supported text operations."""
        return [
            'uppercase', 'lowercase', 'reverse', 'word_count', 
            'char_count', 'title_case', 'remove_spaces', 
            'first_word', 'last_word'
        ]


__all__ = ["TextProcessorNode"] 