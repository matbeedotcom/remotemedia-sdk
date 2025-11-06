# RemoteMedia Runtime - System Architecture Diagram

## 1. High-Level System Overview

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          REMOTEMEDIA RUNTIME SYSTEM                              │
│                                                                                   │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │                         TRANSPORT LAYER                                    │  │
│  │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐                │  │
│  │  │ gRPC Server  │    │ WebRTC (opt) │    │  FFI (PyO3)  │                │  │
│  │  │ Port 50051   │    │              │    │  Python SDK  │                │  │
│  │  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘                │  │
│  └─────────┼───────────────────┼───────────────────┼────────────────────────┘  │
│            │                   │                   │                            │
│  ┌─────────▼───────────────────▼───────────────────▼────────────────────────┐  │
│  │                         SERVICE LAYER                                     │  │
│  │  ┌──────────────────────┐           ┌──────────────────────┐             │  │
│  │  │ StreamingService     │           │ ExecutionService     │             │  │
│  │  │ (Bidirectional RPC)  │           │ (Unary RPC)          │             │  │
│  │  │  - StreamPipeline    │           │  - ExecutePipeline   │             │  │
│  │  └──────────┬───────────┘           └──────────┬───────────┘             │  │
│  │             │                                   │                          │  │
│  │  ┌──────────▼───────────────────────────────────▼───────────┐             │  │
│  │  │         Session Management & Middleware                  │             │  │
│  │  │  - Auth (API tokens)                                     │             │  │
│  │  │  - Metrics (Prometheus)                                  │             │  │
│  │  │  - Resource Limits                                       │             │  │
│  │  │  - Version Negotiation                                   │             │  │
│  │  └──────────────────────────┬───────────────────────────────┘             │  │
│  └─────────────────────────────┼─────────────────────────────────────────────┘  │
│                                │                                                 │
│  ┌─────────────────────────────▼─────────────────────────────────────────────┐  │
│  │                      ROUTING & ORCHESTRATION LAYER                         │  │
│  │  ┌────────────────────────────────────────────────────────────────────┐   │  │
│  │  │                      SessionRouter                                  │   │  │
│  │  │  (Persistent per-session, central message broker)                  │   │  │
│  │  │                                                                      │   │  │
│  │  │  Input Stream ──► Main Loop ──► Node Tasks ──► Output Collection   │   │  │
│  │  │                      │              │                               │   │  │
│  │  │                      └──► Routing ──┘                               │   │  │
│  │  │                           Logic                                     │   │  │
│  │  └──────────────────────────┬───────────────────────────────────────┬─┘   │  │
│  │                              │                                       │     │  │
│  │  ┌───────────────────────────▼────────────┐    ┌──────────────────▼───┐ │  │
│  │  │      ExecutorRegistry                  │    │   StreamingNode      │ │  │
│  │  │  - Pattern matching (Whisper.*Node)    │    │   Registry           │ │  │
│  │  │  - Explicit mappings (FastResampleNode)│    │   (Factory pattern)  │ │  │
│  │  │  - Default executor selection          │    │                      │ │  │
│  │  └────────────────────────────────────────┘    └──────────────────────┘ │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │                         EXECUTION LAYER                                 │  │
│  │                                                                          │  │
│  │  ┌────────────────────────┐        ┌─────────────────────────────────┐ │  │
│  │  │  Native Rust Executor  │        │  Multiprocess Python Executor   │ │  │
│  │  │                        │        │                                 │ │  │
│  │  │  • In-process          │        │  • Process isolation            │ │  │
│  │  │  • Fast path (2-16x)   │        │  • iceoryx2 zero-copy IPC      │ │  │
│  │  │  • Audio nodes         │        │  • Dedicated IPC threads        │ │  │
│  │  │  • Zero-copy numpy     │        │  • Global session storage       │ │  │
│  │  └────────┬───────────────┘        └────────┬────────────────────────┘ │  │
│  │           │                                  │                          │  │
│  │  ┌────────▼──────────────────────────────────▼────────────────────────┐ │  │
│  │  │               CPYTHON DEPRECATED EXECUTOR                           │ │  │
│  │  │  (Legacy in-process Python, replaced by multiprocess)               │ │  │
│  │  └─────────────────────────────────────────────────────────────────────┘ │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │                          NODE LAYER                                     │  │
│  │                                                                          │  │
│  │  RUST NATIVE NODES              PYTHON MULTIPROCESS NODES               │  │
│  │  ┌──────────────────┐            ┌────────────────────────────┐        │  │
│  │  │ FastResampleNode │            │ WhisperNode (ASR)          │        │  │
│  │  │ SileroVADNode    │            │ LFM2Node (Audio model)     │        │  │
│  │  │ AudioChunkerNode │            │ VibeVoiceNode (TTS)        │        │  │
│  │  │ BufferAccumulator│            │ HF* (Hugging Face models)  │        │  │
│  │  └──────────────────┘            └────────────────────────────┘        │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │                      INFRASTRUCTURE LAYER                               │  │
│  │                                                                          │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │  │
│  │  │  Manifest    │  │  RuntimeData │  │   Metrics    │  │   Error    │ │  │
│  │  │  Parser      │  │  (Audio/Text)│  │  (29μs)      │  │  Handling  │ │  │
│  │  │  (YAML/JSON) │  │              │  │              │  │  + Retry   │ │  │
│  │  └──────────────┘  └──────────────┘  └──────────────┘  └────────────┘ │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## 2. Detailed gRPC Streaming Data Flow

