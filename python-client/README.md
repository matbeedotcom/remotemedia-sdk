# RemoteMedia Processing SDK

A Python SDK for building distributed audio/video/data processing pipelines with transparent remote offloading capabilities.

## Overview

The RemoteMedia Processing SDK enables developers to create complex, real-time processing applications that can seamlessly offload computationally intensive tasks to remote execution services. The SDK handles WebRTC communication, data synchronization, and remote execution while providing a transparent and intuitive developer experience.

## Key Features

- **Pythonic Pipeline API**: High-level, intuitive API for defining processing pipelines
- **Transparent Remote Offloading**: Execute processing nodes remotely with minimal code changes
- **Real-time A/V Processing**: Optimized for low-latency audio/video processing
- **WebRTC Integration**: Built-in WebRTC support for real-time communication
- **Flexible Architecture**: Support for both SDK-provided and custom processing nodes
- **Secure Remote Execution**: Sandboxed execution environment for user-defined code
- **CloudPickle Integration**: Serialize and execute user-defined Python classes remotely
- **AST-Based Dependency Analysis**: Automatic detection and packaging of local Python dependencies
- **Enhanced Code Packaging**: Handles modules from anywhere in the filesystem, not just project directory
- **Automatic Module Loading**: Pre-loads Python modules on the server for proper deserialization
- **Pip Package Dependencies**: Automatically install required packages on remote servers
- **TypeScript/Node.js Support**: Export TypeScript interface definitions for type-safe Node.js integration
- **Pipeline Export/Import**: Export complete pipeline definitions for cross-language interoperability
- **JavaScript Pipeline Integration**: Create and execute pipelines from JavaScript/TypeScript clients
- **Dynamic Pipeline Registry**: Discover and execute registered pipelines via gRPC

## Development Status

**Current Phase**: Phase 4 - WebRTC Real-time Audio Processing (COMPLETE) ✅

**Phase 4 Achievements:**
- ✅ **WebRTC Server Integration**: Real-time audio/video streaming with aiortc
- ✅ **Voice Activity Detection (VAD)**: Speech segmentation with buffering
- ✅ **Speech-to-Speech Pipeline**: Ultravox STT + Kokoro TTS integration
- ✅ **Remote Proxy Client**: Transparent remote execution for ANY Python object

**Phase 3 Achievements:**
- ✅ **Remote Python Code Execution**: Full support for executing user-defined Python code remotely
- ✅ **CloudPickle Class Serialization**: Serialize and execute custom Python classes with state preservation
- ✅ **AST-Based Dependency Analysis**: Automatic detection of local Python file dependencies
- ✅ **Code & Dependency Packaging**: Complete packaging system for deployable archives
- ✅ **Secure Execution Environment**: Sandboxed remote execution with restricted globals
- ✅ **Comprehensive Testing**: 7/7 test scenarios passing (4 CloudPickle + 3 dependency packaging)

**What Works Now:**
- **NEW**: RemoteProxyClient - Make ANY Python object remote with one line of code
- WebRTC real-time audio processing with proper frame timing
- Voice-triggered speech-to-speech conversation system
- Users can define Python classes locally with custom dependencies
- AST analysis automatically detects and packages local Python file imports
- CloudPickle enables serialization of complex user-defined objects
- Remote execution preserves object state and functionality across network boundaries
- End-to-end remote code execution with proper error handling and logging

See `PHASE_3_PROJECT_TRACKING.md` for detailed status and `DevelopmentStrategyDocument.md` for complete roadmap.

## Quick Start

### Local Processing Pipeline
```python
from remotemedia.core import Pipeline
from remotemedia.nodes import MediaReaderNode, AudioResampleNode, MediaWriterNode

# Create a simple local processing pipeline
pipeline = Pipeline(
    MediaReaderNode(file_path="input.mp3"),
    AudioResampleNode(target_sample_rate=16000),
    MediaWriterNode(output_path="output.wav")
)
pipeline.run()
```

### Remote Code Execution
The SDK makes it simple to define a node locally and have it execute on a remote server. This is ideal for offloading heavy ML workloads.

