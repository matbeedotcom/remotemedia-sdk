# WebRTC Panel — Design Spec

## Goal

Implement a functional WebRTC panel in the embedded UI that connects to the WebSocket signaling server, establishes an RTCPeerConnection with `remotemedia-server`, and supports real-time audio via media tracks and arbitrary data via a data channel — all simultaneously on a single connection.

## Architecture

```
Browser (WebRTC Panel)
  │
  ├─ WebSocket ──────────────── WS Signaling Server (--signal-port)
  │   (JSON-RPC 2.0)             peer.announce / offer / answer / ice
  │
  └─ RTCPeerConnection ──────── ServerPeer (remotemedia-server)
      ├─ Media Track (audio) ──── getUserMedia → addTrack → pipeline
      └─ Data Channel ("pipeline") ── binary wire format → pipeline
```

### Connection flow

1. Frontend fetches `/api/status` → gets `transport_type: "webrtc"` and `address`
2. Opens WebSocket to `ws://${address}/ws`
3. Sends `peer.announce` with capabilities `["audio", "data"]`
4. Creates `RTCPeerConnection` with STUN servers
5. Creates data channel `"pipeline"` (reliable, ordered)
6. Creates SDP offer → sends via `peer.offer` to `"remotemedia-server"`
7. Receives SDP answer → `setRemoteDescription`
8. Exchanges ICE candidates via `peer.ice_candidate`
9. Connection established — data channel opens, audio tracks ready
10. When user enables mic: `getUserMedia({audio: true})` → `addTrack()` (renegotiation)

### Audio path

- **Input**: `getUserMedia({audio: true})` → `MediaStreamTrack` added to peer connection
- **Output**: `ontrack` event → `MediaStreamTrack` → `<audio>` element for playback
- Audio flows through WebRTC's native Opus codec / RTP / jitter buffer
- Pipeline processes audio via `ServerPeer` media track handler

### Data channel path

- **Wire format**: Same binary format as `data_transfer.rs` (NAPI/iceoryx2 IPC):
  ```
  ┌──────────┬─────────────┬────────────┬──────────────┬─────────────┬─────────┐
  │ data_type│ session_len  │ session_id │ timestamp_ns │ payload_len │ payload │
  │ 1 byte   │ 2 bytes (LE) │ N bytes    │ 8 bytes (LE) │ 4 bytes (LE)│ M bytes │
  └──────────┴─────────────┴────────────┴──────────────┴─────────────┴─────────┘
  ```
- **DataType discriminants** (matching Rust enum):
  - 1=Audio, 2=Video, 3=Text, 4=Tensor, 5=ControlMessage, 6=Numpy, 7=File
- **No JSON serialization** — binary WebSocket messages / data channel binary mode
- TypeScript encoder/decoder matches the wire format (~50 lines)

### Supported data types

`sendData` accepts any `DataType` variant:

| Type | Payload format |
|------|---------------|
| Audio | PCM samples (f32 LE) prefixed by 16-byte header (sample_rate, channels, num_samples) |
| Video | Pixel data prefixed by 19-byte header (width, height, format, codec, frame_num, keyframe) |
| Text | Raw UTF-8 bytes |
| Tensor | Raw bytes + shape/dtype header |
| ControlMessage | Type-specific binary |
| Numpy | Raw array bytes + shape/strides/dtype |
| File | Path + metadata as length-prefixed strings |

## CLI Changes

Rename `--ws-port` to `--signal-port` and add `--signal-type`:

```
remotemedia serve pipeline.json --transport webrtc \
  --port 18080 \
  --signal-port 18091 \
  --signal-type websocket \   # default: websocket, alt: grpc
  --ui --ui-port 3001
```

- `--signal-port` — port for the signaling server (browser-accessible)
- `--signal-type` — signaling protocol (`websocket` default, `grpc` for gRPC-Web)
- `transport.address` in status API returns the signal port address

## UI Panel Layout

All sections active simultaneously on one `RTCPeerConnection`:

