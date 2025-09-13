# WasmEdge Integration Architecture

## System Architecture Overview

The WasmEdge integration extends RemoteMedia's pipeline architecture with intelligent edge processing capabilities while maintaining seamless integration with existing gRPC remote services.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Client Application                        │
│              (Python / Node.js / Browser / Mobile)               │
└────────────────────────────┬─────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                      RemoteMedia Pipeline                        │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                   IntelligentRouter                      │   │
│  │  ┌─────────────┐  Decision Engine  ┌──────────────┐    │   │
│  │  │   Metrics   │◄──────┬──────────►│   Fallback   │    │   │
│  │  │  Collector  │       │           │   Handler    │    │   │
│  │  └─────────────┘       ▼           └──────────────┘    │   │
│  └──────────┬──────────────┴────────────────┬──────────────┘   │
│             │                                │                   │
│        Edge Path                        Cloud Path               │
│             ▼                                ▼                   │
│  ┌──────────────────┐            ┌────────────────────┐        │
│  │  WasmEdgeNode    │            │ RemoteObjectNode   │        │
│  │                  │            │                    │        │
│  │  ┌────────────┐  │            │  ┌──────────────┐ │        │
│  │  │ WASM VM    │  │            │  │ gRPC Client  │ │        │
│  │  │            │  │            │  │              │ │        │
│  │  │ ┌────────┐ │  │            │  └──────┬───────┘ │        │
│  │  │ │ Module │ │  │            └──────────┼─────────┘        │
│  │  │ └────┬───┘ │  │                       │                  │
│  │  └──────┼─────┘  │                       │                  │
│  │         │        │                       │                  │
│  │    Host Functions│                       │                  │
│  │         ▼        │                       │                  │
│  │  ┌────────────┐  │                       │                  │
│  │  │gRPC Bridge │──┼───────────────────────┘                  │
│  │  └────────────┘  │                                          │
│  └──────────────────┘                                          │
└─────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Remote Processing Service                     │
│                        (gRPC Server)                             │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ Registered Nodes: UltravoxNode, KokoroTTS, VAD, etc.     │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. WasmEdgeNode
The base node for executing WebAssembly modules within the pipeline.

**Key Features:**
- **Session Management**: Maintains per-session WASM VM instances
- **Memory Safety**: Configurable memory limits and sandboxed execution
- **Streaming Support**: Full async generator support for real-time processing
- **Host Functions**: Extensible interface for WASM → Python communication

**Integration Points:**
- Extends `remotemedia.core.node.Node` base class
- Compatible with existing Pipeline execution model
- Supports session state management

### 2. HybridWasmNode
Enhanced WASM node with bidirectional gRPC communication capabilities.

**Key Features:**
- **gRPC Client Integration**: Reuses existing `RemoteExecutionClient`
- **Host Function Bridge**: WASM modules can call remote services
- **Session Continuity**: Maintains session across edge-cloud boundaries
- **Transparent Serialization**: Automatic data conversion between WASM and gRPC

**Communication Flow:**
```
WASM Module → Host Function → gRPC Bridge → Remote Service
     ↑                                             │
     └─────────────── Response ────────────────────┘
```

### 3. IntelligentRouter
Routes processing intelligently between edge (WASM) and cloud (gRPC) based on data characteristics and system metrics.

**Decision Factors:**
- **Data Size**: Small data → Edge, Large data → Cloud
- **Complexity**: Simple operations → Edge, ML/Complex → Cloud
- **Latency Requirements**: Real-time → Edge, Batch → Cloud
- **Resource Availability**: CPU/Memory constraints
- **Success Metrics**: Historical performance data

**Routing Strategies:**
1. **Edge-First**: Try edge, fallback to cloud on failure
2. **Cloud-First**: Default to cloud for reliability
3. **Auto**: Dynamic decision based on metrics
4. **Hybrid**: Split processing between edge and cloud

### 4. gRPC Bridge
Enables WASM modules to communicate with remote services.

**Responsibilities:**
- **Service Discovery**: Map service names to gRPC endpoints
- **Serialization**: Convert between WASM memory and gRPC messages
- **Error Handling**: Propagate gRPC errors to WASM
- **Connection Management**: Reuse existing gRPC connections

## Data Flow Patterns

### Pattern 1: Edge-Only Processing
```
Input → WasmEdgeNode → Output
```
Use Case: Simple, latency-critical operations (VAD, preprocessing)

### Pattern 2: Cloud-Only Processing
```
Input → RemoteObjectNode → gRPC → Remote Service → Output
```
Use Case: Complex ML models, resource-intensive operations