```python
# client_script.py
from remotemedia.core import Pipeline
from remotemedia.nodes import MediaReaderNode, MediaWriterNode, RemoteObjectExecutionNode
from my_custom_nodes import AudioEchoEffect # A custom node defined in your project

# 1. Instantiate your custom node locally.
#    This object will be serialized and sent to the server for execution.
echo_effect = AudioEchoEffect(delay_seconds=0.5, decay_factor=0.6)

# 2. Wrap it in a RemoteObjectExecutionNode
remote_echo_node = RemoteObjectExecutionNode(node_object=echo_effect)

# 3. Build the pipeline. The remote node fits in just like any other.
pipeline = Pipeline(
    MediaReaderNode(file_path="input.wav"),
    remote_echo_node,
    MediaWriterNode(output_path="output_with_echo.wav")
)

# When run, the pipeline will transparently execute the echo effect on the remote server.
pipeline.run()
```

### Remote Proxy Client (NEW!)
The RemoteProxyClient provides the simplest way to execute ANY Python object remotely:

```python
from remotemedia.remote import RemoteProxyClient
from remotemedia.core.node import RemoteExecutorConfig

# Configure connection
config = RemoteExecutorConfig(host="localhost", port=50052)

async with RemoteProxyClient(config) as client:
    # Make ANY object remote with just ONE line!
    calculator = Calculator()
    remote_calc = await client.create_proxy(calculator)
    
    # Use it exactly like a local object (just add await)
    result = await remote_calc.add(5, 3)
    print(f"5 + 3 = {result}")  # Executed on remote server!
    
    # Keyword arguments work transparently!
    result = await remote_calc.calculate(operation="multiply", a=10, b=4)
    
    # The remote object maintains state
    history = await remote_calc.history()  # State persists remotely
```

**With Pip Package Dependencies (NEW!):**
```python
# Specify pip packages that your remote code needs
config = RemoteExecutorConfig(
    host="localhost", 
    port=50052,
    pip_packages=["numpy", "pandas", "scipy", "requests"]
)

async with RemoteProxyClient(config) as client:
    # Your object can now use these packages on the remote server!
    data_processor = DataProcessor()
    remote_processor = await client.create_proxy(data_processor)
    
    # The remote server automatically installs packages in a virtual environment
    result = await remote_processor.analyze_with_pandas(data)
```

**Key Features:**
- **One-line remote conversion**: `remote_obj = await client.create_proxy(obj)`
- **Works with ANY Python object**: No special base class required
- **Transparent usage**: Call methods exactly as you would locally
- **State persistence**: Objects maintain state on the remote server
- **Session management**: Automatic session handling with unique IDs
- **Generator support**: Generators automatically materialized to lists
- **Property support**: Access properties with `await`
- **Async method support**: Both sync and async methods work seamlessly
- **Pip package dependencies**: Automatically install required packages on remote server

**Supported Method Types:**
- ✅ Synchronous methods (automatically wrapped in async)
- ✅ Asynchronous methods
- ✅ Generator functions (automatically converted to lists)
- ✅ Async generator functions (automatically converted to lists)
- ✅ Properties and attributes (accessed with `await`)
- ✅ Static methods
- ✅ Most special methods (`__call__`, `__getitem__`, etc.)
- ✅ **Keyword arguments** (NEW): Full support for kwargs in all method types

**Generator Streaming Support (NEW!):** 
- ✅ **True streaming**: Generators now return proxy objects that fetch items as needed
- ✅ **Batched fetching**: Configurable batch size for optimal performance (default: 10 items)
- ✅ **Early termination**: Stop iteration at any time, server resources are freed
- ✅ **Memory efficient**: Only requested items are generated and transferred
- ✅ **Automatic cleanup**: Generators are properly closed on completion or error
- ✅ **Error propagation**: Server-side errors in generators are properly propagated to client

Example:
```python
# Generators now stream data instead of materializing to lists!
async for chunk in await remote_obj.read_large_file("data.bin"):
    process(chunk)
    if should_stop():
        break  # Generator properly closed on server
```

