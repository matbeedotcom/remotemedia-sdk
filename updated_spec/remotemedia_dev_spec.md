# 🧭 RemoteMedia Developer Specification (Unified)

This document merges all improvements from both `remotemedia_dev_spec` and `remotemedia_dev_spec_updated`. It reflects the current architecture with dynamic node definitions, optional capability-based scheduling, and the hybrid Rust → RustPython → WASM execution model.

### *“OCI for AI pipelines, backed by WebRTC transport and WASM execution”*

---

## 0. Philosophy

RemoteMedia’s primary goal is **code → result** with minimal friction.\
No configuration files, no manifest editing, no YAML.\
Every pipeline is authored in Python as ordinary code.\
All packaging, serialization, transport, and remote execution happen **automatically**.

---

## 1. Design Principles

1. **Python as SDL (System Description Language)**

   - Pipelines are authored in Python; this defines the declarative graph.
   - The runtime (Rust/WASM) interprets the generated manifest, not the Python code itself.

2. **Per-Pipeline Autonomy**

   - Every pipeline describes its own runtime graph and connection boundaries.
   - A pipeline can reference other pipelines (local or remote).

3. **Node = Pipeline**

   - Pipelines are composable: a node may encapsulate a sub-pipeline or a pre-compiled pipeline package (`.rmpkg`).

4. **Automatic Execution Placement**

   - The runtime decides *where* to execute each node (local, remote, GPU, browser).
   - The developer only provides host information or remote hint if desired.

5. **Zero Configuration UX**

   - Default behavior: “import, build, run.”
   - Remote servers, registry paths, and caching all handled automatically.

6. **Transparent Transport**

   - Pipelines communicate via WebRTC channels (media + data) for real-time streams.
   - For batch tasks, gRPC remains supported.

7. **Portable Execution**

   - Pipelines can be packaged, signed, cached, and executed anywhere: browser, edge, or data center.

---

## 2. Developer Ergonomics Manifesto

To keep the SDK usable and intuitive:

- **One-liners must work.**\
  If it can’t run with just `.run()`, it’s too complex.

- **Configuration is a last resort.**\
  Only experts editing `.remotemedia/config.json` should ever see internal knobs.

- **Implicit always before explicit.**\
  Auto-discover peers, cache modules, and open transports automatically.

- **The default path should do the smart thing.**\
  Remote execution, WASM fallback, and caching all happen without flags.

- **Every advanced feature should be opt-in via code, not files.**

### Example

```python
from remotemedia import Pipeline, AudioSource, HFPipelineNode, AudioSink

p = Pipeline("voice_assistant")
p.add(AudioSource(device="default"))
p.add(HFPipelineNode("speech_to_text", host="ai.example.com"))
p.add(HFPipelineNode("text_to_speech", host="ai.example.com"))
p.add(AudioSink(device="default"))

p.run()
```

That’s all a user should write.\
The system negotiates connections, transfers pipelines if needed, executes remotely, and streams results.

---

## 3. System Architecture Overview

### Layers

| Layer                | Language    | Responsibility                                    |
| -------------------- | ----------- | ------------------------------------------------- |
| **Python SDK**       | Python      | Defines pipelines (SDL), orchestrates runtime     |
| **Rust Runtime**     | Rust        | Executes pipelines, manages WASM, WebRTC, caching |
| **WASM Sandbox**     | WASM        | Isolated portable node execution                  |
| **Remote Executor**  | Rust/Python | Executes received pipeline packages               |
| **Registry / Cache** | Any         | Stores `.rmpkg` artifacts                         |

---

## 4. Pipeline Model

Each pipeline:

- Has a **name**, **nodes**, and **edges**.
- Can serialize itself to a JSON manifest (`p.serialize()`).
- Can be exported as a pre-compiled `.rmpkg`.

### Node Types

- `PythonNode`: runs in RustPython VM.
- `RustNode`: native binary execution.
- `WasmNode`: portable execution.
- `HFPipelineNode`: bridge to another pipeline (local or remote).
- `IO Nodes`: for streams (Audio/Video/Data sources and sinks).

---

## 5. WebRTC Transport Model

- **Signaling** handled automatically by `host` argument or environment config.
- **Data Channels** carry structured pipeline messages.
- **Media Tracks** handle audio/video.
- **Peer Negotiation** (offer/answer) is transparent to the user.

Each runtime exposes a **WebRTC endpoint**:

- Auto-registers with a signaling service.
- Can act as initiator or responder.
- Can transfer pipeline packages directly over the data channel.

---

## 6. Pipeline Packaging (`.rmpkg`)

Each pipeline can be compiled into an OCI-style package containing:

- `manifest.json` – pipeline SDL.
- `modules/` – node binaries (.wasm, .pynode).
- `models/` – optional model weights.
- `meta/` – runtime.json, signature, provenance.

Packaged via:

```bash
remotemedia build pipeline.py
```

Registered via:

