//! Session Control Bus — client-side pub/sub/intercept over a live session.
//!
//! # Goal
//!
//! Let a client (Python SDK, browser, operator UI) observe, inject, and edit
//! data flowing through any node of a running session without redefining the
//! pipeline manifest. The session is the first-class address; nodes are named
//! endpoints within it; ports are an optional qualifier defaulting to `main`.
//!
//! # Address model
//!
//! ```text
//! session_id / node_id [ . port ] / direction
//! ─────┬────── ──┬──── ───┬──── ────┬─────
//!   primary   topic     default   "in" = inject   (client -> node input)
//!   key       segment   "main"    "out" = tap     (node output -> client)
//! ```
//!
//! The control bus is **per-session** — a client "attaches to session X" and
//! from there addresses `{node_id}[.port]` for any operation. Ports are not
//! modeled in this prototype beyond an optional string; extending them to
//! structured capabilities is a later step (see spec 023/025).
//!
//! # Topology
//!
//! ```text
//!                  ┌────────────────────────────┐
//!                  │        SessionRouter       │
//!                  │                            │
//!   client ──publish──> input_tx (to_node = X) ─┤  [inject path]
//!                  │           │                │
//!                  │           ▼                │
//!                  │       node X runs          │
//!                  │           │                │
//!                  │           ▼                │
//!                  │     outputs ───────────────┤  [tap path]
//!                  │           │   ├─> broadcast to subscribers
//!                  │           │   └─> (if intercepted) oneshot-to-client,
//!                  │           │       awaits reply with deadline
//!                  │           ▼                │
//!                  │     downstream nodes       │
//!                  └────────────────────────────┘
//! ```
//!
//! # What this prototype provides
//!
//! - [`SessionControlBus`]       — process-wide registry, keyed by session_id.
//! - [`SessionControl`]          — per-session handle holding subscribers,
//!                                 injectors, and intercept hooks.
//! - [`ControlFrame`]            — wire frame (Subscribe/Publish/Intercept/Reply).
//! - [`ControlAddress`]          — `{node_id, port, direction}` tuple.
//! - A single integration hook for [`SessionRouter`]:
//!     * `SessionRouter::attach_control(&SessionControl)` — called at session
//!       creation; gives the router a handle it consults in `process_input`
//!       to broadcast outputs and apply intercepts.
//!
//! # What this prototype does NOT do (yet)
//!
//! - Per-node auxiliary *port schemas* / capability typing. Ports here are
//!   free-form strings; nodes choose how to interpret auxiliary publishes
//!   (e.g. `llm.in.context` is just metadata on the `DataPacket` that the
//!   node's `process_streaming` inspects). Typed ports are a follow-up.
//! - Authorization. Attach frames are trusted in this prototype; a real
//!   deployment must gate `attach(session_id)` on ownership.
//!   Noted as [`ControlAuth`] placeholder below.
//! - Transport framing. `ControlFrame` is the logical message; gRPC /
//!   WebSocket wrapping lives in the transport crates.

use crate::data::RuntimeData;
use crate::transport::session_router::DataPacket;
use crate::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};

// Capacity of the per-(node,port) output broadcast channel.
// Slow subscribers get `RecvError::Lagged` and skip ahead — never block the
// hot path. Sized for bursty multi-output nodes (e.g. LFM2-Audio emitting
// 20+ text tokens + 60+ audio-liveness envelopes per turn). 16 was enough
// for 20ms mic frames but truncated every model reply.
const DEFAULT_TAP_CAPACITY: usize = 1024;

// Deadline a node's output will wait on an intercept reply before the
// original frame is passed through unchanged (and a warning is logged).
// Prevents a disconnected client from stalling the pipeline.
const DEFAULT_INTERCEPT_DEADLINE: Duration = Duration::from_millis(50);

// ────────────────────────────────────────────────────────────────────────────
// Addressing
// ────────────────────────────────────────────────────────────────────────────

/// A point within a session the client wants to talk to.
#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlAddress {
    /// Node id within the session's manifest.
    pub node_id: String,
    /// Optional named port. `None` means the node's `main` port.
    pub port: Option<String>,
    /// Which side of the node: input-facing or output-facing.
    pub direction: Direction,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// `publish` target — client -> node input (auxiliary input if port is set).
    In,
    /// `subscribe` / `intercept` target — node output -> client.
    Out,
}