See `examples/test_streaming_generators.py` for comprehensive examples.

See `examples/simplest_proxy.py` and `examples/test_transparent_generators.py` for more examples.

### Pipeline Export and JavaScript Integration (NEW!)

The SDK now supports exporting complete pipeline definitions that can be discovered, executed, and even created by JavaScript/TypeScript clients:

#### Registering Pipelines for Export

```python
from remotemedia.core import Pipeline
from remotemedia.core.pipeline_registry import PipelineRegistry
from remotemedia.nodes import CalculatorNode, PassThroughNode

# Create a pipeline
pipeline = Pipeline(
    name="calculator_pipeline",
    nodes=[
        PassThroughNode(),
        CalculatorNode(),
        PassThroughNode()
    ]
)

# Register it with the global registry
registry = PipelineRegistry.get_instance()
pipeline_id = await registry.register_pipeline(
    name="calculator_pipeline",
    pipeline=pipeline,
    metadata={
        "description": "A simple calculator pipeline",
        "version": "1.0.0",
        "author": "Example Author"
    }
)

# Pipeline is now discoverable and executable via gRPC!
```

#### JavaScript Client Usage

```javascript
import { PipelineClient } from '@remote_media_processing/nodejs-client';

// Connect to the server
const client = new PipelineClient({
  host: 'localhost',
  port: 50052
});

// Discover available pipelines
const pipelines = await client.listPipelines();
console.log('Available pipelines:', pipelines);

// Get detailed info about a specific pipeline
const info = await client.getPipelineInfo('calculator_pipeline');
console.log('Pipeline nodes:', info.definition.nodes);

// Execute a registered pipeline
const result = await client.executePipeline('calculator_pipeline', {
  operation: 'multiply',
  args: [10, 5]
});
console.log('Result:', result); // { result: 50 }

// Stream data through a pipeline
const stream = client.streamPipeline('data_processing_pipeline');
stream.on('data', (chunk) => console.log('Received:', chunk));
stream.on('error', (err) => console.error('Error:', err));
stream.on('end', () => console.log('Stream complete'));

// Send data to the pipeline
await stream.send({ data: 'process this' });
await stream.end();
```

#### Creating Pipelines from JavaScript

```javascript
import { PipelineClient, PipelineBuilder } from '@remote_media_processing/nodejs-client';

const client = new PipelineClient({ host: 'localhost', port: 50052 });

// Build a pipeline definition in JavaScript
const builder = new PipelineBuilder('my_js_pipeline');
builder
  .addNode('DataSourceNode', { buffer_size: 100 })
  .addNode('CalculatorNode', {})
  .addNode('DataSinkNode', { result_key: 'output' })
  .connect(0, 1)  // Connect source to calculator
  .connect(1, 2); // Connect calculator to sink

// Register the JavaScript-created pipeline on the server
const pipelineId = await client.registerPipeline(
  'my_js_pipeline',
  builder.build(),
  {
    metadata: {
      description: 'Pipeline created from JavaScript',
      source: 'nodejs-client'
    }
  }
);

// Now execute it
const result = await client.executePipeline(pipelineId, {
  operation: 'add',
  args: [3, 7]
});
```

#### Bidirectional Data Flow

The SDK provides special I/O nodes for JavaScript integration:

```python
from remotemedia.nodes import DataSourceNode, DataSinkNode, JavaScriptBridgeNode

# Create a pipeline with JavaScript I/O points
pipeline = Pipeline(
    name="js_interactive_pipeline",
    nodes=[
        DataSourceNode(buffer_size=100),  # Receives data from JavaScript
        YourProcessingNode(),
        JavaScriptBridgeNode(),            # Bidirectional JavaScript communication
        MoreProcessingNode(),
        DataSinkNode(result_key="output")  # Sends results to JavaScript
    ]
)
```

