# Session Control Bus — Developer Guide

**Status:** Prototype. Rust core is wired into `SessionRouter` with integration tests;
Python client surface is specified here but not yet implemented.

**Audience:** Developers building real-time pipelines with RemoteMedia who need to
observe, inject, or edit data flowing through a *running* session from a client —
without redefining the pipeline manifest.

## Scope and non-goals

The bus is an **in-process tokio-native control plane** for a running
`PipelineExecutor` / `SessionRouter`. It is appropriate wherever the pipeline
runs inside a regular user-session process that can host tokio and (eventually)
a gRPC/WebSocket control-plane transport:

- Router process ↔ GUI / operator UI / Python client
- Router process ↔ third-party observer (debug tooling, monitors)
- Router process ↔ itself (in-process handlers, test harnesses)

**The bus is NOT a fit for:**

- **HAL / coreaudiod plugins or any audio-server-hosted code.** These run under
  sandbox profiles that reject tokio runtimes, iceoryx2, sockets, and shared
  filesystem primitives. The HAL↔router boundary uses the mmap control file
  (bitmask / shared-state region) — not this bus.
- **Cross-machine fan-out independent of a session.** The bus is scoped to a
  single `session_id` on one server. A cross-fleet control plane is a
  different system.
- **Low-latency per-sample dataplane control.** The data plane (audio frames,
  tokens) flows through node connections. The bus is for session-level
  coordination (attach, publish context, intercept an output), not per-sample
  edits.

Architecture rule: **HAL↔GUI traffic always traverses the router.** The router
translates Session Control Bus semantics into HAL-side mmap writes (and vice
versa for telemetry). Keeping the bus tokio-only is deliberate; forcing it
through the HAL boundary would compromise both.

---

## 1. Motivation

A RemoteMedia pipeline is a static graph of nodes declared in YAML:

```yaml
nodes:
  - id: vad
  - id: whisper
  - id: llm
  - id: tts
connections:
  - { from: vad,     to: whisper }
  - { from: whisper, to: llm }
  - { from: llm,     to: tts }
```

This is fine for fixed dataflows. It breaks down the moment a developer wants to:

- **Inject auxiliary data mid-session** — e.g. RAG docs into the LLM's context
  after each transcript arrives.
- **Observe intermediate outputs** — e.g. stream Whisper transcripts to a debug
  UI without adding a sink node.
- **Edit data in flight** — e.g. scrub PII from transcripts before they reach
  the LLM.
- **Respond to tool calls from the client** — e.g. LLM emits `<tool>…</tool>`,
  the client executes it locally and feeds the result back.

Before: each of these required editing the manifest, restarting the session,
and wiring new sink/source nodes. The pipeline became a programming target
disguised as a config file.

**The Session Control Bus is the programmatic escape hatch.** The manifest
stays as the declarative skeleton. The bus lets a client reach into an
already-running session and:

1. **Subscribe** to any node's output (a **tap**).
2. **Publish** data to any node's input (an **inject**).
3. **Intercept** a node's output and replace/drop it before downstream fan-out.

All three are addressed by **session**, then node.

---

## 2. Core concepts

### 2.1 Addressing

```
session_id / node_id [ . port ] / direction
    │           │           │            │
  primary    topic      optional      "in"  = publish target (node input)
   key      segment     (default      "out" = subscribe/intercept target
                         "main")             (node output)
```

- **Session is the primary dimension.** Every operation happens inside a
  `session_id`. A client attaches to a session; within that attach, it
  addresses nodes freely. Sessions are created by `pipeline.stream(...)`;
  the bus never spawns its own.
- **Node is the topic segment.** Identifiers come straight from the manifest.
- **Port is an optional qualifier.** `None` means the node's `main` rail.
  Declared auxiliary ports (e.g. `llm.in.context`) are a later extension.
- **Direction is explicit.** `In` = data flowing *into* the node from the
  client; `Out` = data flowing *out* of the node toward the client.

### 2.2 Four operations, one frame protocol

