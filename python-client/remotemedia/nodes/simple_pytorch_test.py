"""
Simple PyTorch test node to isolate heap corruption issue.

This node performs minimal PyTorch operations to test if the issue
is with PyTorch in event loop context or something else.
"""
import logging
from typing import AsyncGenerator

try:
    from remotemedia_runtime.runtime_data import RuntimeData
    RUNTIME_DATA_AVAILABLE = True
except ImportError:
    RUNTIME_DATA_AVAILABLE = False
    RuntimeData = None
    logging.warning("RuntimeData bindings not available.")

logger = logging.getLogger(__name__)

# Configure logger
if not logger.handlers:
    console_handler = logging.StreamHandler()
    console_handler.setLevel(logging.INFO)
    formatter = logging.Formatter('%(levelname)s:%(name)s:%(message)s')
    console_handler.setFormatter(formatter)
    logger.addHandler(console_handler)
    logger.setLevel(logging.INFO)


class SimplePyTorchNode:
    """
    Minimal node that does a simple PyTorch operation.
    Tests if PyTorch works in async generator + event loop context.
    """

    def __init__(self, node_id: str, **kwargs):
        """Initialize the test node."""
        self.node_id = node_id
        self._initialized = False
        self._model = None

    async def initialize(self) -> None:
        """Initialize PyTorch model."""
        if self._initialized:
            return

        try:
            import torch
            logger.info("Importing PyTorch for simple test")

            # Create a minimal model - just a single linear layer
            logger.info("Creating minimal PyTorch model (single linear layer)")
            self._model = torch.nn.Linear(10, 1)

            self._initialized = True
            logger.info("SimplePyTorchNode initialized successfully")

        except ImportError as e:
            raise ImportError("PyTorch not installed") from e
        except Exception as e:
            logger.error(f"Failed to initialize SimplePyTorchNode: {e}")
            raise

    async def cleanup(self) -> None:
        """Clean up."""
        if self._model is not None:
            self._model = None
            self._initialized = False
            logger.info("SimplePyTorchNode cleaned up")

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """
        Process input using PyTorch.

        Args:
            data: RuntimeData containing text (ignored, we just use dummy data)

        Yields:
            RuntimeData.Text with test results
        """
        if not self._initialized:
            await self.initialize()

        logger.info("SimplePyTorchNode: Starting PyTorch operation")

        try:
            import torch
            import numpy as np

            # TEST: Do PyTorch operation INSIDE async generator to see if it crashes
            logger.info("INSIDE async generator: About to yield first (this will trigger async generator)")
            yield RuntimeData.text("Starting PyTorch test...")

            logger.info("INSIDE async generator: Creating dummy input")
            dummy_input = torch.randn(1, 10)

            logger.info("INSIDE async generator: Running forward pass")
            output = self._model(dummy_input)

            logger.info(f"INSIDE async generator: Got output shape {output.shape}")

            # Convert to numpy
            output_np = output.detach().cpu().numpy()
            logger.info(f"INSIDE async generator: Converted to numpy, shape {output_np.shape}")

            # Yield the result as RuntimeData.Text
            logger.info("INSIDE async generator: Yielding final result")
            result_text = f"PyTorch test completed! Output shape: {output_np.shape}, value: {output_np[0][0]:.4f}"
            yield RuntimeData.text(result_text)

            logger.info("SimplePyTorchNode: Completed successfully")

        except Exception as e:
            logger.error(f"Error during PyTorch operation: {e}")
            raise RuntimeError(f"PyTorch test failed: {e}") from e

    def get_config(self) -> dict:
        """Get node configuration."""
        return {
            "node_id": self.node_id,
            "node_type": "SimplePyTorchNode",
        }