```
┌───────────────────────────────────────────────────────────────────────────────┐
│                        STREAMING PIPELINE EXECUTION                            │
└───────────────────────────────────────────────────────────────────────────────┘

CLIENT                    TRANSPORT              SERVICE              ROUTER
  │                          │                      │                    │
  │  StreamPipeline()        │                      │                    │
  ├─────────────────────────►│                      │                    │
  │                          │  Parse manifest      │                    │
  │                          ├─────────────────────►│                    │
  │                          │                      │                    │
  │                          │  Create session_id   │                    │
  │                          │  Create SessionRouter│                    │
  │                          ├──────────────────────┼───────────────────►│
  │                          │                      │                    │
  │                          │                      │  Initialize nodes  │
  │                          │                      │  Start node tasks  │
  │                          │                      │  ◄─────────────────┤
  │                          │                      │                    │
  │  ◄────── READY ──────────┤◄─────────────────────┤                    │
  │                          │                      │                    │
  │                          │                      │                    │
  │  Audio Chunk #1          │                      │                    │
  ├─────────────────────────►│  send_input()        │                    │
  │                          ├──────────────────────┼───────────────────►│
  │                          │                      │                    │
  │                          │                      │  Route to node_1   │
  │                          │                      │  ────────────────► │
  │                          │                      │                  Node Task 1
  │                          │                      │                    │
  │                          │                      │  ◄──── output ──── │
  │                          │                      │                    │
  │  ◄─── Result Chunk ──────┤◄─────────────────────┤◄───────────────────┤
  │                          │                      │                    │
  │  Audio Chunk #2          │                      │                    │
  ├─────────────────────────►│                      │                    │
  │         ...              │        ...           │        ...         │
  │                          │                      │                    │
  │  Close stream            │                      │                    │
  ├─────────────────────────►│  shutdown_signal     │                    │
  │                          ├──────────────────────┼───────────────────►│
  │                          │                      │                    │
  │                          │                      │  Cleanup nodes     │
  │                          │                      │  Terminate session │
  │                          │                      │  ◄─────────────────┤
  │                          │                      │                    │
  │  ◄────── Done ───────────┤                      │                    │
  └──────────────────────────┴──────────────────────┴────────────────────┘
```

