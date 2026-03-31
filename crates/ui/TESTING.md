# remotemedia-ui Testing

## Overview

The UI crate has two test layers:

| Layer | Location | Runner | What it tests |
|-------|----------|--------|---------------|
| Rust integration | `tests/webrtc_ui_integration.rs` | `cargo test` | HTTP API, streaming I/O (SSE), session lifecycle |
| Browser E2E | `e2e/tests/*.spec.ts` | Playwright | Full browser rendering, user interaction, WebRTC signaling |

## Rust Integration Tests

Fast, headless tests that start the UI server in-process and exercise the HTTP API
with `reqwest`. No browser or CLI binary required.

```bash
# Run all 13 tests
cargo test -p remotemedia-ui --test webrtc_ui_integration

# With output
cargo test -p remotemedia-ui --test webrtc_ui_integration -- --nocapture
```

### Test coverage

| Group | Tests | Description |
|-------|-------|-------------|
| Status API | 2 | `/api/status` with and without transport info |
| Manifest API | 2 | `/api/manifest` present and missing |
| Pipeline Execution | 3 | Text passthrough, inline manifest, missing manifest error |
| Static Assets | 2 | Index HTML serving, SPA fallback routing |
| Streaming Input/Output | 3 | Text passthrough via SSE, multi-message ordering, invalid session 404 |
| Session Lifecycle | 1 | Create session, verify count, close, verify cleanup |

### Architecture note

The streaming tests work because `api.rs` uses a **background drain task** per session.
When a session is created, a `tokio::spawn` task owns the `SessionHandleWrapper` and uses
`tokio::select!` to concurrently forward input from an `mpsc` channel and drain output
to a `broadcast` channel. The HTTP input handler only needs a read lock and does a
non-blocking channel send, while SSE subscribers receive via the broadcast channel.

## Browser E2E Tests (Playwright)

Full end-to-end tests that launch a real Chromium browser against a running CLI server
with the embedded web UI and WebRTC WebSocket signaling.

### Prerequisites

```bash
cd crates/ui/e2e
npm install
npx playwright install chromium
```

### Running

```bash
cd crates/ui/e2e

# Run all 32 tests (auto-starts the CLI server)
npm test

# Run with visible browser
npm run test:headed

# Interactive Playwright UI
npm run test:ui

# Debug mode (step through tests)
npm run test:debug

# Run a single test file
npx playwright test tests/ui-e2e.spec.ts
npx playwright test tests/webrtc-signaling.spec.ts

# Run a single test by name
npx playwright test -g "executes text passthrough via UI"
```

### Server startup

Playwright auto-starts the CLI via its `webServer` config:

```
cargo run --features ui,webrtc -- serve fixtures/passthrough.json \
  --transport webrtc --port 18080 --signal-port 18091 --ui --ui-port 3001
```

This starts three servers:
- **UI server** on `:3001` (Axum, serves embedded Preact frontend + HTTP API)
- **gRPC signaling** on `:18080` (tonic, WebRTC peer management)
- **WebSocket signaling** on `:18091` (JSON-RPC 2.0, browser-accessible)

The first run triggers a full `cargo build --features ui,webrtc` (~2-3 min).
Subsequent runs reuse the binary. Set `reuseExistingServer: false` in CI.

### Port overrides

```bash
UI_PORT=4000 SIGNAL_PORT=19000 npm test
```

### Test coverage

#### `ui-e2e.spec.ts` — 18 tests

| Group | Tests | Description |
|-------|-------|-------------|
| Status | 3 | API endpoint, connected indicator, session count |
| Navigation | 3 | Default tab, switch to Manifest, switch back |
| Manifest | 2 | Node card rendering, API response |
| Pipeline Execution | 5 | Text passthrough, JSON input, invalid JSON error, loading state, multiple executions |
| Pipeline API | 2 | Execute endpoint, stream session lifecycle |
| Streaming I/O | 2 | SSE single message, SSE multi-message ordering |
| SPA Routing | 1 | Deep path serves index.html |

#### `webrtc-signaling.spec.ts` — 17 tests

| Group | Tests | Description |
|-------|-------|-------------|
| WebRTC UI Tab | 2 | Tab visible when `transport_type=webrtc`, panel shows signaling address |
| Signaling Connection | 1 | Browser WebSocket connects to `ws://host:port/ws` |
| Peer Announce | 2 | Successful registration, duplicate peer rejection |
| Peer List | 1 | `peer.list` returns registered peers |
| Multi-Peer | 2 | `peer.joined` notification, `other_peers` in announce response |
| SDP Offer/Answer | 2 | Valid offer to `remotemedia-server`, invalid SDP error (-32002) |
| Error Handling | 3 | Unknown method (-32601), invalid JSON (-32700), pre-announce operations |
| Disconnect | 1 | `peer.left` notification on WebSocket close |
| Panel Interaction | 3 | Connect/disconnect lifecycle, data channel text send, UI cleanup |

### WebRTC signaling protocol (JSON-RPC 2.0)

The WebSocket signaling tests exercise the full JSON-RPC 2.0 protocol on `ws://host:port/ws`:

```
peer.announce  →  Register peer, get other_peers list
peer.list      →  Query all connected peers
peer.offer     →  Send SDP offer (to peer or "remotemedia-server")
peer.answer    →  Send SDP answer
peer.ice_candidate → Exchange ICE candidates
peer.disconnect →  Explicit peer disconnect
```

Notifications (server-initiated):
```
peer.joined       →  Broadcast when a new peer announces
peer.left         →  Broadcast when a peer disconnects
peer.state_change →  Connection state transitions
```

Error codes:
```
-32700  PARSE_ERROR
-32601  METHOD_NOT_FOUND
-32602  INVALID_PARAMS
-32000  PEER_NOT_FOUND
-32002  OFFER_INVALID (SDP validation failed)
```

## Rebuilding the frontend

If you modify frontend source in `crates/ui/frontend/src/`, rebuild before testing:

```bash
cd crates/ui/frontend && npm run build
```

The Rust `build.rs` embeds `dist/` at compile time. After rebuilding the frontend,
you must also rebuild the Rust crate:

```bash
cargo clean -p remotemedia-ui && cargo build -p remotemedia-ui
```

For E2E tests, also rebuild the CLI:

```bash
cd examples/cli/remotemedia-cli && cargo build --features ui,webrtc
```