### Pattern 3: Hybrid Processing
```
Input → WasmEdgeNode (preprocess) → RemoteObjectNode (ML) → WasmEdgeNode (postprocess) → Output
```
Use Case: Optimize bandwidth and latency while leveraging cloud ML

### Pattern 4: Intelligent Routing
```
Input → IntelligentRouter → [Edge | Cloud] → Output
            ↑
      Decision Engine
```
Use Case: Automatic optimization based on real-time conditions

### Pattern 5: WASM with Remote Calls
```
Input → HybridWasmNode → Local Processing
                ↓
         Need Remote Service?
                ↓
         gRPC Bridge → Remote Service
                ↓
         Continue Local Processing
                ↓
             Output
```
Use Case: Edge processing with selective cloud augmentation

## Memory Management

### WASM VM Lifecycle
```
Session Start → Create VM → Load Module → Initialize
      ↓
   Processing → Execute Functions → Host Calls
      ↓
Session End → Cleanup → Destroy VM
```

### Resource Limits
- **Per-Module Memory**: Configurable (default: 128MB)
- **Execution Timeout**: Configurable (default: 30s)
- **VM Pool Size**: Based on concurrent sessions
- **Cache Strategy**: LRU for compiled modules

## Security Model

### Sandboxing Layers
1. **WASM Sandbox**: Memory isolation, capability-based security
2. **Host Function Control**: Whitelist allowed operations
3. **gRPC Authentication**: Reuse existing auth tokens
4. **Resource Limits**: Prevent resource exhaustion

### Trust Boundaries
```
Untrusted Input → WASM Sandbox → Validated Host Functions → Trusted gRPC
```

## Performance Optimization

### Edge Optimization
- **Module Caching**: Compiled WASM modules cached in memory
- **VM Pooling**: Reuse VM instances across requests
- **JIT Compilation**: WasmEdge JIT for hot code paths
- **SIMD Support**: Hardware acceleration for audio/video

### Routing Optimization
- **Adaptive Thresholds**: Learn optimal routing boundaries
- **Predictive Routing**: Anticipate workload patterns
- **Batch Processing**: Group similar requests
- **Circuit Breaking**: Fast failure detection

## Integration with Existing System

### Pipeline Integration
```python
# Existing pipeline
pipeline = Pipeline([
    Node1(),
    Node2(),
    Node3()
])

# Enhanced with WASM
pipeline = Pipeline([
    WasmEdgeNode("preprocess.wasm"),  # New: Edge processing
    Node2(),                           # Existing: Remote node
    IntelligentRouter(                 # New: Hybrid routing
        edge_node=WasmEdgeNode("simple.wasm"),
        cloud_node=Node3()
    )
])
```

### Session Management
- Inherits session state from base `Node` class
- Session ID flows through edge and cloud nodes
- State persistence across processing boundaries

### Error Handling
- WASM errors propagated as `NodeError`
- gRPC errors maintain existing error codes
- Fallback mechanisms for edge failures

## Deployment Topology

### Development
```
Developer Machine
    ├── WASM Modules (local)
    ├── Python Client
    └── Remote Service (localhost:50052)
```

### Production - Distributed
```
Edge Locations (CDN/PoP)
    ├── WASM Modules
    ├── WasmEdge Runtime
    └── gRPC Client → Central Cloud

Central Cloud
    ├── Remote Processing Service
    ├── ML Models
    └── Heavy Processing Nodes
```

### Production - Hybrid
```
Client Device
    ├── WASM Modules (cached)
    ├── Local Processing
    └── Selective Cloud Calls → Remote Service
```

## Monitoring and Observability

### Metrics Collection
- **Edge Metrics**: Execution time, memory usage, cache hits
- **Cloud Metrics**: gRPC latency, service availability
- **Routing Metrics**: Decision accuracy, fallback rate
- **System Metrics**: CPU, memory, network usage

### Tracing
```
Request ID → Edge Processing → gRPC Call → Remote Processing → Response
     ↓             ↓               ↓              ↓              ↓
   Trace         Span 1          Span 2        Span 3      Complete
```

## Future Extensions

### Planned Enhancements
1. **Module Marketplace**: Share and discover WASM modules
2. **Auto-Optimization**: ML-based routing decisions
3. **Edge Clustering**: Distributed edge processing
4. **Module Composition**: Chain WASM modules dynamically
5. **Browser Runtime**: Direct browser execution
6. **Mobile SDKs**: iOS/Android native integration

This architecture provides a robust foundation for hybrid edge-cloud processing while maintaining full compatibility with your existing RemoteMedia pipeline system.