From JavaScript:
```javascript
// Create a bidirectional stream
const stream = client.streamPipeline('js_interactive_pipeline', {
  bidirectional: true
});

// Send data and receive processed results
stream.on('data', (result) => {
  console.log('Processed:', result);
  // Can send more data based on results
  if (result.needsMoreData) {
    stream.send({ moreData: true });
  }
});

stream.send({ initialData: 'start processing' });
```

#### Pipeline Discovery API

```javascript
// List all registered pipelines with filtering
const pipelines = await client.listPipelines({
  filter: {
    tags: ['audio', 'realtime'],
    author: 'team-audio'
  }
});

// Unregister when done
await client.unregisterPipeline('pipeline_id');
```

### TypeScript/Node.js Integration

Generate TypeScript interface definitions for type-safe Node.js integration:

```bash
# Generate TypeScript definitions
python scripts/generate_typescript_defs.py -o remotemedia-types.d.ts
```

Use from Node.js/TypeScript:

```typescript
import { RemoteExecutionClient, NodeType, AudioTransformConfig } from './remotemedia-types';

const config: RemoteExecutorConfig = {
  host: 'localhost',
  port: 50052,
  protocol: 'grpc'
};

const client = new RemoteExecutionClient(config);

// Execute with full type safety
const audioConfig: AudioTransformConfig = {
  sampleRate: 16000,
  channels: 1
};

const result = await client.executeNode(
  NodeType.AudioTransform,
  audioConfig,
  audioData
);
```

See [TypeScript/Node.js Usage Guide](docs/TYPESCRIPT_USAGE.md) for complete documentation.

### Pip Package Dependencies (NEW!)

The SDK now supports automatic installation of pip packages on the remote server:

```python
from remotemedia.core.node import RemoteExecutorConfig
from remotemedia.remote import RemoteProxyClient

# Specify packages your remote code needs
config = RemoteExecutorConfig(
    host="localhost",
    port=50052,
    pip_packages=["beautifulsoup4", "requests", "pillow", "matplotlib"]
)

class WebScraper:
    def scrape_images(self, url):
        import requests
        from bs4 import BeautifulSoup
        from PIL import Image
        import io
        
        # These imports work because packages are installed remotely!
        response = requests.get(url)
        soup = BeautifulSoup(response.content, 'html.parser')
        # ... process images with PIL ...

async with RemoteProxyClient(config) as client:
    scraper = WebScraper()
    remote_scraper = await client.create_proxy(scraper)
    
    # Packages are installed automatically in a virtual environment
    images = await remote_scraper.scrape_images("https://example.com")
```

**Features:**
- **Automatic installation**: Packages are installed when creating the proxy
- **Virtual environment isolation**: Each session gets its own virtual environment
- **Dependency resolution**: Package dependencies are automatically resolved
- **Error handling**: Clear error messages if packages fail to install
- **No server restart needed**: Add new packages dynamically per session

**Supported packages**: Any pip-installable package including:
- Scientific computing: `numpy`, `scipy`, `pandas`, `scikit-learn`
- Web scraping: `beautifulsoup4`, `requests`, `httpx`, `selenium`
- Image processing: `pillow`, `opencv-python`, `imageio`
- Machine learning: `torch`, `tensorflow`, `transformers`
- Data visualization: `matplotlib`, `seaborn`, `plotly`
- And many more!

See `examples/demo_with_pip_packages.py` for comprehensive examples.

## Installation

```bash
# Development installation
pip install -e .

# Or install from PyPI (when available)
pip install remotemedia

# Install with OmniASR support (multilingual transcription)
pip install -e ".[omniasr]"
```

## Environment Variables

### OmniASR Transcription

When using the OmniASR node for multilingual speech transcription:

- **`FAIRSEQ2_CACHE_DIR`** (optional): Directory for caching downloaded OmniASR models
  - Default: `~/.cache/fairseq2/`
  - Example: `export FAIRSEQ2_CACHE_DIR=/data/models/fairseq2`
  - Models are 1-6 GB depending on variant (omniASR_LLM_1B vs omniASR_LLM_300M)