impl ControlAddress {
    pub fn node_in(node_id: impl Into<String>) -> Self {
        Self { node_id: node_id.into(), port: None, direction: Direction::In }
    }
    pub fn node_out(node_id: impl Into<String>) -> Self {
        Self { node_id: node_id.into(), port: None, direction: Direction::Out }
    }
    pub fn with_port(mut self, port: impl Into<String>) -> Self {
        self.port = Some(port.into());
        self
    }
    fn key(&self) -> (String, Option<String>, Direction) {
        (self.node_id.clone(), self.port.clone(), self.direction)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Wire frames
// ────────────────────────────────────────────────────────────────────────────

/// Logical control-plane frame. Transport crates wrap this in gRPC/WS framing.
#[derive(Clone, Debug)]
pub enum ControlFrame {
    /// Start observing outputs of a node/port.
    Subscribe(ControlAddress),
    /// Stop observing.
    Unsubscribe(ControlAddress),
    /// Push data into a node's input (main or auxiliary port).
    Publish { addr: ControlAddress, data: RuntimeData },
    /// Splice a hook between a node's output and its downstream fanout.
    /// The client is expected to respond with `InterceptReply` keyed by
    /// `correlation_id` within the deadline.
    Intercept { addr: ControlAddress, deadline: Option<Duration> },
    /// Detach an intercept.
    RemoveIntercept(ControlAddress),
    /// Client response to an `InterceptRequest` event.
    InterceptReply { correlation_id: u64, decision: InterceptDecision },
    /// Flip a node's runtime execution state (Enabled / Bypass / Disabled).
    /// Applies on the next packet the router processes.
    SetNodeState { node_id: String, state: NodeState },
    /// Reset a node to the default `Enabled` state.
    ClearNodeState { node_id: String },
}

#[derive(Clone, Debug)]
pub enum InterceptDecision {
    /// Forward this data unchanged to downstream.
    Pass,
    /// Replace the frame with this data before forwarding.
    Replace(RuntimeData),
    /// Drop the frame entirely — downstream nodes don't see it.
    Drop,
}

/// Events the bus emits toward a connected client.
#[derive(Clone, Debug)]
pub enum ControlEvent {
    /// A tapped output. Addr is the origin (`node_id`, `port`, `Out`).
    Tap { addr: ControlAddress, data: RuntimeData },
    /// An intercept asking for a decision; reply with `InterceptReply`.
    InterceptRequest { addr: ControlAddress, correlation_id: u64, data: RuntimeData },
    /// Out-of-band error (e.g., publish rejected, deadline exceeded).
    Error { addr: Option<ControlAddress>, message: String },
}

// ────────────────────────────────────────────────────────────────────────────
// Per-session control state
// ────────────────────────────────────────────────────────────────────────────

/// Placeholder — real deployments will gate attach on this.
#[derive(Clone, Debug, Default)]
pub struct ControlAuth {
    pub principal: Option<String>,
}

type TapKey = (String, Option<String>); // (node_id, port)
type InterceptKey = (String, Option<String>);

/// One active interception. The router awaits `reply_rx` for at most `deadline`.
struct InterceptSlot {
    events_tx: mpsc::Sender<ControlEvent>,
    deadline: Duration,
    next_correlation: std::sync::atomic::AtomicU64,
    /// Map correlation_id -> reply oneshot. Bounded by pipeline latency; we
    /// clear entries on deadline expiry.
    pending: DashMap<u64, oneshot::Sender<InterceptDecision>>,
}

/// Reason a session closed — propagated to every attached control client
/// via [`SessionControl::close_subscriber`].
#[derive(Clone, Debug)]
pub enum CloseReason {
    Normal,
    Error(String),
}

/// Envelope field name for aux-port publishes. Nodes inspect the payload
/// for this key to distinguish aux-channel input from the main input.
pub const AUX_PORT_ENVELOPE_KEY: &str = "__aux_port__";

/// Reserved aux port name for "the user has barged in / cancel the
/// in-flight call". Handled universally by the router (see
/// `session_router::spawn_node_pipeline`) — nodes never see a frame
/// on this port; the runtime aborts whatever they're currently doing
/// and drops the envelope.
pub const BARGE_IN_PORT: &str = "barge_in";

/// If `data` is an aux-port envelope produced by [`wrap_aux_port`],
/// return the port name. Otherwise return `None`.
///
/// Used by the router to filter universal control frames (currently
/// `barge_in`) before they reach `process_*` and to decide whether a
/// frame is aux-channel or main-channel for routing purposes.
pub fn aux_port_of(data: &RuntimeData) -> Option<&str> {
    match data {
        RuntimeData::Json(v) => v.get(AUX_PORT_ENVELOPE_KEY).and_then(|p| p.as_str()),
        _ => None,
    }
}

/// Wrap a `RuntimeData` payload in the aux-port envelope described in
/// [`SessionControl::publish`]. Pulled out so `python_streaming_node.rs`
/// and test code can construct the same shape.
pub fn wrap_aux_port(port: &str, data: RuntimeData) -> RuntimeData {
    let inner = match data {
        RuntimeData::Json(v) => v,
        RuntimeData::Text(t) => serde_json::json!({ "text": t }),
        RuntimeData::Binary(b) => {
            use base64::Engine as _;
            serde_json::json!({
                "binary_b64": base64::engine::general_purpose::STANDARD.encode(&b),
            })
        }
        // For richer types (Audio, Video, Tensor, Numpy), the envelope
        // carries a type tag; the node is expected to consult the
        // original payload via another code path if needed. For the
        // common aux-port use cases (text context, JSON facts, tool
        // results) the cases above cover the traffic.
        other => serde_json::json!({
            "_unsupported_for_envelope": format!("{:?}", std::mem::discriminant(&other)),
        }),
    };
    RuntimeData::Json(serde_json::json!({
        AUX_PORT_ENVELOPE_KEY: port,
        "payload": inner,
    }))
}

/// Runtime execution state for a single node, controlled via the bus.
///
/// - `Enabled` (default): node runs normally.
/// - `Bypass`: node is skipped; its inputs are forwarded as its outputs.
///   Downstream nodes see the same data the bypassed node would have
///   received — as if the node were a passthrough.
/// - `Disabled`: node is skipped and produces no outputs. Downstream
///   nodes see nothing from this branch for the current input. Useful
///   when you want to temporarily sever a subgraph without modifying
///   the manifest.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    Enabled,
    Bypass,
    Disabled,
}

impl Default for NodeState {
    fn default() -> Self {
        NodeState::Enabled
    }
}

/// Per-session control state. Created alongside the `SessionRouter` and
/// handed to the router via `attach_control`.
pub struct SessionControl {
    session_id: String,
    /// Broadcasts a node output to every subscriber of (node_id, port).
    /// `broadcast::Sender` is cheap to clone; slow receivers lag, not block.
    taps: DashMap<TapKey, broadcast::Sender<RuntimeData>>,
    /// Active intercepts keyed by (node_id, port). Only one intercept per
    /// output port in v1 — stacking is possible but adds ordering ambiguity.
    intercepts: DashMap<InterceptKey, Arc<InterceptSlot>>,
    /// Handle to the router's input channel so `publish` can inject packets.
    /// Set by `attach_input_sender` after the router is constructed.
    input_tx: RwLock<Option<mpsc::Sender<DataPacket>>>,
    /// Monotonic sequence for injected packets (kept separate from the
    /// transport's own sequence to make out-of-band traffic identifiable).
    inject_seq: std::sync::atomic::AtomicU64,
    /// Broadcasts session-close to every attached client. Sent once by the
    /// router when its run-loop exits. Unbounded lag is fine — the payload
    /// is one terminal message.
    close_tx: broadcast::Sender<CloseReason>,
    /// Per-node execution state, driven by the bus. Absent entry = Enabled.
    /// Read on the router hot path once per node per packet; lock-free via
    /// DashMap to avoid an async RwLock on every output.
    node_states: DashMap<String, NodeState>,
    /// Optional fire-and-forget channel installed by the transport layer
    /// (e.g. WebRTC `ServerPeer`) so Rust nodes can request a drain of
    /// the transport's outbound audio buffer without depending on the
    /// transport crate directly. The transport sets the hook once at
    /// session setup; nodes call `request_flush_audio()` to send a
    /// ping. Absent on transports that don't have a flushable audio
    /// queue (gRPC, etc.) — callers should treat its absence as a
    /// no-op.
    flush_audio_tx: RwLock<Option<mpsc::Sender<()>>>,
}

impl SessionControl {
    pub fn new(session_id: impl Into<String>) -> Arc<Self> {
        let (close_tx, _) = broadcast::channel(1);
        Arc::new(Self {
            session_id: session_id.into(),
            taps: DashMap::new(),
            intercepts: DashMap::new(),
            input_tx: RwLock::new(None),
            inject_seq: std::sync::atomic::AtomicU64::new(0),
            close_tx,
            node_states: DashMap::new(),
            flush_audio_tx: RwLock::new(None),
        })
    }