| Operation        | Address                    | What it does                                                     |
|------------------|----------------------------|------------------------------------------------------------------|
| `subscribe`      | `.out`                     | Broadcast of a node's outputs to a client receiver. Non-blocking. |
| `publish`        | `.in`                      | Injects a `DataPacket` into the router's input channel with `to_node = node_id`. |
| `intercept`      | `.out`                     | Splices a correlated oneshot between the node's output and its downstream fan-out. Client replies with `Pass` / `Replace(data)` / `Drop`, bounded by a deadline. |
| `set_node_state` | node_id (not an address)   | Flip a node's runtime execution state: `Enabled` / `Bypass` / `Disabled`. Applies on the next packet. |

All four are expressed as `ControlFrame` variants over a single bi-directional
stream between the client and server. Transport framing (gRPC / WebSocket) is
the job of the transport crates; the core just deals in frames.

### 2.3 Node runtime state

Every node has a runtime state driven by the bus, independent of its manifest
declaration:

- **`Enabled`** (default): node runs normally.
- **`Bypass`**: node is skipped; its inputs are forwarded as its outputs.
  Downstream nodes see data as if the bypassed node were a passthrough.
  Tap subscribers still receive the forwarded data via the standard
  `on_node_output` hook, so observers see exactly what downstream sees.
- **`Disabled`**: node is skipped and produces no outputs. Downstream
  nodes receive nothing from this branch for the current input. Useful
  for temporarily severing a subgraph without editing the manifest.

Absent an explicit override, a node is `Enabled`. State changes take effect
on the **next** packet the router processes — this is a best-effort toggle,
not a barrier on in-flight packets.

### 2.4 Lifecycle rules

The session is authoritative. The control bus is subordinate:

| Event                                     | Effect on session | Effect on attach                                      |
|-------------------------------------------|-------------------|-------------------------------------------------------|
| Control attach opens                      | None              | Attach succeeds iff session exists                    |
| Control client drops attach               | None              | Session continues running                             |
| Session closes (normal)                   | —                 | All attaches receive `CloseReason::Normal`; streams end |
| Session closes (error)                    | —                 | All attaches receive `CloseReason::Error(...)`         |
| Attach arrives after session terminated   | None              | `SessionNotFoundError` returned immediately            |

**Consequences:**
- An attach cannot keep a session alive.
- A session's shutdown cleanly tears down every attach.
- Third-party observers (debug UIs, monitors) can attach to any existing
  session without being a pipeline participant.

---

## 3. Architecture

```
              ┌────────────────────────────────────────────────┐
              │          SessionRouter (per session)           │
              │                                                │
  client ─────┼─► input_tx ───┐                                │
  (main data) │                │                               │
              │                ▼                                │
              │           ┌──────────┐                          │
              │           │  Node A  │──outputs──┐              │
              │           └──────────┘            │              │
              │                                   ▼              │
              │                      ┌──────────────────────┐    │
  SessionControl ───────── attaches ─┤ on_node_output hook  │    │
    │                                │  • tap fan-out       │    │
    │                                │  • intercept splice  │    │
    │                                └──────────┬───────────┘    │
    │                                           │                 │
    ├─► Subscribe (tap) ─────────────────── broadcast::Sender     │
    │                                                              │
    ├─► Publish  (inject) ── DataPacket ──────► input_tx          │
    │                        { to_node: N }                       │
    │                                                              │
    └─► Intercept ── oneshot<InterceptDecision> ── on_node_output │
                                                                   │
              ┌────────────────────────────────────────────────┐
              │     SessionControlBus (process-wide)           │
              │     DashMap< session_id, Arc<SessionControl> > │
              └────────────────────────────────────────────────┘
```

Key properties:

- The **bus is a thin registry.** It holds `Arc<SessionControl>` keyed by
  `session_id`. The transport layer looks up a session here when a client
  sends its `Attach(session_id)` frame. The bus does not own the session.