## 3. SessionRouter Internal Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         SESSION ROUTER (Per-Session)                         │
│                                                                               │
│  Lifecycle: Created at stream start, destroyed at stream end                 │
│  Purpose: Central message broker for all nodes in this session               │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  INPUT CHANNELS                                                               │
│  ┌──────────────────┐         ┌──────────────────┐                          │
│  │  input_rx        │         │  shutdown_rx     │                          │
│  │  (from client)   │         │  (control)       │                          │
│  └────────┬─────────┘         └────────┬─────────┘                          │
│           │                            │                                     │
│           └───────────┬────────────────┘                                     │
│                       │                                                      │
│                       ▼                                                      │
│  ┌─────────────────────────────────────────────────────────────────┐        │
│  │                    MAIN LOOP (async task)                        │        │
│  │                                                                  │        │
│  │  select! {                                                       │        │
│  │      packet = input_rx.recv() => {                              │        │
│  │          // Route packet to appropriate node                    │        │
│  │          if packet.to_node.is_some() {                          │        │
│  │              node_inputs[to_node].send(packet)                  │        │
│  │          } else {                                                │        │
│  │              // Send to first node in pipeline                  │        │
│  │              node_inputs[first_node].send(packet)               │        │
│  │          }                                                       │        │
│  │      }                                                           │        │
│  │      _ = shutdown_rx.recv() => break                            │        │
│  │  }                                                               │        │
│  └─────────────────────────────────────────────────────────────────┘        │
│                       │                                                      │
│                       │  Routes to...                                        │
│                       │                                                      │
│  ┌────────────────────┴─────────────────────────────────────────┐           │
│  │                    NODE TASKS                                 │           │
│  │                                                                │           │
│  │  ┌──────────────────┐    ┌──────────────────┐    ┌─────────┐│           │
│  │  │ Node Task 1      │    │ Node Task 2      │    │ Node N  ││           │
│  │  │                  │    │                  │    │         ││           │
│  │  │ input_rx ────►   │    │ input_rx ────►   │    │ input_rx││           │
│  │  │   ↓              │    │   ↓              │    │   ↓     ││           │
│  │  │ process()        │    │ process()        │    │ process ││           │
│  │  │   ↓              │    │   ↓              │    │   ↓     ││           │
│  │  │ router_tx ────►  │    │ router_tx ────►  │    │ router_ ││           │
│  │  └──────────────────┘    └──────────────────┘    └─────────┘│           │
│  │           │                       │                    │     │           │
│  └───────────┼───────────────────────┼────────────────────┼─────┘           │
│              │                       │                    │                 │
│              └───────────┬───────────┴────────────────────┘                 │
│                          │                                                  │
│                          ▼                                                  │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │                  OUTPUT COLLECTION                               │       │
│  │                                                                  │       │
│  │  router_tx.recv() => {                                           │       │
│  │      match packet.from_node {                                    │       │
│  │          "final_node" => client_tx.send(packet.data)            │       │
│  │          _ => {                                                  │       │
│  │              // Route to next node based on manifest             │       │
│  │              let next = manifest.get_next_node(from_node)        │       │
│  │              node_inputs[next].send(packet)                      │       │
│  │          }                                                        │       │
│  │      }                                                            │       │
│  │  }                                                                │       │
│  └────────────────────────────┬─────────────────────────────────────┘       │
│                               │                                             │
│                               ▼                                             │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │                     OUTPUT CHANNEL                               │       │
│  │  ┌────────────────┐                                              │       │
│  │  │  client_tx     │  (sends results back to gRPC client)         │       │
│  │  └────────────────┘                                              │       │
│  └─────────────────────────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────────────────────────┘

KEY CHARACTERISTICS:
  • Single router instance per session (persistent throughout stream)
  • Each node runs in independent tokio task with dedicated input channel
  • All outputs flow back through shared router_tx channel
  • Pipelined execution: nodes process asynchronously, don't block
  • Router handles both inter-node routing AND client output