    /// Install the flush-audio hook. Called by the transport layer at
    /// session setup; later calls overwrite the previous hook (last-
    /// writer-wins, rare in practice since each session has one
    /// transport).
    pub async fn install_flush_audio_hook(&self, tx: mpsc::Sender<()>) {
        *self.flush_audio_tx.write().await = Some(tx);
    }

    /// Request an audio-buffer flush on the transport attached to this
    /// session. Fire-and-forget: returns `true` if a hook was installed
    /// and the ping was accepted (or the channel is just slow),
    /// `false` if no transport registered a hook. Never blocks.
    pub async fn request_flush_audio(&self) -> bool {
        let tx = self.flush_audio_tx.read().await.clone();
        match tx {
            Some(tx) => tx.try_send(()).is_ok(),
            None => false,
        }
    }

    /// Set a node's runtime execution state.
    ///
    /// Takes effect on the **next** packet the router processes. Callers
    /// that need a happens-before guarantee on an in-flight packet should
    /// coordinate with the data plane separately — this is a best-effort
    /// toggle, not a barrier.
    pub fn set_node_state(&self, node_id: impl Into<String>, state: NodeState) {
        self.node_states.insert(node_id.into(), state);
    }

    /// Reset a node to the default `Enabled` state (removes the override).
    pub fn clear_node_state(&self, node_id: &str) {
        self.node_states.remove(node_id);
    }