- The **router's hot path is unchanged when no control is attached.**
  `Option<Arc<SessionControl>>` is checked once per node output; `None`
  short-circuits the whole hook.
- **Tap fan-out uses `tokio::sync::broadcast`.** Slow subscribers lag, never
  block the pipeline. `Lagged` is surfaced to the client as a warning, not
  a fatal error.
- **Intercept uses a deadline.** If the client doesn't reply within the
  deadline (default 50 ms), the router forwards the original frame
  unchanged and logs a warning. A wedged client cannot stall the pipeline.
- **`publish` reuses the router's existing input channel.** The bus
  constructs a `DataPacket` with `to_node = addr.node_id` and pushes it
  into `input_tx`. No new ingestion path.

---

## 4. Rust API (working today)

All code below is exercised by
`crates/core/tests/session_control_integration.rs` and the module-level
unit tests in `crates/core/src/transport/session_control.rs`.

### 4.0 Wire-level frames (`ControlFrame`)

All client→server traffic is one of these variants. Transport crates
wrap them in gRPC/WebSocket framing; the core deals only in the logical
frames below.

```rust
pub enum ControlFrame {
    Subscribe(ControlAddress),
    Unsubscribe(ControlAddress),
    Publish       { addr: ControlAddress, data: RuntimeData },
    Intercept     { addr: ControlAddress, deadline: Option<Duration> },
    RemoveIntercept(ControlAddress),
    InterceptReply { correlation_id: u64, decision: InterceptDecision },
    SetNodeState   { node_id: String,     state: NodeState },
    ClearNodeState { node_id: String },
}
```

Server→client events are `ControlEvent`:

```rust
pub enum ControlEvent {
    Tap              { addr: ControlAddress, data: RuntimeData },
    InterceptRequest { addr: ControlAddress, correlation_id: u64, data: RuntimeData },
    Error            { addr: Option<ControlAddress>, message: String },
}
```

Every inbound frame is dispatched through `SessionControl::handle_frame`,
which returns a `FrameOutcome` (either `Done`, a tap `Receiver`, or an
intercept event stream). `SetNodeState` / `ClearNodeState` return
`FrameOutcome::Done` — they're fire-and-forget state mutations.

### 4.1 Construct a router with an attached control bus

```rust
use std::sync::Arc;
use remotemedia_core::manifest::Manifest;
use remotemedia_core::nodes::streaming_registry::create_default_streaming_registry;
use remotemedia_core::transport::session_control::SessionControl;
use remotemedia_core::transport::session_router::{
    SessionRouter, DEFAULT_ROUTER_OUTPUT_CAPACITY,
};
use tokio::sync::mpsc;

let manifest = Arc::new(load_manifest_from_yaml("voice-assistant.yaml")?);
let registry = Arc::new(create_default_streaming_registry());
let (output_tx, mut output_rx) = mpsc::channel(DEFAULT_ROUTER_OUTPUT_CAPACITY);

let session_id = "sess-1".to_string();
let (mut router, shutdown_tx) =
    SessionRouter::new(session_id.clone(), manifest, registry, output_tx)?;

let ctrl = SessionControl::new(session_id.clone());
router.attach_control(ctrl.clone()).await;

// Register in the process-wide bus so a transport can find this session.
let bus = my_executor.control_bus(); // Arc<SessionControlBus>
bus.register(ctrl.clone());

let input_tx = router.get_input_sender();
let handle = router.start();
```

**Ordering matters:**
- `attach_control` must be called **before** `start()`. After `start()`,
  the router has consumed its `input_tx` and the bus has nothing to
  clone for `publish`.
- `subscribe` / `intercept` may be called **before or after** `start()`.
  Subscribing before the first output guarantees you don't miss it.

### 4.2 Tap — observe a node's output

```rust
use remotemedia_core::transport::session_control::ControlAddress;
use remotemedia_core::data::RuntimeData;

let mut tap = ctrl.subscribe(&ControlAddress::node_out("whisper"))?;

tokio::spawn(async move {
    while let Ok(data) = tap.recv().await {
        if let RuntimeData::Text(t) = data {
            println!("transcript: {}", t);
        }
    }
});
```