```
┌─────────────────────────────────────────────┐
│ WebRTC Real-Time                            │
├─────────────────────────────────────────────┤
│ [Connect]  ● Connected  peer: browser-xyz   │
├─────────────────────────────────────────────┤
│ Audio                                       │
│ [🎤 Start Mic]                              │
│ ┌─────────────────────────────────────────┐ │
│ │ ▁▂▃▅▇▅▃▂▁▂▃▅▇▅▃▂  (input waveform)    │ │
│ └─────────────────────────────────────────┘ │
│ Output: 🔊 ▶ playing...                    │
├─────────────────────────────────────────────┤
│ Data Channel                                │
│ ┌─────────────────────────────────────────┐ │
│ │ > hello world                           │ │
│ │ < hello world                (passthru) │ │
│ └─────────────────────────────────────────┘ │
│ [________________] [Send]                   │
└─────────────────────────────────────────────┘
```

## Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `frontend/src/lib/webrtc.ts` | Rewrite | `WebRtcClient` class: signaling WS, RTCPeerConnection, data channel, media tracks |
| `frontend/src/lib/wire-format.ts` | Create | Binary encoder/decoder matching `data_transfer.rs` wire format |
| `frontend/src/components/WebRtcPanel.tsx` | Rewrite | Full panel: connection controls, audio section, data channel section |
| `frontend/src/components/AudioVisualizer.tsx` | Create | Canvas-based waveform for mic input |
| `cli/src/commands/serve.rs` | Modify | `--signal-port`, `--signal-type` flags (rename `--ws-port`) |
| `crates/ui/src/lib.rs` | Modify | No changes needed — `TransportInfo.address` already carries the right value |

## WebRtcClient API

```typescript
class WebRtcClient {
  // Lifecycle
  connect(signalingUrl: string, peerId?: string): Promise<void>
  disconnect(): void
  readonly state: 'disconnected' | 'connecting' | 'connected' | 'failed'

  // Audio (media tracks)
  enableMic(): Promise<MediaStream>
  disableMic(): void
  readonly micEnabled: boolean

  // Data channel (binary wire format)
  sendText(text: string): void
  sendAudio(samples: Float32Array, sampleRate: number, channels: number): void
  sendBinary(data: Uint8Array, mimeType?: string): void
  sendRaw(dataType: DataType, payload: Uint8Array): void

  // Events
  onStateChange: (state: string) => void
  onAudioTrack: (track: MediaStreamTrack) => void
  onData: (dataType: DataType, payload: Uint8Array, sessionId: string) => void
  onPeerJoined: (peerId: string) => void
  onPeerLeft: (peerId: string) => void
}
```

## Wire Format Module

```typescript
// wire-format.ts — matches data_transfer.rs

enum DataType {
  Audio = 1, Video = 2, Text = 3, Tensor = 4,
  ControlMessage = 5, Numpy = 6, File = 7
}

function encode(dataType: DataType, sessionId: string, payload: Uint8Array): ArrayBuffer
function decode(buffer: ArrayBuffer): { dataType: DataType, sessionId: string, timestampNs: bigint, payload: Uint8Array }
```

## E2E Test Coverage

Extend existing Playwright tests in `crates/ui/e2e/tests/webrtc-signaling.spec.ts`:

| Test | Description |
|------|-------------|
| Panel renders connect button | WebRTC tab shows "Connect" when `transport_type=webrtc` |
| Connect establishes peer connection | Click Connect → status shows "Connected" |
| Data channel text roundtrip | Type text → Send → receive passthrough output |
| Data channel binary roundtrip | Send binary DataType → receive back |
| Disconnect cleans up | Click Disconnect → status shows "Disconnected" |
| Mic toggle shows visualizer | Enable mic → visualizer canvas appears |
| Audio track received | Pipeline output audio track triggers playback element |

Note: `getUserMedia` requires Playwright's `--use browserContext.permissions=["microphone"]` or mocking.

## Out of Scope

- Video tracks (can be added later with same pattern)
- gRPC-Web signaling implementation (flag exists, implementation deferred)
- Protobuf codegen (using existing binary wire format instead)
