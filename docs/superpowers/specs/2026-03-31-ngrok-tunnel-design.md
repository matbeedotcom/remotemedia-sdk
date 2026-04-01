# ngrok Tunnel Support for CLI Serve Command

## Problem

Firefox requires a secure context (HTTPS) for WebRTC APIs and does not support
ICE gathering on loopback addresses. Developers testing the embedded web UI need
an HTTPS URL with a real hostname. Currently the only workaround is accessing via
LAN IP, which still lacks HTTPS.

## Solution

Add a `--tunnel` flag to `remotemedia serve` that creates an ngrok HTTPS tunnel
to the UI server port. The WebSocket signaling handler is mounted on the UI
server's axum router at `/ws`, so a single tunnel covers both the frontend and
signaling — one URL, one port.

## Architecture

### Single-Port Merged Server

When `--tunnel` (or `--transport webrtc --ui` without a separate `--signal-port`)
is used, the WS signaling handler runs as an axum route on the UI server:

```
Browser ──HTTPS──▶ ngrok ──HTTP──▶ localhost:3001 (UI server)
                                    ├─ GET /           → SPA frontend
                                    ├─ GET /api/status → transport info
                                    ├─ POST /api/...   → pipeline API
                                    └─ GET /ws         → WebSocket signaling (axum upgrade)
```

The standalone `WebSocketSignalingServer` (dedicated thread + runtime) remains
available for FFI consumers — this design does not modify it.

### Unified Signaling Handler (No Divergence)

To avoid two diverging WebSocket implementations, the full connection lifecycle
is extracted into a transport-agnostic function in `handler.rs`:

```rust
pub async fn run_signaling_session(
    ws_sender: impl Sink<String, Error = E> + Unpin,
    ws_receiver: impl Stream<Item = Result<WsMessage, E>> + Unpin,
    state: Arc<SharedState>,
) -> Result<()>
```

Where `WsMessage` is a simple enum abstracting over text/binary/ping/pong/close
frames (or we use the existing `tokio_tungstenite::Message` as the common type
and adapt axum's `Message` to it).

This function manages the **full connection lifecycle**, not just message
dispatch:
- Creates the internal `mpsc::channel` for per-connection outbound messages
- Spawns the forward task that drains the channel into the WS sink
- Manages per-connection `peer_id: Arc<RwLock<Option<String>>>` state
- Calls `handle_message` for each incoming text frame
- Performs cleanup on disconnect (event emission, ServerPeer shutdown, peer
  removal, broadcast)

`handle_message` and all `handle_*` sub-functions remain **private** — they
continue to use `mpsc::Sender<String>` internally. Only
`run_signaling_session` and `SharedState` are public exports.

Both the standalone server (`tokio-tungstenite`) and the axum handler
(`axum::extract::ws`) call this function with thin adapters that convert between
their respective WebSocket message types.

### ngrok Integration

The `ngrok` crate (v0.18, official Rust SDK) provides native tunnel creation
without an external binary. It connects outbound to ngrok infrastructure and
receives an HTTPS URL.

```rust
let session = ngrok::Session::builder()
    .authtoken_from_env()  // NGROK_AUTHTOKEN
    .connect()
    .await?;

let listener = session
    .http_endpoint()
    .listen_and_forward(Url::parse(&format!("http://localhost:{}", ui_port))?)
    .await?;

println!("Tunnel URL: {}", listener.url());
```

The tunnel starts **after** the UI server binds, and runs as a background tokio
task. On ctrl-c, a shared `tokio_util::sync::CancellationToken` triggers both
the UI server and the ngrok session to shut down in coordinated fashion.

**Auth token:** Read from `NGROK_AUTHTOKEN` env var. The CLI uses `dotenvy` to
load `.env` files from the project root before checking the env var. If missing,
the CLI prints a clear error directing the user to
https://dashboard.ngrok.com for a free token.

### Frontend Changes

`WebRtcPanel.tsx` `getSignalingUrl()` changes from hardcoded `ws://` to
protocol-relative:

```typescript
function getSignalingUrl(address: string): string {
  const isSecure = window.location.protocol === 'https:';
  const protocol = isSecure ? 'wss' : 'ws';
  // When behind a tunnel (HTTPS), use window.location.host which includes
  // the correct port (443 implicit for HTTPS). When local, substitute
  // 0.0.0.0 with the actual hostname and keep the explicit port.
  if (isSecure) {
    return `${protocol}://${window.location.host}/ws`;
  }
  const host = address.replace('0.0.0.0', window.location.hostname);
  return `${protocol}://${host}/ws`;
}
```

When tunneling, the signaling URL is derived entirely from `window.location`
(e.g., `wss://abc123.ngrok.app/ws`) — no port mismatch because ngrok uses
standard HTTPS port 443. When running locally without a tunnel, the existing
`address`-based logic with explicit port is preserved.

The Firefox loopback warning naturally hides itself since `isLoopback()` returns
false for ngrok URLs.

### TransportInfo Update After Tunnel Establishment

The ngrok tunnel URL is not known until after the tunnel connects, which happens
after the UI server starts. To handle this:

- `AppState.transport_info` is changed to `Arc<RwLock<Option<TransportInfo>>>`
- After the tunnel is established, `serve.rs` updates the transport info with
  the tunnel URL
- The `/api/status` endpoint reads the current value from the lock
- The frontend's `getSignalingUrl` uses `window.location` for HTTPS anyway,
  so the transport info is primarily informational when tunneling

## CLI Interface

```
remotemedia serve manifest.yaml --transport webrtc --ui --tunnel
```

**New flag:**
- `--tunnel` — Enable ngrok HTTPS tunnel (requires `NGROK_AUTHTOKEN` env var)

**Behavior:**
- `--tunnel` without `--ui` → error: `"--tunnel requires --ui"`
- `--tunnel` without `--transport webrtc` → error: `"--tunnel requires --transport webrtc"`
- `--tunnel` with `--signal-port` → warning: `"--signal-port ignored when --tunnel is active (signaling served on UI port)"`
- `--tunnel` without `NGROK_AUTHTOKEN` → error with signup link

**Output:**
```
Starting WebRTC server on 0.0.0.0:8080 with pipeline manifest.yaml
Web UI available at http://0.0.0.0:3001
Tunnel URL: https://abc123.ngrok.app
```

## Feature Flags & Dependencies

### `remotemedia-webrtc` (crates/transports/webrtc)

- Make `handler` module public in `websocket/mod.rs`
- Export `run_signaling_session` and `SharedState` (handle_message remains private)
- Refactor `handle_connection` to call `run_signaling_session` internally

### `remotemedia-ui` (crates/ui)

```toml
# Cargo.toml
remotemedia-webrtc = { workspace = true, features = ["ws-signaling"], optional = true }
axum = { version = "0.7", features = ["tokio", "json", "http2", "ws"] }

[features]
webrtc = ["dep:remotemedia-webrtc"]
```

Note: enabling `webrtc` on `remotemedia-ui` transitively pulls in tonic, prost,
and other gRPC dependencies via `ws-signaling`. This is acceptable for the
development/testing CLI use case. The UI crate's default features do not include
`webrtc`, so library consumers are unaffected.

- New `ws_signaling.rs` module (feature-gated on `webrtc`)
- Axum WebSocket handler at `/ws` delegating to `run_signaling_session`
- `AppState.transport_info` changes to `Arc<RwLock<Option<TransportInfo>>>`
- `AppState` gains `Option<Arc<SharedState>>` for signaling state
- `/ws` route added to router only when signaling state is present

### `remotemedia-cli` (examples/cli/remotemedia-cli)

```toml
# Cargo.toml
ngrok = { version = "0.18", optional = true }
dotenvy = { version = "0.15", optional = true }

[features]
tunnel = ["dep:ngrok", "dep:dotenvy"]
```

- `ServeArgs` gains `--tunnel` bool flag
- `serve.rs` loads `.env` via dotenvy, wires up ngrok session after UI server starts
- Passes `SignalingState` to `UiServerBuilder` when tunnel or merged mode active
- Updates `AppState.transport_info` after tunnel URL is known

## Files Changed

| File | Change |
|------|--------|
| `crates/transports/webrtc/src/signaling/websocket/mod.rs` | Make `handler` pub, export new function |
| `crates/transports/webrtc/src/signaling/websocket/handler.rs` | Extract `run_signaling_session` from `handle_connection` |
| `crates/transports/webrtc/src/signaling/websocket/server.rs` | Call `run_signaling_session` (no behavior change) |
| `crates/transports/webrtc/src/signaling/mod.rs` | Update re-exports for new public items |
| `crates/ui/Cargo.toml` | Add optional `remotemedia-webrtc` dep, `ws` feature on axum |
| `crates/ui/src/lib.rs` | Add optional signaling state, `/ws` route, `Arc<RwLock<>>` transport info |
| `crates/ui/src/ws_signaling.rs` | New: axum WS handler |
| `crates/ui/frontend/src/components/WebRtcPanel.tsx` | Protocol-relative WS URL |
| `examples/cli/remotemedia-cli/Cargo.toml` | Add optional `ngrok` + `dotenvy` deps, `tunnel` feature |
| `examples/cli/remotemedia-cli/src/commands/serve.rs` | `--tunnel` flag, ngrok setup, merged server wiring |

## Testing

- Unit: `run_signaling_session` tested with mock sink/stream (existing JSON-RPC tests adapt)
- Integration: `cargo test -p remotemedia-ui --features webrtc` — WS signaling via axum
- E2E (merged server): Playwright tests with merged WS-on-UI-port mode (no tunnel, fully automated)
- E2E (tunnel): Playwright tests with `--tunnel` flag (manual — requires ngrok token)
- Regression: existing e2e tests pass without `--tunnel` (standalone signaling path unchanged)

## Performance Considerations

The axum WS handler runs on the same tokio runtime as the UI server. For the CLI
serve use case this is appropriate — the dedicated-thread design in
`WebSocketSignalingServer` exists for FFI contexts (napi-rs) where the main
runtime may not poll consistently. The CLI's tokio runtime does not have this
issue.

If benchmarks show contention under high peer counts, the axum handler could be
moved to a dedicated tokio task with its own runtime, but this is not expected
to be necessary for the development/testing use case this feature targets.