- Returns `tokio::sync::broadcast::Receiver<RuntimeData>`.
- Multiple concurrent subscribers to the same address each get their own
  receiver — fan-out is handled by the broadcast channel.
- Dropping the receiver implicitly unsubscribes.

### 4.3 Publish — inject data into a node's input

```rust
use serde_json::json;

ctrl.publish(
    &ControlAddress::node_in("llm").with_port("context"),
    RuntimeData::Json(json!({ "docs": ["snippet 1", "snippet 2"] })),
).await?;
```

- Sends a `DataPacket` with `to_node = "llm"` into the router's input channel.
- Returns when the router accepts the packet (bounded channel — `await`
  applies backpressure if the pipeline is behind).
- `Err` if the session has closed (`input_tx` dropped).

### 4.4 Intercept — edit or drop a node's output

```rust
use remotemedia_core::transport::session_control::{
    ControlAddress, ControlEvent, InterceptDecision,
};
use std::time::Duration;

let addr = ControlAddress::node_out("llm");
let mut events = ctrl.intercept(&addr, Some(Duration::from_millis(50)))?;

let ctrl_handler = ctrl.clone();
tokio::spawn(async move {
    while let Some(ev) = events.recv().await {
        if let ControlEvent::InterceptRequest { correlation_id, data, .. } = ev {
            let redacted = redact_pii(data);
            ctrl_handler.complete_intercept(
                correlation_id,
                InterceptDecision::Replace(redacted),
            );
        }
    }
});
```

- One intercept per `(node_id, port)`. Installing a second replaces the first.
- Every output that passes through that port waits up to the deadline
  for a reply; on timeout the original frame is forwarded unchanged.
- `InterceptDecision::Drop` prevents the frame from reaching downstream
  nodes *and* the client sink.

### 4.5 Node state — enable / bypass / disable

```rust
use remotemedia_core::transport::session_control::NodeState;

// Bypass: node is skipped; inputs flow through as outputs.
ctrl.set_node_state("calc", NodeState::Bypass);

// Disabled: node is skipped and produces no outputs.
ctrl.set_node_state("whisper", NodeState::Disabled);

// Re-enable (either form works):
ctrl.set_node_state("calc", NodeState::Enabled);
ctrl.clear_node_state("calc"); // remove override entirely

// Query:
let current = ctrl.node_state("calc");
```

- Applies on the next packet the router processes.
- Lock-free (`DashMap`) — safe to call from any async context and cheap
  on the router hot path.
- `Bypass` outputs still pass through `on_node_output`, so tap
  subscribers and intercepts fire normally on the forwarded data.

### 4.6 Lifecycle — close signal

```rust
use remotemedia_core::transport::session_control::CloseReason;

let mut close_rx = ctrl.close_subscriber();

// Select over close alongside your tap/intercept streams.
tokio::select! {
    data = tap.recv() => { /* handle */ }
    reason = close_rx.recv() => {
        match reason {
            Ok(CloseReason::Normal) => { /* clean exit */ }
            Ok(CloseReason::Error(msg)) => { /* log and exit */ }
            Err(_) => { /* channel lag — treat as closed */ }
        }
        return;
    }
}
```

Fires once when the router's run-loop exits. All active taps, intercepts,
and `publish` attempts should terminate on receipt.

### 4.7 Process-wide registry

```rust
use remotemedia_core::transport::session_control::SessionControlBus;

let bus = SessionControlBus::new(); // Arc<SessionControlBus>

// Called by the executor when a session is created:
bus.register(ctrl.clone());

// Transport handler on client Attach frame:
let ctrl = bus.get(&session_id).ok_or(Error::SessionNotFound)?;

// Called by the executor on session termination (after the router joins):
bus.unregister(&session_id);
```

The bus is typically owned by the `PipelineExecutor`. The transport crate
that serves the control-plane RPC takes an `Arc<SessionControlBus>` from
the executor and routes incoming `Attach(session_id)` frames through it.

