"""
Text Collector Node

Buffers incoming text tokens and yields complete sentences/patterns.
This node is useful for streaming text-to-speech where you want to accumulate
text tokens into complete sentences before passing to downstream nodes.

Key features:
- Buffers text tokens until a sentence boundary is detected
- Configurable split pattern (punctuation, newlines, etc.)
- Handles special tokens like <|text_end|> and <|audio_end|>
- Yields RuntimeData.Text for complete sentences
"""

import logging
import re
from typing import AsyncGenerator, Optional, TYPE_CHECKING

# Import RuntimeData bindings
if TYPE_CHECKING:
    from remotemedia.core.multiprocessing.data import RuntimeData

try:
    from remotemedia.core.multiprocessing.data import RuntimeData
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    RuntimeData = None  # type: ignore
    logging.warning("[TextCollectorNode] RuntimeData bindings not available. Using fallback implementation.")

logger = logging.getLogger(__name__)

# Configure logger
if not logger.handlers:
    console_handler = logging.StreamHandler()
    console_handler.setLevel(logging.INFO)
    formatter = logging.Formatter('%(levelname)s:%(name)s:%(message)s')
    console_handler.setFormatter(formatter)
    logger.addHandler(console_handler)
    logger.setLevel(logging.INFO)


class TextCollectorNode:
    """
    Text collector node that buffers text tokens and yields complete sentences.

    This node accumulates streaming text tokens and yields them as complete
    sentences/phrases when punctuation boundaries are detected.
    """

    def __init__(
        self,
        node_id: str,
        split_pattern: str = r'[.!?,;:\n]+',
        min_sentence_length: int = 3,
        yield_partial_on_end: bool = True,
        **kwargs
    ):
        """
        Initialize TextCollectorNode.

        Args:
            node_id: Unique identifier for this node instance
            split_pattern: Regex pattern to detect sentence boundaries (default: punctuation + newlines)
            min_sentence_length: Minimum sentence length before yielding (default: 3 chars)
            yield_partial_on_end: If True, yield partial sentences when <|text_end|> is received
        """
        self.node_id = node_id
        self.split_pattern = split_pattern
        self.min_sentence_length = min_sentence_length
        self.yield_partial_on_end = yield_partial_on_end
        self.is_streaming = True

        # Compile regex pattern for performance
        self._boundary_pattern = re.compile(split_pattern)

        logger.info(
            f"TextCollectorNode initialized: pattern='{split_pattern}', "
            f"min_length={min_sentence_length}"
        )

    async def initialize(self) -> None:
        """Initialize the node (no-op for this node)."""
        logger.info("TextCollectorNode initialized")

    async def cleanup(self) -> None:
        """Clean up the node (no-op for this node)."""
        logger.info("TextCollectorNode cleaned up")

    async def process(
        self,
        data: RuntimeData
    ) -> AsyncGenerator[RuntimeData, None]:
        """
        Process incoming text data and yield complete sentences.

        Args:
            data: RuntimeData containing text chunks (RuntimeData.Text)

        Yields:
            RuntimeData.Text for complete sentences
        """
        # Buffer for accumulating text
        text_buffer = ""

        try:
            # Validate input type
            if not data.is_text():
                logger.warning(f"TextCollectorNode expects text input, got {data.data_type()}")
                # Pass through non-text data
                yield data
                return

            # Extract text content
            text_chunk = data.as_text()
            logger.info(f"Received text chunk: '{text_chunk}'")

            # Check for special end tokens
            is_text_end = '<|text_end|>' in text_chunk
            is_audio_end = '<|audio_end|>' in text_chunk

            # Remove special tokens from the text
            cleaned_text = text_chunk
            cleaned_text = cleaned_text.replace('<|text_end|>', '')
            cleaned_text = cleaned_text.replace('<|audio_end|>', '')
            cleaned_text = cleaned_text.replace('<|[^|]+|>', '')  # Remove other special tokens

            # Add to buffer
            text_buffer += cleaned_text
            logger.info(f"Buffer now: '{text_buffer}'")

            # Find sentence boundaries in the buffer
            # We'll yield all complete sentences and keep the remainder
            sentences = []
            last_end = 0

            for match in self._boundary_pattern.finditer(text_buffer):
                # Get text up to and including the delimiter
                sentence_end = match.end()
                sentence = text_buffer[last_end:sentence_end].strip()

                # Only yield if meets minimum length
                if len(sentence) >= self.min_sentence_length:
                    sentences.append(sentence)
                    last_end = sentence_end

            # Yield complete sentences
            for sentence in sentences:
                logger.info(f"Yielding complete sentence: '{sentence}'")
                yield RuntimeData.text(sentence)

            # Update buffer to keep the remainder
            text_buffer = text_buffer[last_end:].strip()
            logger.info(f"Remaining buffer: '{text_buffer}'")

            # Handle end tokens
            if is_text_end or is_audio_end:
                # Yield any remaining partial sentence if configured
                if self.yield_partial_on_end and text_buffer:
                    logger.info(f"Yielding partial sentence on end: '{text_buffer}'")
                    yield RuntimeData.text(text_buffer)
                    text_buffer = ""

                # Pass through the end token
                if is_text_end:
                    logger.info("Yielding <|text_end|>")
                    yield RuntimeData.text("<|text_end|>")
                if is_audio_end:
                    logger.info("Yielding <|audio_end|>")
                    yield RuntimeData.text("<|audio_end|>")

        except Exception as e:
            logger.error(f"Error in TextCollectorNode: {e}", exc_info=True)
            # Yield error message
            yield RuntimeData.text(f"ERROR: Text collection failed: {str(e)}")

    def get_config(self) -> dict:
        """Get node configuration."""
        return {
            "node_id": self.node_id,
            "node_type": "TextCollectorNode",
            "split_pattern": self.split_pattern,
            "min_sentence_length": self.min_sentence_length,
            "yield_partial_on_end": self.yield_partial_on_end,
        }


# Example usage
async def main():
    """
    Example demonstrating TextCollectorNode
    """
    if not RUNTIME_DATA_AVAILABLE:
        print("RuntimeData bindings not available. Please build the Rust extension.")
        return

    import asyncio

    print("=" * 60)
    print("Text Collector Node Example")
    print("=" * 60)

    # Create collector node
    collector = TextCollectorNode(
        node_id="text_collector_1",
        split_pattern=r'[.!?,;:\n]+',
        min_sentence_length=3,
    )

    await collector.initialize()

    # Simulate streaming text tokens
    text_chunks = [
        "Hello",
        " world",
        "!",
        " How",
        " are",
        " you",
        " doing",
        " today",
        "?",
        " I'm",
        " doing",
        " great",
        ".",
        "<|text_end|>",
    ]

    print("\nSimulating streaming text tokens:")
    for chunk in text_chunks:
        print(f"  Input chunk: '{chunk}'")
        text_data = RuntimeData.text(chunk)

        async for output in collector.process(text_data):
            if output.is_text():
                text_out = output.as_text()
                print(f"  → Output: '{text_out}'")

    await collector.cleanup()
    print("\n" + "=" * 60)


if __name__ == "__main__":
    import asyncio
    asyncio.run(main())