    /// Current state for a node. Nodes without an explicit override are
    /// `Enabled`. Called on the router hot path — DashMap lookup is
    /// lock-free.
    pub fn node_state(&self, node_id: &str) -> NodeState {
        self.node_states
            .get(node_id)
            .map(|e| *e.value())
            .unwrap_or(NodeState::Enabled)
    }

    /// Per-attach handle that fires once when the session closes.
    /// Every attach subscribes; dropping the receiver is harmless.
    pub fn close_subscriber(&self) -> broadcast::Receiver<CloseReason> {
        self.close_tx.subscribe()
    }

    /// Called once from `SessionRouter` just before its run-loop returns.
    /// Signals every attach to drain and exit. Idempotent.
    pub fn signal_close(&self, reason: CloseReason) {
        let _ = self.close_tx.send(reason);
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Wire the router's input channel so `publish` can reach nodes.
    /// Called once from `SessionRouter::attach_control`.
    pub async fn attach_input_sender(&self, tx: mpsc::Sender<DataPacket>) {
        *self.input_tx.write().await = Some(tx);
    }

    // ─── Client-facing API (driven by `handle_frame`) ──────────────────────

    /// Subscribe to a node's output. Returns a broadcast receiver the
    /// transport layer forwards to the client as `ControlEvent::Tap`.
    pub fn subscribe(&self, addr: &ControlAddress) -> Result<broadcast::Receiver<RuntimeData>> {
        if addr.direction != Direction::Out {
            return Err(crate::Error::Execution(
                "subscribe requires Direction::Out".into(),
            ));
        }
        let key: TapKey = (addr.node_id.clone(), addr.port.clone());
        let sender = self
            .taps
            .entry(key)
            .or_insert_with(|| broadcast::channel(DEFAULT_TAP_CAPACITY).0)
            .clone();
        Ok(sender.subscribe())
    }

    /// Inject data into a node's input port.
    ///
    /// When `addr.port` is `Some(port_name)` — i.e. the client is writing
    /// to an auxiliary input rail, not the node's main input — the payload
    /// is wrapped in an envelope so the node can distinguish aux-channel
    /// input from main-channel input:
    ///
    /// ```json
    /// {
    ///   "__aux_port__": "context",
    ///   "payload": <original payload, recursive>
    /// }
    /// ```
    ///
    /// Nodes that care about aux ports (e.g. an LLM reading a `context`
    /// rail) inspect the envelope in their `.process()`. Nodes that don't
    /// care can treat the envelope as opaque and ignore it, or unwrap
    /// `payload` to get the original shape.
    ///
    /// `addr.port == None` means the main rail — payload is forwarded
    /// unchanged, preserving the existing wire behavior.
    pub async fn publish(&self, addr: &ControlAddress, data: RuntimeData) -> Result<()> {
        if addr.direction != Direction::In {
            return Err(crate::Error::Execution(
                "publish requires Direction::In".into(),
            ));
        }
        let tx_guard = self.input_tx.read().await;
        let tx = tx_guard.as_ref().ok_or_else(|| {
            crate::Error::Execution("session control not yet attached to router".into())
        })?;

        let payload = match &addr.port {
            None => data,
            Some(port) => wrap_aux_port(port, data),
        };

        let packet = DataPacket {
            data: payload,
            from_node: format!("__control__:{}", self.session_id),
            to_node: Some(addr.node_id.clone()),
            session_id: self.session_id.clone(),
            sequence: self
                .inject_seq
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            sub_sequence: 0,
        };
        tx.send(packet).await.map_err(|_| {
            crate::Error::Execution(format!(
                "session {} router input closed",
                self.session_id
            ))
        })
    }

    /// Install an intercept for a node's output port. The returned
    /// `events_rx` is handed to the transport to forward
    /// `InterceptRequest`s to the client.
    pub fn intercept(
        &self,
        addr: &ControlAddress,
        deadline: Option<Duration>,
    ) -> Result<mpsc::Receiver<ControlEvent>> {
        if addr.direction != Direction::Out {
            return Err(crate::Error::Execution(
                "intercept requires Direction::Out".into(),
            ));
        }
        let key: InterceptKey = (addr.node_id.clone(), addr.port.clone());
        let (events_tx, events_rx) = mpsc::channel(DEFAULT_TAP_CAPACITY);
        let slot = Arc::new(InterceptSlot {
            events_tx,
            deadline: deadline.unwrap_or(DEFAULT_INTERCEPT_DEADLINE),
            next_correlation: std::sync::atomic::AtomicU64::new(0),
            pending: DashMap::new(),
        });
        if self.intercepts.insert(key, slot).is_some() {
            tracing::warn!(
                session = %self.session_id,
                node = %addr.node_id,
                "replaced existing intercept"
            );
        }
        Ok(events_rx)
    }

    pub fn remove_intercept(&self, addr: &ControlAddress) {
        let key: InterceptKey = (addr.node_id.clone(), addr.port.clone());
        self.intercepts.remove(&key);
    }

    /// Complete a pending intercept. Called by the transport when the
    /// client sends an `InterceptReply` frame.
    pub fn complete_intercept(&self, correlation_id: u64, decision: InterceptDecision) {
        // We don't know which (node,port) owns this correlation_id without
        // searching — intercepts are few, so a linear scan is fine.
        for entry in self.intercepts.iter() {
            if let Some((_, tx)) = entry.value().pending.remove(&correlation_id) {
                let _ = tx.send(decision);
                return;
            }
        }
    }

    /// Dispatch a client-originated frame. This is the single entry point the
    /// transport layer calls; it returns any event stream the frame opens
    /// (for Subscribe/Intercept) wrapped in `FrameOutcome`.
    pub async fn handle_frame(&self, frame: ControlFrame) -> Result<FrameOutcome> {
        match frame {
            ControlFrame::Subscribe(addr) => {
                let rx = self.subscribe(&addr)?;
                Ok(FrameOutcome::Tap(rx))
            }
            ControlFrame::Unsubscribe(addr) => {
                // Receivers are reclaimed when the transport drops its
                // broadcast::Receiver; nothing to actively cancel here.
                let _ = addr;
                Ok(FrameOutcome::Done)
            }
            ControlFrame::Publish { addr, data } => {
                self.publish(&addr, data).await?;
                Ok(FrameOutcome::Done)
            }
            ControlFrame::Intercept { addr, deadline } => {
                let rx = self.intercept(&addr, deadline)?;
                Ok(FrameOutcome::Intercept(rx))
            }
            ControlFrame::RemoveIntercept(addr) => {
                self.remove_intercept(&addr);
                Ok(FrameOutcome::Done)
            }
            ControlFrame::InterceptReply { correlation_id, decision } => {
                self.complete_intercept(correlation_id, decision);
                Ok(FrameOutcome::Done)
            }
            ControlFrame::SetNodeState { node_id, state } => {
                self.set_node_state(node_id, state);
                Ok(FrameOutcome::Done)
            }
            ControlFrame::ClearNodeState { node_id } => {
                self.clear_node_state(&node_id);
                Ok(FrameOutcome::Done)
            }
        }
    }

    /// Publish a frame to a node's tap WITHOUT sending it through the
    /// data-path. This is the side channel nodes use to emit
    /// control-plane events (e.g. `turn_state` envelopes from the
    /// `ConversationCoordinatorNode`) that clients subscribed to
    /// `<node>.out` should see but that must NOT reach the
    /// downstream consumer. `broadcast::Sender::send` is sync so the
    /// method stays sync; callers can invoke from any context
    /// including the middle of a sync `process_streaming`.
    ///
    /// Ensures the tap sender exists so the broadcast ring captures
    /// the event even if no subscriber has attached yet — the first
    /// late subscriber then sees whatever's still in the buffer.
    pub fn publish_tap(&self, node_id: &str, port: Option<&str>, data: RuntimeData) {
        let key: TapKey = (node_id.to_string(), port.map(|s| s.to_string()));
        let sender = self
            .taps
            .entry(key)
            .or_insert_with(|| broadcast::channel(DEFAULT_TAP_CAPACITY).0)
            .clone();
        let _ = sender.send(data);
    }

    // ─── Router-facing API (called from SessionRouter::process_input) ──────

    /// Called by the router after a node produces an output, before the
    /// output is stored for downstream routing. Returns the (possibly
    /// replaced) data, or `None` if the intercept asked to drop it.
    ///
    /// Fans out to tap subscribers as a side effect.
    pub async fn on_node_output(
        &self,
        node_id: &str,
        port: Option<&str>,
        data: RuntimeData,
    ) -> Option<RuntimeData> {
        let port_owned = port.map(|s| s.to_string());

        // 1. Broadcast to tap subscribers (fire-and-forget; slow subs lag).
        let tap_key: TapKey = (node_id.to_string(), port_owned.clone());
        if let Some(tap) = self.taps.get(&tap_key) {
            let _ = tap.send(data.clone());
        }

        // 2. If an intercept is installed, ask the client and wait (bounded).
        let intercept = self
            .intercepts
            .get(&tap_key)
            .map(|r| r.value().clone());
        let Some(slot) = intercept else {
            return Some(data);
        };

        let correlation_id = slot
            .next_correlation
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        slot.pending.insert(correlation_id, reply_tx);

        let request = ControlEvent::InterceptRequest {
            addr: ControlAddress {
                node_id: node_id.to_string(),
                port: port_owned,
                direction: Direction::Out,
            },
            correlation_id,
            data: data.clone(),
        };

        // If the event channel is full or closed, we fall through to Pass so
        // the data plane never stalls on a wedged control plane.
        if slot.events_tx.try_send(request).is_err() {
            slot.pending.remove(&correlation_id);
            tracing::warn!(
                session = %self.session_id,
                node = %node_id,
                "intercept event channel saturated; passing through"
            );
            return Some(data);
        }

        match tokio::time::timeout(slot.deadline, reply_rx).await {
            Ok(Ok(InterceptDecision::Pass)) => Some(data),
            Ok(Ok(InterceptDecision::Replace(replacement))) => Some(replacement),
            Ok(Ok(InterceptDecision::Drop)) => None,
            Ok(Err(_canceled)) => {
                tracing::warn!(
                    session = %self.session_id,
                    node = %node_id,
                    "intercept reply channel dropped; passing through"
                );
                Some(data)
            }
            Err(_elapsed) => {
                slot.pending.remove(&correlation_id);
                tracing::warn!(
                    session = %self.session_id,
                    node = %node_id,
                    deadline_ms = slot.deadline.as_millis() as u64,
                    "intercept deadline exceeded; passing through"
                );
                Some(data)
            }
        }
    }
}

/// Return type of [`SessionControl::handle_frame`]. The transport layer
/// decides how to ferry streams (broadcast receiver / mpsc receiver) back
/// to the client over its wire protocol.
pub enum FrameOutcome {
    Done,
    Tap(broadcast::Receiver<RuntimeData>),
    Intercept(mpsc::Receiver<ControlEvent>),
}

// ────────────────────────────────────────────────────────────────────────────
// Process-wide registry
// ────────────────────────────────────────────────────────────────────────────

/// Process-wide registry of active session controls. The control-plane
/// transport (gRPC/WebSocket handler) looks up a `SessionControl` here by
/// `session_id` after the client sends its `Attach(session_id)` frame.
///
/// The `PipelineExecutor` is the natural owner of this bus; `create_session`
/// inserts, `terminate_session` removes.
#[derive(Default)]
pub struct SessionControlBus {
    sessions: DashMap<String, Arc<SessionControl>>,
}

impl SessionControlBus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Register a new session. Typically called by the executor right after
    /// `SessionRouter::new`.
    pub fn register(&self, control: Arc<SessionControl>) {
        self.sessions.insert(control.session_id.clone(), control);
    }