---

## 5. Python client surface (design — not yet implemented)

The Python client mirrors the Rust API over gRPC, exposed as idiomatic
asyncio. This section describes the target surface so code written against
the Rust side can anticipate it.

### 5.1 Opening a session and attaching control

```python
from remotemedia import Pipeline

pipeline = Pipeline.from_yaml("voice-assistant.yaml")

async with pipeline.stream(remote="grpc://localhost:50051") as session:
    # session.id is the authoritative session_id on the server.
    async with session.control.attach() as ctrl:
        await run_my_handlers(ctrl)
```

- `session.control.attach()` opens a **separate** control-plane RPC.
  It does **not** keep the session alive: exiting the outer `session`
  block terminates both.
- `attach()` fails fast with `SessionNotFoundError` if the server has
  already torn down the session.
- The attach runs a background reader task that dispatches incoming
  frames to active subscriber queues.

### 5.2 Addressing

Dotted strings parsed once, with optional typed accessors:

```
"whisper.out"           → (node=whisper, port=None,    dir=Out)
"llm.in.context"        → (node=llm,     port=context, dir=In)
"llm.out"               → (node=llm,     port=None,    dir=Out)
```

Grammar: `{node}.{in|out}[.{port}]`. The client validates the node
against the local copy of the manifest and raises `UnknownNodeError`
before sending the frame.

### 5.3 Tap — `async for`

```python
async for data in ctrl.subscribe("whisper.out"):
    print(data.text)
```

- `data` is a `Data` wrapper with `.text`, `.audio`, `.json`, `.binary`,
  `.stream_id`, `.timestamp_us` accessors. Wrong variant raises.
- `TapLagWarning` is yielded (not raised) when the server-side broadcast
  lags the client. The loop does not die.
- Exiting the `async for` sends `Unsubscribe`.

### 5.4 Publish — awaitable

```python
await ctrl.publish("llm.in.context", Data.text("\n".join(docs)))
```

- Blocks until the server acks the inject.
- Backpressure is real: if the router's input channel is full, this
  await blocks. Correct behavior for real-time pipelines.

### 5.5 Intercept — decorator or context manager

**Decorator form** (declarative):

```python
@ctrl.intercept("whisper.out", deadline_ms=50)
async def redact(data):
    return Data.text(pii_scrubber(data.text))   # Replace
    # return data                                # Pass
    # return None                                # Drop
```

**Context manager form** (imperative):

```python
async with ctrl.intercept("llm.out", deadline_ms=50) as stream:
    async for req in stream:
        decision = await decide(req.data)
        await req.reply(decision)
```

The decorator is sugar over the context manager.

### 5.6 Node state — enable / bypass / disable

Flip a node's runtime execution state. Sends `SetNodeState` /
`ClearNodeState` on the wire.

```python
from remotemedia import NodeState

# Bypass the noise-suppression stage (inputs flow through as outputs)
await ctrl.set_node_state("noise_suppress", NodeState.Bypass)

# Disable the TTS branch entirely (produces no output)
await ctrl.set_node_state("tts", NodeState.Disabled)

# Re-enable — either form works
await ctrl.set_node_state("tts", NodeState.Enabled)
await ctrl.clear_node_state("tts")

# Query current state
state = await ctrl.node_state("noise_suppress")
```

Semantics:

- Applies on the **next** packet the router processes. Not a barrier on
  in-flight packets — for that, drain first and then toggle.
- Awaits only as long as the transport takes to acknowledge the frame;
  state mutation itself is lock-free on the server.
- `UnknownNodeError` raised client-side if `node_id` isn't in the
  manifest (checked against the local manifest copy — no round-trip).
- `Bypass` still fans out to tap subscribers — observers see the
  forwarded data just as downstream nodes do.

### 5.7 `Data` wrapper

Thin tagged wrapper around `RuntimeData`:

```python
Data.text("hello")
Data.audio(numpy_array, sample_rate=16000, channels=1)
Data.json({"role": "user", "content": "..."})
Data.binary(b"...")

data.kind           # "text" | "audio" | "json" | "binary"
data.text           # raises if kind != "text"
data.audio          # (ndarray, sample_rate, channels)
data.json
data.stream_id
data.timestamp_us
```

---

## 6. Worked examples

### 6.1 RAG injection — the canonical case

Goal: for every transcript Whisper emits, retrieve top-k docs from a
vector DB and inject them into the LLM's `context` port.

```python
async with pipeline.stream(remote=url) as session, \
           session.control.attach() as ctrl:

    async for turn in ctrl.subscribe("whisper.out"):
        docs = await vectordb.search(turn.text, k=3)
        context = "\n\n".join(d.text for d in docs)
        await ctrl.publish("llm.in.context", Data.text(context))
```

**Zero manifest changes.** The LLM node declares a `context` auxiliary
port once; every client can populate it with different data.

### 6.2 Live PII redaction

Goal: scrub PII from transcripts before they reach the LLM.

```python
@ctrl.intercept("whisper.out")
async def redact(data):
    return Data.text(scrubber.scrub(data.text))
```

One decorator. The LLM never sees unscrubbed text. Transcripts still
flow to any other subscriber that's attached to `whisper.out` — the
intercept happens on the main wire, but tap subscribers receive the
**original** output (interception only affects downstream routing,
not fan-out).

### 6.3 Client-side tool use

Goal: the LLM emits `<tool>…</tool>` markers; the client executes the
tool locally and feeds the result back.

```python
import re
TOOL_PATTERN = re.compile(r"<tool>(\w+)\((.*)\)</tool>")

async for response in ctrl.subscribe("llm.out"):
    match = TOOL_PATTERN.search(response.text)
    if match:
        tool, args = match.groups()
        result = await TOOLS[tool](args)
        await ctrl.publish("llm.in.tool_result", Data.json(result))
```

### 6.4 Operator UI — regenerate with correction

Goal: an operator UI displays the LLM's draft response; the operator
clicks "regenerate with this correction" and the corrected turn
replaces the user input.

```python
# on operator click:
async def on_regenerate(corrected_turn: str):
    await ctrl.publish("llm.in.main", Data.text(corrected_turn))

# on operator "drop hallucination" click:
@ctrl.intercept("llm.out")
async def gate(data):
    return None if should_drop(data.text) else data
```

### 6.5 Mute a node at runtime

Goal: temporarily silence the TTS branch without modifying the manifest.

```python
# mute
await ctrl.set_node_state("tts", NodeState.Disabled)

# unmute
await ctrl.set_node_state("tts", NodeState.Enabled)
```

For an A/B test of a processing stage, `Bypass` is more useful than
`Disabled` — it short-circuits the node while still delivering data to
downstream consumers:

```python
# compare "with noise suppression" vs "without" in the same session
await ctrl.set_node_state("noise_suppress", NodeState.Bypass)
await asyncio.sleep(5.0)
await ctrl.set_node_state("noise_suppress", NodeState.Enabled)
```

### 6.6 Debug tracing without touching the manifest

Goal: stream every node's output to a local log / waveform viewer.

```python
for node_id in ["vad", "whisper", "llm", "tts"]:
    asyncio.create_task(tap_to_log(ctrl, node_id))

async def tap_to_log(ctrl, node_id):
    async for data in ctrl.subscribe(f"{node_id}.out"):
        await logfile.write(f"{node_id}: {data!r}\n")
```

---

## 7. Gotchas and limitations

### 7.1 Known today

- **Ordering on intercept + tap.** Taps fire on the *original* node output;
  intercepts modify the value sent downstream. If you want taps to see the
  modified value, subscribe to a downstream node instead.
- **`publish` keeps the router's input channel alive.** `SessionControl`
  holds its own clone of `input_tx` via `attach_input_sender`. Dropping the
  external `input_tx` is not sufficient to shut down — use the explicit
  `shutdown_tx` returned by `SessionRouter::new`. This is documented
  behavior, not a bug: the bus must be able to inject even when the
  transport is not currently streaming main data.