- **`HF_TOKEN`** (optional): HuggingFace authentication token
  - Required only if accessing gated/private models
  - Example: `export HF_TOKEN=hf_...`
  - Get token from: https://huggingface.co/settings/tokens

### General Configuration

- **`REMOTEMEDIA_LOG_LEVEL`** (optional): Logging verbosity
  - Values: `DEBUG`, `INFO`, `WARNING`, `ERROR`
  - Default: `INFO`

## Project Structure

```
remotemedia/                 # Core SDK package
├── core/                   # Core pipeline and node classes
│   ├── pipeline.py         # Pipeline management with export/import
│   ├── pipeline_registry.py # Global pipeline registry for discovery
│   ├── node.py             # Base Node and RemoteExecutorConfig
│   └── exceptions.py       # Custom exceptions
├── nodes/                  # Built-in processing nodes
│   ├── base.py             # Basic utility nodes (PassThrough, Buffer)
│   ├── audio.py            # Audio processing nodes
│   ├── video.py            # Video processing nodes
│   ├── transform.py        # Data transformation nodes
│   ├── calculator.py       # Calculator node for testing
│   ├── text_processor.py   # Text processing node
│   ├── code_executor.py    # Remote Python code execution
│   ├── serialized_class_executor.py  # CloudPickle class execution
│   └── io_nodes.py         # DataSource/Sink nodes for JavaScript I/O
├── packaging/              # Code & dependency packaging (Phase 3)
│   ├── dependency_analyzer.py  # AST-based import analysis
│   └── code_packager.py    # Archive creation with dependencies
├── webrtc/                 # WebRTC communication
│   └── manager.py          # WebRTC connection manager
├── remote/                 # Remote execution client
│   ├── client.py           # gRPC remote execution client
│   └── proxy_client.py     # Transparent proxy for ANY Python object
├── serialization/          # Data serialization utilities
│   └── base.py             # JSON and Pickle serializers
├── utils/                  # Common utilities
│   └── logging.py          # Logging configuration
└── cli.py                  # Command-line interface

examples/                   # Example applications
├── basic_pipeline.py       # Basic local pipeline usage
├── simple_remote_test.py   # Remote execution examples
└── README.md               # Examples documentation

tests/                      # Comprehensive test suite
├── test_pipeline.py        # Pipeline class tests
├── test_connection.py      # Basic connection tests
├── test_working_system.py  # System integration tests
├── test_remote_code_execution.py     # Remote Python execution
├── test_cloudpickle_execution.py     # CloudPickle class execution
├── test_dependency_packaging.py      # AST analysis & packaging
├── test_custom_node_remote_execution.py  # Custom node execution
├── test_custom_library_packaging.py  # Custom library tests
├── test_existing_custom_library.py   # Real file dependency tests
├── import_detection_tests/ # Test files for dependency analysis
└── run_remote_test.py      # Test runner utilities

remote_service/             # Remote execution service (Docker)
├── src/                    # gRPC server implementation
├── Dockerfile              # Container configuration
├── requirements.txt        # Service dependencies
└── README.md               # Service documentation

docs/                       # Documentation
scripts/                    # Development scripts
```

## Documentation

- [**Developer Guide**](DEVELOPER_GUIDE.md) - **Start here!** Essential guide for building with the SDK.
- [**Pipeline Developer Guide**](PIPELINE_DEVELOPER_GUIDE.md) - Complete guide to pipeline export and JavaScript integration
- [**Pipeline Registry Integration**](PIPELINE_REGISTRY_INTEGRATION.md) - WebRTC server integration with pipeline registry
- [**TypeScript/Node.js Usage**](docs/TYPESCRIPT_USAGE.md) - Guide for using the SDK from Node.js applications
- [Development Strategy](DevelopmentStrategyDocument.md)
- [Project Tracking](PROJECT_TRACKING.md)
- [API Documentation](docs/) (Coming soon)

## Contributing

This project is in early development. Please see `PROJECT_TRACKING.md` for current development status and priorities.

## License

[License to be determined]

## Requirements

- Python 3.9+
- See `requirements.txt` for dependencies 