```

## 4. Multiprocess Python Execution Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              MULTIPROCESS PYTHON NODE EXECUTION (spec 002)                   │
│                                                                               │
│  RUST RUNTIME (async tokio)           IPC LAYER              PYTHON PROCESS │
└─────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────┐
│  SessionRouter           │
│  (session_id: abc123)    │
└────────┬─────────────────┘
         │
         │ send_data_to_node(node_id, session_id, data)
         │
         ▼
┌────────────────────────────────────────────────────────────────────────────┐
│  GLOBAL_SESSIONS                                                            │
│  OnceLock<Arc<RwLock<HashMap<String, HashMap<String, Sender<IpcCommand>>>>> │
│                                                                             │
│  session_id "abc123" ──► {                                                 │
│      "lfm2_audio" ──► mpsc::Sender<IpcCommand>  ─────┐                    │
│      "whisper"    ──► mpsc::Sender<IpcCommand>       │                    │
│  }                                                    │                    │
└───────────────────────────────────────────────────────┼────────────────────┘
                                                        │
        ┌───────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  IPC THREAD (dedicated OS thread, one per node)                             │
│  Thread: "ipc-lfm2_audio"                                                   │
│                                                                              │
│  cmd_rx.try_recv() ◄────┐                                                   │
│       │                  │                                                   │
│       ▼                  │ Commands:                                        │
│  match command {         │  - SendData { data }                             │
│      SendData => {       │  - RegisterOutputCallback { callback_tx }        │
│          publisher       │  - Shutdown                                      │
│            .publish(data)│                                                   │
│      }                   │                                                   │
│  }                       │                                                   │
│       │                  │                                                   │
│       ▼                  │                                                   │
│  ┌──────────────────────────────────────────────────────────────┐           │
│  │  PERSISTENT PUBLISHERS/SUBSCRIBERS (!Send types)             │           │
│  │                                                               │           │
│  │  Publisher<IPCRuntimeData>   ────────────────────────────►   │           │
│  │     (abc123_lfm2_audio_input)                                │           │
│  │                                                               │           │
│  │                                    iceoryx2 shared memory     │           │
│  │                                    (zero-copy transfer)       │           │
│  │                                                               │           │
│  │  Subscriber<IPCRuntimeData>  ◄────────────────────────────   │           │
│  │     (abc123_lfm2_audio_output)                               │           │
│  └──────────────────────────────────────────────────────────────┘           │
│       │                                                                      │
│       │ subscriber.receive()                                                │
│       │ (polling with std::thread::yield_now())                             │
│       │                                                                      │
│       ▼                                                                      │
│  if let Some(output) = subscriber.receive() {                               │
│      // Send via callback for continuous forwarding                         │
│      output_callback_tx.send(output)                                        │
│  } else {                                                                    │
│      std::thread::yield_now()  // NOT sleep(1ms)!                           │
│  }                                                                           │
│       │                                                                      │
└───────┼──────────────────────────────────────────────────────────────────────┘
        │
        │ output_callback_tx
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  BACKGROUND DRAINING TASK (tokio task)                                      │
│  Registered via register_output_callback()                                  │
│                                                                              │
│  while let Some(ipc_output) = output_rx.recv().await {                      │
│      let runtime_data = from_ipc_runtime_data(ipc_output)?;                │
│      let packet = DataPacket {                                              │
│          data: runtime_data,                                                │
│          from_node: "lfm2_audio",                                           │
│          session_id: "abc123",                                              │
│          ...                                                                 │
│      };                                                                      │
│      router_tx.send(packet)?;  // Back to SessionRouter                    │
│  }                                                                           │
└────────────────────────────────────────────────────────────────────────────┬┘
                                                                              │
                                                                              │
        ┌─────────────────────────────────────────────────────────────────────┘
        │
        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│  PYTHON PROCESS (node.py)                                                   │
│  PID: 12345, Node: lfm2_audio                                               │
│                                                                              │
│  async def _process_loop(self):                                             │
│      while not self.should_stop:                                            │
│          data = await self._receive_input()  # iceoryx2 subscriber         │
│          if data is None:                                                   │
│              await asyncio.sleep(0)  # NOT sleep(0.01)!                     │
│              continue                                                        │
│                                                                              │
│          # Process data (may yield multiple outputs)                        │
│          result = self.process(data)                                        │
│          if inspect.isasyncgen(result):                                     │
│              async for output in result:                                    │
│                  await self._send_output(output)  # iceoryx2 publisher     │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘

                         ▲                              │
                         │                              │
                         │ Input channel                │ Output channel
                         │ (abc123_lfm2_audio_input)    │ (abc123_lfm2_audio_output)
                         │                              │
                         │      iceoryx2 IPC            ▼
                         │      (zero-copy shared memory)
                         │
                         └──────────────────────────────┘
```

