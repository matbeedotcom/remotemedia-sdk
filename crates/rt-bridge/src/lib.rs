//! Real-time thread ↔ async pipeline bridge.
//!
//! `remotemedia-rt-bridge` lets real-time audio hosts — Core Audio HAL
//! plugins, AudioUnit render callbacks, JACK client process callbacks,
//! AAudio data callbacks — drive a `remotemedia-core` node without ever
//! entering the tokio scheduler, awaiting a bounded channel, or
//! contending a shared lock.
//!
//! # Model
//!
//! ```text
//!                            ┌──────────────────────┐
//!   RT audio thread          │  Pinned worker thread │   (non-RT) async world
//!   (HAL IO proc)            │  (remotemedia-rt-bridge) │
//!                            │                      │
//!   fill input slice ──┐     │                      │
//!                      │     │                      │
//!   RtInputProducer    │     │                      │
//!    .try_push()  ─────┼────►│  input  (rtrb SPSC)  │
//!                      │     │  ring   consumer     │
//!                      │     │                      │
//!                      │     │  node.process(data)  │
//!                      │     │   [SyncStreamingNode]│
//!                      │     │                      │
//!   RtOutputConsumer   │     │  output (rtrb SPSC)  │
//!    .try_pop() ◄──────┼─────│  ring   producer     │
//!                      │     │                      │
//!   write output  ─────┘     └──────────────────────┘
//! ```
//!
//! # What the bridge guarantees
//!
//! - **No tokio.** The worker thread is a plain OS thread; it does not
//!   run a tokio executor. The RT thread does not touch tokio either.
//! - **No `.await` in the RT path.** [`RtInputProducer::try_push`] and
//!   [`RtOutputConsumer::try_pop`] are wait-free lock-free.
//! - **No shared lock between RT and worker.** `rtrb` rings are
//!   single-producer / single-consumer — the only synchronization is
//!   an atomic head/tail pair.
//! - **No hidden allocation steady-state.** A `RuntimeData::Audio`
//!   backed by an `AudioSamples::Pooled` travels as a fat pointer
//!   through the ring; the backing buffer returns to the pool on drop
//!   on the worker thread. Nothing in the bridge allocates per packet.
//!
//! # What it does *not* guarantee
//!
//! - The bridge is only as RT-safe as the node you put in it. If
//!   `node.process()` allocates, takes a lock, or calls a blocking
//!   syscall, the worker thread will hiccup — but the RT thread is
//!   protected, and the hiccup surfaces as a full input ring (drop
//!   in `try_push`) or an empty output ring (silence in `try_pop`),
//!   never as a parked audio callback.
//! - The `realtime` feature enables `thread_priority` and
//!   `core_affinity` on the worker. On Linux this requires
//!   `CAP_SYS_NICE` or suitable rlimit; on macOS it requires the
//!   RT entitlement (`com.apple.security.app-sandbox` +
//!   `com.apple.security.realtime-audio`). Fall back to the default
//!   feature-off build if those aren't available.
//!
//! # Public surface
//!
//! - [`RtBridge`] — owns the worker thread.
//! - [`RtInputProducer`] — **RT-safe** handle for pushing audio.
//! - [`RtOutputConsumer`] — **RT-safe** handle for pulling results.
//! - [`RtBridgeConfig`] — ring capacities, worker thread name, and
//!   (with the `realtime` feature) priority and core pinning.
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_rt_bridge::{RtBridge, RtBridgeConfig};
//! use remotemedia_core::data::RuntimeData;
//! use remotemedia_core::nodes::SyncStreamingNode;
//!
//! let node: Box<dyn SyncStreamingNode> = Box::new(MyDspNode::new());
//! let (_bridge, mut producer, mut consumer) =
//!     RtBridge::spawn(node, RtBridgeConfig::default());
//!
//! // Inside the HAL IO callback:
//! fn io_callback(input: RuntimeData, producer: &mut _, consumer: &mut _) {
//!     let _ = producer.try_push(input);          // wait-free
//!     if let Some(output) = consumer.try_pop() { // wait-free
//!         // route output back to HAL speaker buffer
//!     }
//! }
//! ```

#![warn(clippy::all)]
#![warn(missing_docs)]

mod bridge;
mod error;
mod handles;
mod worker;

#[cfg(feature = "realtime")]
mod realtime;

pub use bridge::{RtBridge, RtBridgeConfig, RtBridgeStats};
pub use error::{SpawnError, TryPushError};
pub use handles::{RtInputProducer, RtOutputConsumer};