    /// Remove a session (on shutdown / termination). All active subscribers
    /// drop their receivers naturally.
    pub fn unregister(&self, session_id: &str) {
        self.sessions.remove(session_id);
    }

    /// Look up a session's control handle. Transport handler calls this
    /// once per client `Attach` and caches the Arc for the connection's
    /// lifetime.
    pub fn get(&self, session_id: &str) -> Option<Arc<SessionControl>> {
        self.sessions.get(session_id).map(|e| e.value().clone())
    }

    /// Install `bus` as the process-wide singleton if none is set yet.
    /// First-writer-wins: later calls from secondary executors (typical
    /// only in multi-executor tests) silently no-op. Regular single-
    /// executor deployments install their bus here once.
    pub fn install_global(bus: Arc<Self>) {
        let _ = GLOBAL_BUS.set(bus);
    }
}

static GLOBAL_BUS: std::sync::OnceLock<Arc<SessionControlBus>> = std::sync::OnceLock::new();

/// Get the process-wide [`SessionControlBus`] if one has been installed.
///
/// Returns `None` until `PipelineExecutor::new` / `with_config` has run
/// once in this process. Rust nodes that need to publish to another
/// node's aux port (e.g. `ConversationCoordinatorNode` firing
/// `llm.in.barge_in` on a user-barge) look themselves up via
/// `global_bus()?.get(session_id)?.publish(...)`.
///
/// Intentionally permissive about missing state — nodes that reach for
/// the bus before an executor exists just degrade to a no-op (the
/// client-side barge-in fanout is the fallback), so a wrong test setup
/// doesn't panic the data plane.
pub fn global_bus() -> Option<Arc<SessionControlBus>> {
    GLOBAL_BUS.get().cloned()
}

// ────────────────────────────────────────────────────────────────────────────
// Integration hook for SessionRouter
// ────────────────────────────────────────────────────────────────────────────
//
// Adding the bus to the existing router requires two small edits to
// session_router.rs. They are written out here rather than applied, so you
// can review before I touch that file.
//
// 1.  Field + setter on `SessionRouter`:
//
//         control: Option<Arc<SessionControl>>,
//
//     and:
//
//         pub async fn attach_control(&mut self, control: Arc<SessionControl>) {
//             control
//                 .attach_input_sender(self.input_tx.clone().expect("pre-run"))
//                 .await;
//             self.control = Some(control);
//         }
//
// 2.  One hook inside `process_input`, after the node's outputs are
//     collected and before they're stored for downstream routing:
//
//         let node_outputs = node_outputs_ref.lock().unwrap().clone();
//         let node_outputs = if let Some(ctrl) = &self.control {
//             let mut filtered = Vec::with_capacity(node_outputs.len());
//             for out in node_outputs {
//                 if let Some(kept) = ctrl.on_node_output(node_id, None, out).await {
//                     filtered.push(kept);
//                 }
//             }
//             filtered
//         } else {
//             node_outputs
//         };
//         all_node_outputs.insert(node_id.clone(), node_outputs);
//
//     (Once typed output ports land, replace `None` with the actual port
//     name the node wrote to.)
//
// 3.  Inject path is already handled: publish() pushes a `DataPacket` with
//     `to_node = addr.node_id` into the router's existing input channel,
//     which process_input already respects (see `packet.to_node` branch).
//     The only missing field is `port` on DataPacket for aux-input routing;
//     adding it is backwards compatible (Option<String>, default None).

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> RuntimeData {
        RuntimeData::Text(s.to_string())
    }