- **One intercept per `(node_id, port)`.** Installing a second replaces
  the first with a warning log. Multiple interceptors would require
  explicit chaining/ordering semantics we haven't specified.
- **Intercept reentrancy loops.** If your intercept handler itself calls
  `ctrl.publish(...)` into the **same** node being intercepted, you can
  create an infinite loop. The prototype does not detect this. Callers
  must avoid it (or use a different address).
- **Ports are free-form strings.** The node API doesn't yet declare typed
  output/input rails. All current hooks pass `port = None`. Auxiliary
  ports work via manifest + node-side conventions for now.
- **Authorization is not enforced.** `SessionControlBus::get(session_id)`
  returns the control handle for any caller. Production deployments must
  gate this at the transport layer via whatever auth scheme (tokens,
  mTLS, per-session grants) the deployment uses.

### 7.2 Not yet implemented

- **Python client module.** Design is above; code is not in the tree yet.
- **Control-plane RPC.** Transport framing for `AttachFrame` / `AttachEvent`
  lives in the transport crates and has not been wired up.
- **Typed per-node auxiliary input ports.** Needs a
  `DataPacket.port: Option<String>` field plus node-API declaration of
  accepted ports with capability types (spec 023/025 extension).
- **Per-stream intercept stacking.** Explicit chain ordering would allow
  multiple intercepts to compose predictably.

---

## 8. Testing

### 8.1 Rust

```bash
# Unit tests for the control primitives
cargo test -p remotemedia-core --lib transport::session_control

# Integration tests: router + attached control end-to-end
cargo test -p remotemedia-core --test session_control_integration
```

The integration test file at
[`crates/core/tests/session_control_integration.rs`](../crates/core/tests/session_control_integration.rs)
is the canonical reference for correct usage. It covers:

- `tap_observes_node_output_end_to_end` — subscribe path
- `publish_injects_input_into_pipeline` — inject path
- `intercept_replaces_downstream_value` — intercept path (with a
  client-side handler that replies to intercept requests)
- `node_bypass_forwards_inputs_to_sink` — `SetNodeState(Bypass)`
  skips execution and forwards inputs as outputs
- `node_disabled_drops_output` — `SetNodeState(Disabled)` produces
  no output downstream
- `node_state_toggles_at_runtime` — full Enabled → Bypass → Disabled
  → clear-to-Enabled cycle across sequential packets
- `bypass_still_fans_out_to_taps` — bypassed nodes still emit to
  tap subscribers via `on_node_output`
- `close_signal_fires_on_router_shutdown` — lifecycle teardown

### 8.2 Python

Not yet — Python surface pending.

---

## 9. Reading the source

| Concept                     | File                                                               |
|-----------------------------|--------------------------------------------------------------------|
| Bus + per-session state     | `crates/core/src/transport/session_control.rs`                     |
| Router integration points   | `crates/core/src/transport/session_router.rs` (search `control`)    |
| End-to-end tests            | `crates/core/tests/session_control_integration.rs`                 |
| Related: `SessionRouter`    | Rest of `crates/core/src/transport/session_router.rs`              |
| Related: data types         | `crates/core/src/lib.rs` (`RuntimeData`)                           |

---

## 10. Future work

- **Typed auxiliary ports** with capability declarations per port.
- **Python `remotemedia.control` module** implementing the surface in §5.
- **gRPC `PipelineControl` service** with `Attach(stream)` bidirectional RPC.
- **Authorization hook** on `SessionControlBus::get`.
- **Intercept composition** — multiple stacked intercepts with ordering.
- **Metrics** — per-address tap lag counters, intercept deadline misses,
  pending intercept counts, exposed via `ctrl.stats()` and the existing
  Prometheus endpoint.
- **Session-scoped record-and-replay** — a first-class consumer of taps
  on every node that dumps a replay file for debugging production sessions.