## 5. Executor Registry & Node Type Routing

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         EXECUTOR REGISTRY                                    │
│  Purpose: Maps node types to executors (Native Rust vs Multiprocess Python) │
└─────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│  REGISTRATION STRATEGIES                                                      │
│                                                                               │
│  1. EXPLICIT MAPPINGS (highest priority)                                     │
│     ┌─────────────────────────┬────────────────────┐                        │
│     │ Node Type               │ Executor           │                        │
│     ├─────────────────────────┼────────────────────┤                        │
│     │ FastResampleNode        │ Native             │                        │
│     │ SileroVADNode           │ Native             │                        │
│     │ AudioChunkerNode        │ Native             │                        │
│     │ AudioBufferAccumulator  │ Native             │                        │
│     └─────────────────────────┴────────────────────┘                        │
│                                                                               │
│  2. PATTERN RULES (regex matching)                                           │
│     ┌─────────────────────────┬────────────────────┬──────────┐             │
│     │ Pattern                 │ Executor           │ Priority │             │
│     ├─────────────────────────┼────────────────────┼──────────┤             │
│     │ ^(Whisper|LFM2|...)Node$│ Multiprocess       │ 100      │             │
│     └─────────────────────────┴────────────────────┴──────────┘             │
│                                                                               │
│  3. DEFAULT EXECUTOR (fallback)                                              │
│     Native                                                                   │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────────────┐
│  RESOLUTION FLOW                                                              │
│                                                                               │
│  manifest.nodes[0].node_type = "WhisperNode"                                 │
│           │                                                                   │
│           ▼                                                                   │
│  ExecutorRegistry.select("WhisperNode")                                      │
│           │                                                                   │
│           ├──► Check explicit mappings?     NO                               │
│           │                                                                   │
│           ├──► Check pattern rules?         YES ✓                            │
│           │     Pattern: ^(Whisper|...)Node$                                 │
│           │     Match: "WhisperNode"                                         │
│           │                                                                   │
│           ▼                                                                   │
│  ExecutorType::Multiprocess                                                  │
│           │                                                                   │
│           ▼                                                                   │
│  MultiprocessExecutor::initialize()                                          │
│           │                                                                   │
│           ├──► Spawn Python process                                          │
│           ├──► Create iceoryx2 channels                                      │
│           ├──► Spawn IPC thread                                              │
│           └──► Register in GLOBAL_SESSIONS                                   │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

## 6. Data Type Flow & Serialization

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         DATA TYPE CONVERSIONS                                │
└─────────────────────────────────────────────────────────────────────────────┘

CLIENT                  PROTOBUF              RUST               IPC           PYTHON
  │                        │                    │                 │              │
  │  AudioBuffer          │                    │                 │              │
  │  {                     │                    │                 │              │
  │    samples: [f32]     │                    │                 │              │
  │    sample_rate: u32   │                    │                 │              │
  │    channels: u32      │                    │                 │              │
  │  }                     │                    │                 │              │
  ├───────────────────────►│                    │                 │              │
  │                        │                    │                 │              │
  │                        │  Convert to        │                 │              │
  │                        │  RuntimeData::Audio│                 │              │
  │                        ├───────────────────►│                 │              │
  │                        │                    │                 │              │
  │                        │                    │  Serialize to   │              │
  │                        │                    │  IPCRuntimeData │              │
  │                        │                    ├────────────────►│              │
  │                        │                    │                 │              │
  │                        │                    │  Binary format: │              │
  │                        │                    │  ┌──────────────┤              │
  │                        │                    │  │type (1 byte) │              │
  │                        │                    │  │session_len   │              │
  │                        │                    │  │  (2 bytes)   │              │
  │                        │                    │  │session_id    │              │
  │                        │                    │  │  (N bytes)   │              │
  │                        │                    │  │timestamp     │              │
  │                        │                    │  │  (8 bytes)   │              │
  │                        │                    │  │payload_len   │              │
  │                        │                    │  │  (4 bytes)   │              │
  │                        │                    │  │payload       │              │
  │                        │                    │  │  (f32 array) │              │
  │                        │                    │  └──────────────┤              │
  │                        │                    │                 │              │
  │                        │                    │  iceoryx2       │              │
  │                        │                    │  zero-copy      │              │
  │                        │                    │  shared memory  │              │
  │                        │                    ├─────────────────┼─────────────►│
  │                        │                    │                 │              │
  │                        │                    │                 │  Deserialize │
  │                        │                    │                 │  to          │
  │                        │                    │                 │  RuntimeData │
  │                        │                    │                 │              │
  │                        │                    │                 │  Process     │
  │                        │                    │                 │              ▼
  │                        │                    │                 │            Model
  │                        │                    │                 │            (LFM2)
  │                        │                    │                 │              │
  │                        │                    │                 │  yield       │
  │                        │                    │                 │  outputs     │
  │                        │                    │                 │              │
  │                        │                    │  ◄──────────────┼──────────────┤
  │                        │                    │                 │              │
  │                        │  ◄─────────────────┤                 │              │
  │                        │                    │                 │              │
  │  ◄─────────────────────┤                    │                 │              │
  │                        │                    │                 │              │
  └────────────────────────┴────────────────────┴─────────────────┴──────────────┘