    #[tokio::test]
    async fn tap_receives_node_outputs() {
        let ctrl = SessionControl::new("sess-1");
        let addr = ControlAddress::node_out("whisper");
        let mut rx = ctrl.subscribe(&addr).unwrap();

        let kept = ctrl.on_node_output("whisper", None, text("hello")).await;
        assert!(kept.is_some());

        let got = rx.recv().await.unwrap();
        match got {
            RuntimeData::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("wrong type"),
        }
    }

    #[tokio::test]
    async fn intercept_replace_mutates_downstream() {
        let ctrl = SessionControl::new("sess-1");
        let addr = ControlAddress::node_out("llm");
        let mut events = ctrl.intercept(&addr, Some(Duration::from_millis(200))).unwrap();

        // Spawn the data-plane side.
        let ctrl_dp = ctrl.clone();
        let data_task = tokio::spawn(async move {
            ctrl_dp.on_node_output("llm", None, text("raw")).await
        });

        // Receive the intercept request and reply with Replace.
        let ev = events.recv().await.unwrap();
        let ControlEvent::InterceptRequest { correlation_id, .. } = ev else {
            panic!("expected InterceptRequest");
        };
        ctrl.complete_intercept(
            correlation_id,
            InterceptDecision::Replace(text("redacted")),
        );

        let out = data_task.await.unwrap().unwrap();
        match out {
            RuntimeData::Text(s) => assert_eq!(s, "redacted"),
            _ => panic!("wrong type"),
        }
    }

    #[tokio::test]
    async fn intercept_deadline_passes_through() {
        let ctrl = SessionControl::new("sess-1");
        let addr = ControlAddress::node_out("llm");
        let _events = ctrl.intercept(&addr, Some(Duration::from_millis(20))).unwrap();
        // No reply — deadline should fire and the frame passes through.
        let out = ctrl.on_node_output("llm", None, text("x")).await.unwrap();
        match out {
            RuntimeData::Text(s) => assert_eq!(s, "x"),
            _ => panic!("wrong type"),
        }
    }

    #[tokio::test]
    async fn publish_without_router_errors_cleanly() {
        let ctrl = SessionControl::new("sess-1");
        let addr = ControlAddress::node_in("llm").with_port("context");
        let err = ctrl.publish(&addr, text("doc")).await.unwrap_err();
        assert!(err.to_string().contains("not yet attached"));
    }

    #[tokio::test]
    async fn bus_registry_lookup() {
        let bus = SessionControlBus::new();
        let ctrl = SessionControl::new("sess-A");
        bus.register(ctrl.clone());

        assert!(bus.get("sess-A").is_some());
        assert!(bus.get("missing").is_none());

        bus.unregister("sess-A");
        assert!(bus.get("sess-A").is_none());
    }
}