```bash
remotemedia push oci://registry.ai/voice_tts:1.0
```

When a remote runtime sees a reference like:

```python
HFPipelineNode("voice_tts", remote_ref="oci://registry.ai/voice_tts:1.0")
```

it automatically:

1. Checks local cache.
2. Fetches the `.rmpkg` if missing.
3. Verifies signature.
4. Executes it in a sandbox.

---

## 7. Cache & Distribution

- Each runtime maintains a local cache (e.g. `~/.remotemedia/pipelines/<sha256>/`).
- Automatic cache warming when referenced by `HFPipelineNode`.
- Peer-to-peer pipeline transfer supported via WebRTC data channels.

No user configuration required.

---

## 8. Security & Isolation

- Every remote pipeline runs in a WASM sandbox with strict limits.
- All packages signed and verified before load.
- Python nodes run in RustPython, disallowing direct system calls.
- Memory and execution time capped per node.
- All WebRTC streams use DTLS-SRTP end-to-end encryption.

---

## 9. Developer API Summary

| Class                  | Role            | Notes                             |
| ---------------------- | --------------- | --------------------------------- |
| `Pipeline`             | Graph container | Defines nodes and flow            |
| `Node`                 | Base class      | Implements `.process()`           |
| `PythonNode`           | Python logic    | Executes locally or in RustPython |
| `WasmNode`             | Portable logic  | Executes anywhere                 |
| `HFPipelineNode`       | Remote bridge   | Streams to another pipeline       |
| `AudioSource / Sink`   | Media I/O       | Local or WebRTC                   |
| `Pipeline.run()`       | Execute         | Automatically handles transport   |
| `Pipeline.serialize()` | Export          | JSON SDL                          |
| `Pipeline.export()`    | Package         | Builds `.rmpkg`                   |
| `Pipeline.push()`      | Publish         | Push to registry                  |

---

## 10. Developer Onboarding Flow

1. **Install SDK**

   ```bash
   pip install remotemedia
   ```

2. **Author a pipeline**

   ```python
   from remotemedia import Pipeline, AudioSource, HFPipelineNode, AudioSink
   p = Pipeline("quicktest")
   p.add(AudioSource())
   p.add(HFPipelineNode("echo", host="localhost:9000"))
   p.add(AudioSink())
   p.run()
   ```

3. **Run remote executor**

   ```bash
   remotemedia serve --port 9000
   ```

4. **Optional: Package it**

   ```bash
   remotemedia build pipeline.py
   remotemedia push oci://registry.example.ai/quicktest
   ```

---

## 11. Responsibilities by Team

| Area                 | Ownership       | Deliverable                             |
| -------------------- | --------------- | --------------------------------------- |
| **Python SDK**       | Python team     | Pipeline DSL, runtime binding           |
| **Rust Runtime**     | Systems team    | Pipeline executor, WebRTC, WASM         |
| **Registry & Cache** | Infra           | Artifact hosting + signature validation |
| **Packager**         | Python team     | `remotemedia build` CLI                 |
| **Sandbox Security** | Systems + Infra | Runtime caps, signatures, attestation   |
| **Dev Experience**   | SDK team        | Docs, examples, IDE autocompletion      |

---

## 12. Core UX Rulebook

✅ Minimal code to success\
✅ No explicit manifests required\
✅ Automatic caching, fetching, and signing\
✅ Simple `.run()` entrypoint\
✅ Debuggability: `p.inspect()` prints full graph + backend placement\
✅ Everything async-compatible\
✅ Failure modes clear: “remote unreachable”, “package signature invalid”, etc.

---

## 13. MVP Execution Stack

For the MVP implementation, RemoteMedia will use a **Rust → RustPython → WASM** execution stack.

### Goals

- Reuse the existing Python-based pipeline DSL without modification.
- Allow execution of current `Node` classes (Python) inside an embedded RustPython VM.
- Wrap Python node execution within Rust, providing isolation and performance management.
- Introduce a minimal WASM host runtime to execute portable nodes and support sandboxed sub-pipelines.

### Architecture

1. **Rust Runtime (Host Layer)**

   - Manages the pipeline graph, node orchestration, and concurrency.
   - Controls lifecycle of RustPython interpreters and WASM instances.
   - Handles WebRTC, caching, and serialization.

2. **RustPython Layer (Embedded Interpreter)**

   - Executes existing Python nodes directly from current codebase.
   - Uses the same node contracts (`.process()`, `.stream()`), ensuring zero code rewrite.
   - All Python logging, error handling, and return data propagate through Rust.

3. **WASM Layer (Portable Sandbox)**

   - Hosts sandboxed or precompiled nodes (`.wasm` binaries).
   - Invoked via WASM runtime bindings managed by the Rust host.
   - Used primarily for portable or untrusted nodes.

4. **Data Flow**

   ```text
   Pipeline SDL (Python) → JSON Manifest → Rust Runtime
                         ↓
                RustPython VM executes Nodes
                         ↓
                 WASM host executes portable ops
   ```