KEY DATA TYPES:

1. RuntimeData (Rust enum)
   - Audio { samples: Vec<f32>, sample_rate: u32, channels: u32 }
   - Text(String)
   - Image { data: Vec<u8>, width: u32, height: u32 }
   - Binary(Vec<u8>)

2. IPCRuntimeData (Binary serialization for iceoryx2)
   - Custom format for zero-copy shared memory
   - Type-tagged with session_id for routing

3. AudioBuffer (Protobuf for gRPC)
   - Generated from protos/common.proto
   - Used for client ↔ server communication
```

## 7. Session Lifecycle & Resource Management

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         SESSION LIFECYCLE                                    │
└─────────────────────────────────────────────────────────────────────────────┘

TIME    CLIENT              SERVICE                ROUTER           EXECUTORS
  │       │                    │                      │                 │
  │       │  StreamPipeline()  │                      │                 │
  │       ├───────────────────►│                      │                 │
  │       │                    │                      │                 │
  ▼       │                    │  session_id = UUID   │                 │
INIT      │                    │  Create SessionRouter│                 │
  │       │                    ├─────────────────────►│                 │
  │       │                    │                      │                 │
  │       │                    │                      │  Initialize     │
  │       │                    │                      │  all nodes      │
  │       │                    │                      ├────────────────►│
  │       │                    │                      │                 │
  │       │                    │                      │  Spawn Python   │
  │       │                    │                      │  processes      │
  │       │                    │                      │                 ▼
  │       │                    │                      │           ProcessManager
  │       │                    │                      │                 │
  │       │                    │                      │  Create IPC     │
  │       │                    │                      │  threads        │
  │       │                    │                      │                 │
  │       │                    │                      │  Register in    │
  │       │                    │                      │  GLOBAL_SESSIONS│
  │       │                    │                      │                 │
  │       │  ◄─── READY ───────┤◄─────────────────────┤                 │
  │       │                    │                      │                 │
  ▼       │                    │                      │                 │
ACTIVE    │  Stream chunks     │                      │                 │
  │       ├───────────────────►│  Route & process     │                 │
  │       │                    ├─────────────────────►│  Forward to     │
  │       │                    │                      │  nodes          │
  │       │                    │                      ├────────────────►│
  │       │  ◄─── Results ─────┤◄─────────────────────┤◄────────────────┤
  │       │                    │                      │                 │
  │       │  (continues for    │                      │                 │
  │       │   duration of      │                      │                 │
  │       │   stream)          │                      │                 │
  │       │                    │                      │                 │
  ▼       │                    │                      │                 │
SHUTDOWN  │  Close stream      │                      │                 │
  │       ├───────────────────►│  shutdown_signal     │                 │
  │       │                    ├─────────────────────►│                 │
  │       │                    │                      │                 │
  │       │                    │                      │  Send Shutdown  │
  │       │                    │                      │  to IPC threads │
  │       │                    │                      ├────────────────►│
  │       │                    │                      │                 │
  │       │                    │                      │  Terminate      │
  │       │                    │                      │  Python         │
  │       │                    │                      │  processes      │
  │       │                    │                      │                 │
  │       │                    │                      │  Remove from    │
  │       │                    │                      │  GLOBAL_SESSIONS│
  │       │                    │                      │                 │
  │       │  ◄─── Done ────────┤                      │                 │
  │       │                    │                      │                 │
  ▼       │                    │                      │                 │
END       └────────────────────┴──────────────────────┴─────────────────┘

KEY POINTS:
  • Session created per gRPC stream (bidirectional)
  • Session ID used to prefix all iceoryx2 channels
  • Router persists throughout session (not per-request)
  • Cleanup is CRITICAL: must terminate processes and remove from global state
  • Session can be terminated by: client close, error, or explicit terminate_session()
```

