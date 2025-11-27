---
name: Generate Python Node
description: Generate a Python MultiprocessNode from a GitHub, HuggingFace URL, or local project
category: Nodes
tags: [node, generation, python, model, multiprocess]
arguments:
  - name: source
    description: GitHub URL, HuggingFace URL, or local filesystem path to the model/project
    required: true
---

# Python Node Generation

Generate a Python `MultiprocessNode` for the RemoteMedia SDK pipeline.

**Source**: $ARGUMENTS.source

## Analysis Steps

1. **Analyze the source**:
   - For GitHub URLs: Fetch README, `requirements.txt`/`pyproject.toml`, main source files
   - For HuggingFace: Fetch model card, identify model type and pipeline
   - For local paths: Read project structure and dependencies

2. **Determine node characteristics**:
   - Input type (Audio, Text, Image, JSON)
   - Output type (Audio, Text, Image, JSON)
   - Streaming vs non-streaming
   - Required dependencies

3. **Generate the node** following this template:

```python
import logging
import numpy as np
from typing import AsyncGenerator, Optional, Union, Dict, Any
import asyncio

from remotemedia.core.multiprocessing.data import RuntimeData
from remotemedia.core import MultiprocessNode, NodeConfig

logger = logging.getLogger(__name__)


class {NodeName}(MultiprocessNode):
    """
    {Description}.

    Input: RuntimeData.{InputType}
    Output: RuntimeData.{OutputType}
    """

    def __init__(
        self,
        node_id: str = None,
        # Model-specific parameters
        config: Union[NodeConfig, Dict[str, Any]] = None,
        **kwargs
    ):
        if config is not None:
            super().__init__(config, **kwargs)
            params = config.params if isinstance(config, NodeConfig) else config.get('params', {})
        else:
            minimal_config = NodeConfig(
                node_id=node_id or "{snake_case_name}",
                node_type="{NodeName}",
                params={}
            )
            super().__init__(minimal_config, **kwargs)

        # Store configuration
        self.model = None
        self.is_streaming = False  # Set True for streaming nodes

    async def initialize(self) -> None:
        """Load the model."""
        logger.info(f"Initializing {NodeName}")
        try:
            # Import and load model
            # from library import Model
            # self.model = Model(...)
            pass
        except ImportError as e:
            raise ImportError(f"Missing dependency: {e}")

    async def process(self, data: RuntimeData) -> RuntimeData:
        """Process input and return output."""
        # For streaming nodes, change return type to AsyncGenerator[RuntimeData, None]
        # and use: yield RuntimeData.{output}(...)

        if not data.is_{input_type}():
            logger.warning(f"Expected {input_type}, got {data.data_type()}")
            return None

        input_data = data.as_{input_method}()

        # Process
        result = await asyncio.get_event_loop().run_in_executor(
            None, self._sync_process, input_data
        )

        return RuntimeData.{output_type}(result)

    def _sync_process(self, input_data):
        """Synchronous processing (run in thread pool)."""
        # Use self.model to process
        return input_data

    async def cleanup(self) -> None:
        """Clean up resources."""
        logger.info(f"Cleaning up {NodeName}")
        self.model = None
```

## Output Requirements

Provide:

1. **Complete node source code** at `python-client/remotemedia/nodes/{snake_case_name}.py`

2. **Export update** for `python-client/remotemedia/nodes/__init__.py`:
   ```python
   from .{snake_case_name} import {NodeName}
   ```

3. **Dependencies** to add to `pyproject.toml`

4. **Example manifest**:
   ```yaml
   nodes:
     - id: {node_id}
       node_type: {NodeName}
       executor: multiprocess
       params:
         # parameters here
   ```

5. **Usage example** showing integration in a pipeline

## RuntimeData Methods

**Checking types:**
- `.is_audio()`, `.is_text()`, `.is_image()`, `.is_json()`
- `.data_type()` - returns string name

**Getting data:**
- `.as_numpy()` - audio samples as numpy array
- `.as_text()` - text content as string
- `.as_json()` - JSON as dict

**Creating:**
- `RuntimeData.audio(samples: np.ndarray, sample_rate: int, channels: int = 1)`
- `RuntimeData.text(content: str)`
- `RuntimeData.json(data: dict)`
- `RuntimeData.image(data: np.ndarray, width: int, height: int)`