### MVP Deliverables

- Rust runtime capable of loading serialized pipeline manifests.
- Embedded RustPython interpreter for Python node execution.
- Minimal WASM host (Wasmtime or Wasmer) integrated for safe, portable nodes.
- Unified logging and result serialization back to Python SDK.

This model provides immediate compatibility with the existing Python-based pipelines while paving the path toward full WASM-native portability.

---

## 14. System Flow & Use Case Diagrams

```mermaid
graph TD

subgraph A[Web Client]
  A1[AudioSource Node]
  A2[HFPipelineNode (speech_to_text)]
  A3[AudioSink]
  A1 --> A2 --> A3
end

subgraph B[Python SDK]
  B1[Pipeline Definition]
  B2[Serialization -> JSON Manifest]
  B3[Send to Rust Runtime]
  B1 --> B2 --> B3
end

subgraph C[Rust Runtime]
  C1[Load Manifest]
  C2[Execute Python Nodes via RustPython]
  C3[Execute Portable Nodes via WASM]
  C4[WebRTC Transport Manager]
  B3 --> C1 --> C2 --> C3
  C3 --> C4
end

subgraph D[Remote Execution]
  D1[RemoteExecutorConfig]
  D2[RemoteProxyClient]
  D3[Remote Node Instance]
  C4 --> D1 --> D2 --> D3
end

subgraph E[OCI / Registry]
  E1[Push .rmpkg]
  E2[Fetch by Reference]
  D3 --> E2
  B3 --> E1
end
```

### Example Use Cases & Corresponding Python Code

**1. Local Execution:**\
Python SDK defines and runs all nodes via RustPython → Rust Runtime.

```python
from remotemedia import Pipeline, AudioSource, HFPipelineNode, AudioSink

p = Pipeline('local_test')
p.add(AudioSource())
p.add(RemoteObjectExecutionNode(obj_to_execute=HFPipelineNode('text_to_speech')))
p.add(AudioSink())

p.run()
```

**2. Remote Execution:**\
Node requests remote capabilities through `RemoteExecutorConfig`. Runtime dispatches to a remote proxy client.

```python
from remotemedia import Pipeline, HFPipelineNode, AudioSource, AudioSink, RemoteExecutorConfig

remote = RemoteExecutorConfig(host='ai.remotehost.com', port=50052)

p = Pipeline('remote_test')
p.add(AudioSource())
p.add(RemoteObjectExecutionNode(obj_to_execute=HFPipelineNode('speech_to_text'), remote_config=remote))
p.add(HFPipelineNode('text_to_speech', remote_config=remote))
p.add(AudioSink())

p.run()
```

**3. Hybrid Pipeline:**\
Web client streams audio → server pipeline processes audio → GPU executor runs model → server streams audio response back.

```python
from remotemedia import Pipeline, AudioSource, AudioSink, HFPipelineNode, RemoteExecutorConfig

remote_gpu = RemoteExecutorConfig(host='gpu-server', port=50052)

p = Pipeline('hybrid_voice_assistant')
p.add(AudioSource(device='microphone'))
p.add(RemoteObjectExecutionNode(obj_to_execute=HFPipelineNode('speech_to_text')))  # Local execution
p.add(RemoteObjectExecutionNode(obj_to_execute=HFPipelineNode('language_model'), remote_config=remote_gpu))  # Remote GPU execution
p.add(RemoteObjectExecutionNode(obj_to_execute=HFPipelineNode('text_to_speech')))
p.add(AudioSink(device='speakers'))

p.run()
```

**4. OCI Package Retrieval:**\
Runtime detects a missing node → fetches `.rmpkg` from registry → executes locally or remotely depending on capabilities.

```python
from remotemedia import Pipeline, HFPipelineNode

p = Pipeline('registry_test')
p.add(RemoteObjectExecutionNode(obj_to_execute=HFPipelineNode('voice_tts'), remote_ref='oci://registry.ai/voice_tts:1.0'))
p.run()
```

**1. Local Execution:**\
Python SDK defines and runs all nodes via RustPython → Rust Runtime.

**2. Remote Execution:**\
Node requests remote capabilities through `RemoteExecutorConfig`. Runtime dispatches to a remote proxy client.

**3. Hybrid Pipeline:**\
Web client streams audio → server pipeline processes audio → GPU executor runs model → server streams audio response back.

**4. OCI Package Retrieval:**\
Runtime detects a missing node → fetches `.rmpkg` from registry → executes locally or remotely depending on capabilities.

---

## 15. Summary

RemoteMedia is:

> **An open, language-neutral runtime for distributed AI pipelines — with Python as the authoring language, Rust as the executor, WASM as the sandbox, WebRTC as the transport, and OCI as the distribution protocol.**

A developer writes normal Python code.\
They call `.run()`.\
Behind the scenes, pipelines connect, stream, and execute wherever they belong — local, remote, or edge — with zero configuration and full determinism.