## 8. Component Directory Reference

```
runtime/src/
├── grpc_service/
│   ├── server.rs              ──► Tonic server setup, middleware, ExecutorRegistry init
│   ├── streaming.rs           ──► Bidirectional streaming RPC handler
│   ├── execution.rs           ──► Unary RPC handler (single request/response)
│   ├── session_router.rs      ──► [CORE] Persistent session routing
│   ├── executor_registry.rs   ──► Node type → Executor mapping (pattern/explicit)
│   ├── auth.rs                ──► API token validation
│   ├── metrics.rs             ──► Prometheus metrics collection
│   └── generated/             ──► Protobuf-generated types (AudioBuffer, etc.)
│
├── python/multiprocess/
│   ├── multiprocess_executor.rs  ──► [CORE] IPC thread arch, global sessions
│   ├── process_manager.rs        ──► Python process spawn/lifecycle
│   ├── ipc_channel.rs            ──► iceoryx2 channel registry
│   ├── data_transfer.rs          ──► Binary IPC serialization format
│   └── health_monitor.rs         ──► Process health checks, crash detection
│
├── executor/
│   ├── scheduler.rs           ──► Topological sort, execution order
│   ├── graph.rs               ──► Pipeline DAG construction
│   ├── retry.rs               ──► Exponential backoff, circuit breaker
│   ├── metrics.rs             ──► Performance tracking (29μs overhead)
│   └── runtime_selector.rs    ──► Rust vs Python runtime selection
│
├── nodes/
│   ├── audio/                 ──► Native Rust audio nodes (2-16x faster)
│   │   ├── resample.rs        ──► High-quality resampling (rubato)
│   │   ├── vad.rs             ──► Silero VAD (ONNX Runtime)
│   │   └── format.rs          ──► Zero-copy format conversion
│   ├── python_streaming.rs    ──► Python node wrapper for streaming
│   └── registry.rs            ──► NodeFactory pattern, CompositeRegistry
│
├── data/
│   ├── runtime_data.rs        ──► Core data types (Audio, Text, Image)
│   └── conversion.rs          ──► Protobuf ↔ RuntimeData conversion
│
└── manifest/
    ├── manifest.rs            ──► Pipeline definition (YAML/JSON)
    └── validation.rs          ──► Schema validation, connection checks

python-client/remotemedia/core/multiprocessing/
├── node.py                    ──► Python node base class, IPC receive/send
├── runner.py                  ──► Entry point for Python processes
└── session.py                 ──► Session management (deprecated)
```

## 9. Performance Characteristics

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         PERFORMANCE PROFILE                                  │
└─────────────────────────────────────────────────────────────────────────────┘

COMPONENT                      LATENCY           THROUGHPUT         NOTES
─────────────────────────────────────────────────────────────────────────────
gRPC Transport                 <5ms              1000+ conns/sec    HTTP/2
SessionRouter                  <100μs            10K msg/sec        Lock-free
Native Rust Nodes              0.4-2ms           2-16x vs Python    In-process
Multiprocess Python            5-20ms/node       Varies by model    IPC overhead
iceoryx2 IPC Transfer          <10μs             Zero-copy          Shared memory
IPC Thread Polling (optimized) <100μs            Yield-based        NOT sleep(1ms)
Python Processing Loop         <100μs            Yield-based        NOT sleep(10ms)
Metrics Collection             29μs              71% under target   Optional
FFI (PyO3) Overhead            <1μs              Zero-copy          rust-numpy

BOTTLENECKS ELIMINATED (Dec 2024):
  ❌ IPC thread sleep(1ms)     → ✅ yield_now()      (~1ms saved per poll)
  ❌ Python loop sleep(10ms)   → ✅ sleep(0)         (~10ms saved per poll)
  ❌ Blocking node processing  → ✅ Pipelined tasks  (concurrent execution)
  ❌ Per-send publisher create → ✅ Persistent pubs  (eliminated 50ms delay)

CURRENT BOTTLENECKS:
  • Python model inference time (dominates pipeline)
  • Model initialization (10-30s for LFM2, Whisper)
  • IPC serialization/deserialization (minor)
```

---

This system diagram provides a comprehensive view of the RemoteMedia Runtime architecture, showing the relationships between components, data flow paths, and key implementation details.
