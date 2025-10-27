# 🧭 RemoteMedia Remote Execution Specification  
### Integrating `RemoteProxyClient` and `RemoteExecutorConfig` into the Pipeline Runtime

---

## 1. Overview

RemoteMedia supports remote execution of nodes through the **Remote Proxy Layer**, built around two central classes:
- `RemoteExecutorConfig` — defines remote endpoint connectivity and runtime options.
- `RemoteProxyClient` — provides an async client interface to create, manage, and stream remote node execution.

This layer allows developers to transparently execute parts of a pipeline on remote machines or services without redefining node classes or altering pipeline semantics.

The system must preserve **local-first ergonomics** and allow remote execution to be an *opt-in property of node instantiation*, not of the pipeline itself.

---

## 2. Execution Model

In RemoteMedia, **nodes decide where and how they execute**, not the pipeline.  
Each node can attach a `RemoteExecutorConfig`, signaling to the runtime that it should be proxied to a remote service.

The pipeline engine handles this transparently:
- The developer writes normal Python node code.
- The runtime serializes that node’s state and sends it to a `RemoteProxyClient` session.
- All future `.process()` or `.stream()` calls are executed remotely through the proxy connection.

---

## 3. Components

### 3.1 `RemoteExecutorConfig`
Defines connection details for a remote execution service.

```python
config = RemoteExecutorConfig(
    host="localhost",
    port=50052,
    ssl_enabled=False,
    max_message_size=10485760,  # optional
    reconnect=True,
)
```

This class is serializable and can be stored within node configuration to allow automated reconnection or dispatch.

**Fields:**
- `host`: Remote server hostname or IP.
- `port`: gRPC or WebRTC negotiation port.
- `ssl_enabled`: Whether TLS/SSL should be used.
- `max_message_size`: Optional override for large tensor or audio frames.
- `reconnect`: Automatically retry sessions if connection drops.

---

### 3.2 `RemoteProxyClient`
Manages a live session to the remote executor.

```python
async with RemoteProxyClient(config) as client:
    processor = StreamingDataProcessor()
    remote = await client.create_proxy(processor)
```

Once the proxy is created:
- Method calls on the returned proxy are executed remotely.
- Streaming APIs (`async for frame in remote.stream(...)`) maintain live duplex channels.
- Serialization and backpressure are handled internally.

Under the hood, the `RemoteProxyClient` uses:
- **gRPC streaming** for structured data (control, JSON, tensor batches).
- **WebRTC** for media or low-latency audio/video streams.
- **Compression and chunking** for large payloads.

---

## 4. Pipeline Integration

### Node-Level Remote Execution
Nodes themselves are *not declared remote* — the **instances added to the pipeline** determine that.

Example:

```python
remote_config = RemoteExecutorConfig(host="localhost", port=50052, ssl_enabled=False)

ultravox_instance = UltravoxNode(
    model_id="fixie-ai/ultravox-v0_5-llama-3_1-8b",
    system_prompt="You are a helpful assistant...",
    buffer_duration_s=10.0,
    name="UltravoxNode"
)

protected_ultravox = UltravoxMinDurationWrapper(
    ultravox_node=ultravox_instance,
    min_duration_s=1.0,
    sample_rate=16000,
    name="ProtectedUltravox"
)

remote_node = RemoteObjectExecutionNode(
    obj_to_execute=protected_ultravox,
    remote_config=remote_config,
    name="RemoteUltravox",
    node_config={'streaming': True}
)

remote_node.is_streaming = True
pipeline.add_node(remote_node)
```

Here, the node’s **execution context** determines its remoteness — not its definition.

- `RemoteObjectExecutionNode` wraps any arbitrary object implementing `.process()` or `.stream()`.
- The node serializes and transmits the wrapped object to the configured remote executor.
- The remote service hosts the object, exposes its methods, and streams results back.

This pattern makes remote execution composable — any local node can be wrapped and executed remotely without rewriting its implementation.

---

## 5. Lifecycle

### 5.1 Local Phase
1. Pipeline constructed normally in Python.
2. Node added with an attached `RemoteExecutorConfig`.
3. Pipeline runtime identifies remote-capable nodes and prepares the dispatch list.

### 5.2 Dispatch Phase
1. Serialize node object (`pickle` or `msgpack`).
2. Connect to remote host using `RemoteProxyClient`.
3. Upload node definition and configuration.
4. Remote executor instantiates the node on the other side.

### 5.3 Execution Phase
- For each `.process()` call, input data is serialized and sent.
- Results are streamed back via the same channel.
- Streaming nodes maintain persistent bidirectional channels.

### 5.4 Teardown Phase
- The `RemoteProxyClient` gracefully closes all channels.
- Remote runtime cleans up instantiated objects.

---

## 6. RemoteExecutor Responsibilities

A `RemoteExecutor` service (implemented in Rust/Python hybrid) must:
1. Receive serialized node objects and configurations.
2. Instantiate them inside its runtime sandbox.
3. Handle `.process()` or `.stream()` calls over WebRTC/gRPC.
4. Return results, status, and heartbeat messages.
5. Respect node-level policies (timeouts, memory, execution mode).

It can execute:
- **WASM nodes** in sandboxed environments.
- **Python or AI/ML nodes** directly if trusted (via RustPython or CPython subprocess).

---

## 7. Example: Hybrid Remote Execution Pipeline

```python
from remotemedia import Pipeline, RemoteExecutorConfig, RemoteObjectExecutionNode

remote = RemoteExecutorConfig(host="10.0.0.12", port=50052)

# A local ASR node
asr = WhisperNode(model="tiny.en")

# Remote TTS node
remote_tts = RemoteObjectExecutionNode(
    obj_to_execute=TTSNode(model="bark"),
    remote_config=remote,
    name="TTSRemote"
)

pipeline = Pipeline("speech_chain")
pipeline.add_node(asr)
pipeline.add_node(remote_tts)
pipeline.run()
```

**Outcome:** The ASR executes locally; the TTS node executes remotely through the proxy client, streaming results back in real time.

---

## 8. Key Takeaways

- Nodes remain plain Python objects.
- Remote execution is configured **per-instance**, not per-class or per-pipeline.
- The `RemoteProxyClient` manages network, streaming, and object lifecycle.
- The runtime automatically distinguishes between local, remote, and sandboxed execution contexts.
- No configuration files or manifests are needed; everything is defined in code.

---

## 9. Future Extensions

- **Dynamic routing:** Runtime decides optimal node placement (local vs. remote) at execution time.
- **Session pooling:** Reuse proxy sessions for multiple nodes.
- **Remote discovery:** Nodes can auto-locate available executors using signaling.
- **WebRTC-first transport:** Use WebRTC as the default for streaming nodes with adaptive bitrate and latency control.

---

**In short:**  
RemoteMedia’s remote execution model brings distributed compute to any pipeline, allowing ad-hoc remote execution of AI/ML workloads while preserving local simplicity.  
Developers write standard Python nodes; the SDK handles remote serialization, transfer, and streaming — transparently, efficiently, and securely.

