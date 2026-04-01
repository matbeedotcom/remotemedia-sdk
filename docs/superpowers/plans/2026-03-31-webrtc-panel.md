# WebRTC Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a functional WebRTC panel in the embedded UI that connects to the WS signaling server, establishes an RTCPeerConnection with `remotemedia-server`, and supports real-time audio via media tracks and text/binary data via data channel.

**Architecture:** Browser connects to WS signaling server (JSON-RPC 2.0), negotiates an RTCPeerConnection with `remotemedia-server` via SDP offer/answer + ICE, then multiplexes audio media tracks and a binary data channel over the single connection. Data channel uses the same binary wire format as `data_transfer.rs`.

**Tech Stack:** Preact, TypeScript, WebRTC API, WebSocket API, Canvas API (audio visualizer)

**Spec:** `docs/superpowers/specs/2026-03-31-webrtc-panel-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/ui/frontend/src/lib/wire-format.ts` | Create | Binary encoder/decoder matching `data_transfer.rs` wire format |
| `crates/ui/frontend/src/lib/webrtc.ts` | Rewrite | `WebRtcClient` class: WS signaling, RTCPeerConnection, data channel, media tracks |
| `crates/ui/frontend/src/components/AudioVisualizer.tsx` | Create | Canvas-based waveform display for audio input/output |
| `crates/ui/frontend/src/components/WebRtcPanel.tsx` | Rewrite | Full panel UI: connection controls, audio section, data channel section |
| `examples/cli/remotemedia-cli/src/commands/serve.rs` | Modify | Rename `--ws-port` to `--signal-port`, add `--signal-type`, update `transport.address` to use signal port |
| `crates/ui/e2e/playwright.config.ts` | Modify | Update CLI command from `--ws-port` to `--signal-port` |
| `crates/ui/e2e/tests/webrtc-signaling.spec.ts` | Modify | Update `WS_PORT` env var name, add panel interaction tests |

---

### Task 1: Wire Format Module

Binary encoder/decoder matching the `data_transfer.rs` wire format used by NAPI/iceoryx2 IPC. This is the foundation — the data channel sends and receives these binary messages.

**Reference:** `crates/transports/ffi/src/napi/runtime_data.rs:10-37` (wire format docs + DataType enum)

**Files:**
- Create: `crates/ui/frontend/src/lib/wire-format.ts`

- [ ] **Step 1: Create wire-format.ts with DataType enum and encode/decode functions**

```typescript
// crates/ui/frontend/src/lib/wire-format.ts

/** Data type discriminants — must match Rust RuntimeData enum in data_transfer.rs */
export enum DataType {
  Audio = 1,
  Video = 2,
  Text = 3,
  Tensor = 4,
  ControlMessage = 5,
  Numpy = 6,
  File = 7,
}

/** Audio payload header size: sample_rate(4) + channels(2) + padding(2) + num_samples(8) = 16 */
export const AUDIO_HEADER_SIZE = 16;

export interface DecodedMessage {
  dataType: DataType;
  sessionId: string;
  timestampNs: bigint;
  payload: Uint8Array;
}

export interface AudioHeader {
  sampleRate: number;
  channels: number;
  numSamples: number;
}

/**
 * Encode a RuntimeData message into the binary wire format.
 *
 * Wire format (little-endian):
 *   [data_type: 1B] [session_len: 2B] [session_id: NB] [timestamp_ns: 8B] [payload_len: 4B] [payload: MB]
 */
export function encode(
  dataType: DataType,
  sessionId: string,
  payload: Uint8Array,
): ArrayBuffer {
  const sessionBytes = new TextEncoder().encode(sessionId);
  const totalLen = 1 + 2 + sessionBytes.length + 8 + 4 + payload.length;
  const buffer = new ArrayBuffer(totalLen);
  const view = new DataView(buffer);
  const bytes = new Uint8Array(buffer);

  let offset = 0;

  // data_type (1 byte)
  view.setUint8(offset, dataType);
  offset += 1;

  // session_len (2 bytes LE)
  view.setUint16(offset, sessionBytes.length, true);
  offset += 2;

  // session_id (N bytes)
  bytes.set(sessionBytes, offset);
  offset += sessionBytes.length;

  // timestamp_ns (8 bytes LE) — current time in nanoseconds
  const nowNs = BigInt(Date.now()) * 1_000_000n;
  view.setBigUint64(offset, nowNs, true);
  offset += 8;

  // payload_len (4 bytes LE)
  view.setUint32(offset, payload.length, true);
  offset += 4;

  // payload (M bytes)
  bytes.set(payload, offset);

  return buffer;
}

/**
 * Decode a binary wire format message into its components.
 */
export function decode(buffer: ArrayBuffer): DecodedMessage {
  const view = new DataView(buffer);
  const bytes = new Uint8Array(buffer);

  if (buffer.byteLength < 15) {
    throw new Error(`Buffer too small: ${buffer.byteLength} bytes (min 15)`);
  }

  let offset = 0;

  const dataType: DataType = view.getUint8(offset);
  offset += 1;

  const sessionLen = view.getUint16(offset, true);
  offset += 2;

  if (buffer.byteLength < 3 + sessionLen + 12) {
    throw new Error(`Buffer too small for session + header`);
  }

  const sessionId = new TextDecoder().decode(bytes.slice(offset, offset + sessionLen));
  offset += sessionLen;

  const timestampNs = view.getBigUint64(offset, true);
  offset += 8;

  const payloadLen = view.getUint32(offset, true);
  offset += 4;

  const payload = bytes.slice(offset, offset + payloadLen);

  return { dataType, sessionId, timestampNs, payload };
}

/** Encode a text string as a Text DataType message */
export function encodeText(sessionId: string, text: string): ArrayBuffer {
  const payload = new TextEncoder().encode(text);
  return encode(DataType.Text, sessionId, payload);
}

/** Decode text payload from a Text DataType message */
export function decodeText(payload: Uint8Array): string {
  return new TextDecoder().decode(payload);
}

/** Encode PCM audio samples as an Audio DataType message */
export function encodeAudio(
  sessionId: string,
  samples: Float32Array,
  sampleRate: number,
  channels: number,
): ArrayBuffer {
  // Audio header: sample_rate(4) + channels(2) + padding(2) + num_samples(8)
  const header = new ArrayBuffer(AUDIO_HEADER_SIZE);
  const headerView = new DataView(header);
  headerView.setUint32(0, sampleRate, true);
  headerView.setUint16(4, channels, true);
  // padding 2 bytes (zeroed)
  headerView.setBigUint64(8, BigInt(samples.length), true);

  // Combine header + sample bytes
  const sampleBytes = new Uint8Array(samples.buffer, samples.byteOffset, samples.byteLength);
  const payload = new Uint8Array(AUDIO_HEADER_SIZE + sampleBytes.length);
  payload.set(new Uint8Array(header), 0);
  payload.set(sampleBytes, AUDIO_HEADER_SIZE);

  return encode(DataType.Audio, sessionId, payload);
}

/** Decode audio header from an Audio payload */
export function decodeAudioHeader(payload: Uint8Array): AudioHeader {
  if (payload.length < AUDIO_HEADER_SIZE) {
    throw new Error(`Audio payload too small: ${payload.length} bytes`);
  }
  const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
  return {
    sampleRate: view.getUint32(0, true),
    channels: view.getUint16(4, true),
    numSamples: Number(view.getBigUint64(8, true)),
  };
}
```

- [ ] **Step 2: Build frontend to verify no TypeScript errors**

Run: `cd crates/ui/frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add crates/ui/frontend/src/lib/wire-format.ts
git commit -m "feat(ui): add binary wire format encoder/decoder for data channel"
```

---

### Task 2: WebRtcClient Class

The core signaling + RTCPeerConnection logic. Connects to the WS signaling server, manages peer connection lifecycle, data channel, and audio tracks.

**Reference:**
- `crates/transports/webrtc/src/signaling/websocket/handler.rs:242-809` (server-side signaling protocol)
- `crates/transports/webrtc/src/signaling/protocol.rs:69-110` (error codes)

**Files:**
- Rewrite: `crates/ui/frontend/src/lib/webrtc.ts`

- [ ] **Step 1: Write WebRtcClient class**

```typescript
// crates/ui/frontend/src/lib/webrtc.ts

import { DataType, encode, decode, encodeText, encodeAudio, type DecodedMessage } from './wire-format';

export type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'failed';

export interface WebRtcClientEvents {
  onStateChange?: (state: ConnectionState) => void;
  onAudioTrack?: (track: MediaStreamTrack, stream: MediaStream) => void;
  onData?: (msg: DecodedMessage) => void;
  onPeerJoined?: (peerId: string) => void;
  onPeerLeft?: (peerId: string) => void;
  onError?: (error: string) => void;
}

export class WebRtcClient {
  private ws: WebSocket | null = null;
  private pc: RTCPeerConnection | null = null;
  private dataChannel: RTCDataChannel | null = null;
  private localStream: MediaStream | null = null;
  private peerId: string;
  private sessionId: string;
  private pendingIceCandidates: RTCIceCandidateInit[] = [];
  private _state: ConnectionState = 'disconnected';
  private _micEnabled = false;
  private events: WebRtcClientEvents;
  private rpcId = 0;

  constructor(events: WebRtcClientEvents = {}) {
    this.events = events;
    this.peerId = `browser-${Math.random().toString(36).slice(2, 8)}`;
    this.sessionId = `session-${Date.now()}`;
  }

  get state(): ConnectionState { return this._state; }
  get micEnabled(): boolean { return this._micEnabled; }

  private setState(state: ConnectionState) {
    this._state = state;
    this.events.onStateChange?.(state);
  }

  private nextRpcId(): string {
    return `rpc-${++this.rpcId}`;
  }

  /** Send a JSON-RPC 2.0 request over the signaling WebSocket */
  private sendRpc(method: string, params: Record<string, unknown>): string {
    const id = this.nextRpcId();
    this.ws?.send(JSON.stringify({ jsonrpc: '2.0', method, params, id }));
    return id;
  }

  /** Wait for a JSON-RPC response with the given ID */
  private waitForRpcResponse(id: string, timeoutMs = 10000): Promise<any> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.ws?.removeEventListener('message', handler);
        reject(new Error(`RPC timeout for ${id}`));
      }, timeoutMs);

      const handler = (event: MessageEvent) => {
        const msg = JSON.parse(event.data);
        if (msg.id === id) {
          clearTimeout(timeout);
          this.ws?.removeEventListener('message', handler);
          if (msg.error) {
            reject(new Error(msg.error.message || 'RPC error'));
          } else {
            resolve(msg.result);
          }
        }
      };
      this.ws?.addEventListener('message', handler);
    });
  }

  /** Connect to the signaling server and establish WebRTC peer connection */
  async connect(signalingUrl: string): Promise<void> {
    if (this._state !== 'disconnected') return;
    this.setState('connecting');

    try {
      // 1. Connect WebSocket
      await this.connectWebSocket(signalingUrl);

      // 2. Announce peer
      const announceId = this.sendRpc('peer.announce', {
        peer_id: this.peerId,
        capabilities: ['audio', 'data'],
        user_data: { display_name: 'Browser UI' },
      });
      await this.waitForRpcResponse(announceId);

      // 3. Create RTCPeerConnection
      this.pc = new RTCPeerConnection({
        iceServers: [{ urls: 'stun:stun.l.google.com:19302' }],
      });

      this.setupPeerConnectionHandlers();

      // 4. Create data channel
      this.dataChannel = this.pc.createDataChannel('pipeline', {
        ordered: true,
      });
      this.dataChannel.binaryType = 'arraybuffer';
      this.setupDataChannelHandlers(this.dataChannel);

      // 5. Create and send offer
      const offer = await this.pc.createOffer();
      await this.pc.setLocalDescription(offer);

      const offerId = this.sendRpc('peer.offer', {
        from: this.peerId,
        to: 'remotemedia-server',
        sdp: offer.sdp,
      });
      const answerResult = await this.waitForRpcResponse(offerId);

      // 6. Set remote description (answer)
      await this.pc.setRemoteDescription({
        type: 'answer',
        sdp: answerResult.sdp,
      });

      // 7. Flush any pending ICE candidates
      for (const candidate of this.pendingIceCandidates) {
        await this.pc.addIceCandidate(candidate);
      }
      this.pendingIceCandidates = [];

      this.setState('connected');
    } catch (err) {
      this.setState('failed');
      this.events.onError?.(err instanceof Error ? err.message : String(err));
      throw err;
    }
  }

  private connectWebSocket(url: string): Promise<void> {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(url);
      const timeout = setTimeout(() => {
        reject(new Error('WebSocket connection timeout'));
      }, 10000);

      this.ws.onopen = () => {
        clearTimeout(timeout);
        resolve();
      };
      this.ws.onerror = () => {
        clearTimeout(timeout);
        reject(new Error('WebSocket connection failed'));
      };
      this.ws.onclose = () => {
        if (this._state === 'connected') {
          this.setState('disconnected');
        }
      };

      // Handle signaling notifications
      this.ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);
        // Only handle notifications (no id = server-initiated)
        if (!msg.id && msg.method) {
          this.handleNotification(msg.method, msg.params);
        }
      };
    });
  }

  private handleNotification(method: string, params: any) {
    switch (method) {
      case 'peer.joined':
        this.events.onPeerJoined?.(params.peer_id);
        break;
      case 'peer.left':
        this.events.onPeerLeft?.(params.peer_id);
        break;
      case 'peer.ice_candidate':
        if (params.from === 'remotemedia-server' && params.candidate) {
          const candidate: RTCIceCandidateInit = {
            candidate: params.candidate,
            sdpMid: params.sdp_mid || '0',
            sdpMLineIndex: params.sdp_m_line_index ?? 0,
          };
          if (this.pc?.remoteDescription) {
            this.pc.addIceCandidate(candidate);
          } else {
            this.pendingIceCandidates.push(candidate);
          }
        }
        break;
    }
  }

  private setupPeerConnectionHandlers() {
    if (!this.pc) return;

    this.pc.onicecandidate = (event) => {
      if (event.candidate) {
        this.sendRpc('peer.ice_candidate', {
          from: this.peerId,
          to: 'remotemedia-server',
          candidate: event.candidate.candidate,
          sdp_mid: event.candidate.sdpMid || '0',
          sdp_m_line_index: event.candidate.sdpMLineIndex ?? 0,
        });
      }
    };

    this.pc.ontrack = (event) => {
      if (event.track.kind === 'audio') {
        this.events.onAudioTrack?.(event.track, event.streams[0]);
      }
    };

    this.pc.onconnectionstatechange = () => {
      if (this.pc?.connectionState === 'failed') {
        this.setState('failed');
      }
    };

    // Handle data channels created by the remote peer
    this.pc.ondatachannel = (event) => {
      event.channel.binaryType = 'arraybuffer';
      this.setupDataChannelHandlers(event.channel);
    };
  }

  private setupDataChannelHandlers(channel: RTCDataChannel) {
    channel.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        try {
          const msg = decode(event.data);
          this.events.onData?.(msg);
        } catch (err) {
          console.error('[WebRTC] Failed to decode data channel message:', err);
        }
      }
    };

    channel.onopen = () => {
      console.log('[WebRTC] Data channel opened');
    };

    channel.onclose = () => {
      console.log('[WebRTC] Data channel closed');
    };
  }

  /** Disconnect and clean up all resources */
  disconnect() {
    if (this.localStream) {
      this.localStream.getTracks().forEach(t => t.stop());
      this.localStream = null;
      this._micEnabled = false;
    }
    this.dataChannel?.close();
    this.dataChannel = null;
    this.pc?.close();
    this.pc = null;
    this.ws?.close();
    this.ws = null;
    this.pendingIceCandidates = [];
    this.setState('disconnected');
  }

  /** Enable microphone — adds audio track to peer connection */
  async enableMic(): Promise<MediaStream> {
    if (!this.pc) throw new Error('Not connected');

    this.localStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    for (const track of this.localStream.getAudioTracks()) {
      this.pc.addTrack(track, this.localStream);
    }
    this._micEnabled = true;

    // Renegotiate if already connected
    if (this.pc.signalingState !== 'closed') {
      const offer = await this.pc.createOffer();
      await this.pc.setLocalDescription(offer);

      const offerId = this.sendRpc('peer.offer', {
        from: this.peerId,
        to: 'remotemedia-server',
        sdp: offer.sdp,
      });
      const result = await this.waitForRpcResponse(offerId);
      await this.pc.setRemoteDescription({ type: 'answer', sdp: result.sdp });
    }

    return this.localStream;
  }

  /** Disable microphone — removes audio tracks */
  disableMic() {
    if (this.localStream) {
      this.localStream.getTracks().forEach(track => {
        track.stop();
        const sender = this.pc?.getSenders().find(s => s.track === track);
        if (sender) this.pc?.removeTrack(sender);
      });
      this.localStream = null;
      this._micEnabled = false;
    }
  }

  // --- Data channel send methods ---

  /** Send a text string over the data channel */
  sendText(text: string) {
    if (!this.dataChannel || this.dataChannel.readyState !== 'open') {
      throw new Error('Data channel not open');
    }
    this.dataChannel.send(encodeText(this.sessionId, text));
  }

  /** Send PCM audio samples over the data channel */
  sendAudio(samples: Float32Array, sampleRate: number, channels: number) {
    if (!this.dataChannel || this.dataChannel.readyState !== 'open') {
      throw new Error('Data channel not open');
    }
    this.dataChannel.send(encodeAudio(this.sessionId, samples, sampleRate, channels));
  }

  /** Send raw binary data over the data channel */
  sendRaw(dataType: DataType, payload: Uint8Array) {
    if (!this.dataChannel || this.dataChannel.readyState !== 'open') {
      throw new Error('Data channel not open');
    }
    this.dataChannel.send(encode(dataType, this.sessionId, payload));
  }
}
```

- [ ] **Step 2: Build to verify no TypeScript errors**

Run: `cd crates/ui/frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add crates/ui/frontend/src/lib/webrtc.ts
git commit -m "feat(ui): implement WebRtcClient with signaling, peer connection, and data channel"
```

---

### Task 3: AudioVisualizer Component

Canvas-based waveform display for mic input audio. Uses `AnalyserNode` from Web Audio API.

**Files:**
- Create: `crates/ui/frontend/src/components/AudioVisualizer.tsx`

- [ ] **Step 1: Create AudioVisualizer component**

```tsx
// crates/ui/frontend/src/components/AudioVisualizer.tsx

import { useEffect, useRef } from 'preact/hooks';

interface AudioVisualizerProps {
  stream: MediaStream | null;
  height?: number;
}

export function AudioVisualizer({ stream, height = 60 }: AudioVisualizerProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animRef = useRef<number>(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !stream) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const audioCtx = new AudioContext();
    const source = audioCtx.createMediaStreamSource(stream);
    const analyser = audioCtx.createAnalyser();
    analyser.fftSize = 256;
    source.connect(analyser);

    const bufferLength = analyser.frequencyBinCount;
    const dataArray = new Uint8Array(bufferLength);

    const draw = () => {
      animRef.current = requestAnimationFrame(draw);
      analyser.getByteTimeDomainData(dataArray);

      const w = canvas.width;
      const h = canvas.height;

      ctx.fillStyle = getComputedStyle(canvas).getPropertyValue('--bg-input').trim() || '#1a1a3e';
      ctx.fillRect(0, 0, w, h);

      ctx.lineWidth = 2;
      ctx.strokeStyle = getComputedStyle(canvas).getPropertyValue('--accent').trim() || '#e94560';
      ctx.beginPath();

      const sliceWidth = w / bufferLength;
      let x = 0;

      for (let i = 0; i < bufferLength; i++) {
        const v = dataArray[i] / 128.0;
        const y = (v * h) / 2;
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
        x += sliceWidth;
      }

      ctx.lineTo(w, h / 2);
      ctx.stroke();
    };

    draw();

    return () => {
      cancelAnimationFrame(animRef.current);
      source.disconnect();
      audioCtx.close();
    };
  }, [stream]);

  return (
    <canvas
      ref={canvasRef}
      class="audio-visualizer-canvas"
      width={480}
      height={height}
      style={{ width: '100%', height: `${height}px` }}
    />
  );
}
```

- [ ] **Step 2: Add CSS for visualizer canvas in style.css**

Append to `crates/ui/frontend/src/style.css`:

```css
/* Audio visualizer canvas */
.audio-visualizer-canvas {
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  display: block;
  margin-top: 0.5rem;
}
```

- [ ] **Step 3: Build to verify**

Run: `cd crates/ui/frontend && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/ui/frontend/src/components/AudioVisualizer.tsx crates/ui/frontend/src/style.css
git commit -m "feat(ui): add AudioVisualizer component with canvas waveform"
```

---

### Task 4: WebRTC Panel UI

Rewrite `WebRtcPanel.tsx` with connection controls, audio section (mic + visualizer + playback), and data channel section (text input + message log).

**Files:**
- Rewrite: `crates/ui/frontend/src/components/WebRtcPanel.tsx`

- [ ] **Step 1: Rewrite WebRtcPanel with all sections**

```tsx
// crates/ui/frontend/src/components/WebRtcPanel.tsx

import { useState, useRef, useCallback, useEffect } from 'preact/hooks';
import { WebRtcClient, type ConnectionState } from '../lib/webrtc';
import { DataType, decodeText } from '../lib/wire-format';
import { AudioVisualizer } from './AudioVisualizer';
import type { DecodedMessage } from '../lib/wire-format';

interface Message {
  direction: 'sent' | 'received';
  text: string;
  timestamp: number;
}

export function WebRtcPanel({ transport }: { transport: any }) {
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [micEnabled, setMicEnabled] = useState(false);
  const [micStream, setMicStream] = useState<MediaStream | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [inputText, setInputText] = useState('');
  const [error, setError] = useState<string | null>(null);

  const clientRef = useRef<WebRtcClient | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const getSignalingUrl = useCallback(() => {
    if (!transport?.address) return null;
    const addr = transport.address;
    // Construct ws:// URL from address
    const host = addr.includes('0.0.0.0')
      ? addr.replace('0.0.0.0', window.location.hostname)
      : addr;
    return `ws://${host}/ws`;
  }, [transport]);

  const handleConnect = useCallback(async () => {
    const url = getSignalingUrl();
    if (!url) {
      setError('No signaling address available');
      return;
    }

    setError(null);

    const client = new WebRtcClient({
      onStateChange: setConnectionState,
      onAudioTrack: (_track, stream) => {
        if (audioRef.current) {
          audioRef.current.srcObject = stream;
          audioRef.current.play().catch(() => {});
        }
      },
      onData: (msg: DecodedMessage) => {
        let text: string;
        if (msg.dataType === DataType.Text) {
          text = decodeText(msg.payload);
        } else {
          text = `[${DataType[msg.dataType] || 'unknown'}] ${msg.payload.length} bytes`;
        }
        setMessages(prev => [...prev, {
          direction: 'received',
          text,
          timestamp: Date.now(),
        }]);
      },
      onError: setError,
    });

    clientRef.current = client;

    try {
      await client.connect(url);
    } catch (err) {
      // error is set via onError callback
    }
  }, [getSignalingUrl]);

  const handleDisconnect = useCallback(() => {
    clientRef.current?.disconnect();
    clientRef.current = null;
    setMicEnabled(false);
    setMicStream(null);
    setMessages([]);
  }, []);

  const handleToggleMic = useCallback(async () => {
    const client = clientRef.current;
    if (!client) return;

    try {
      if (micEnabled) {
        client.disableMic();
        setMicEnabled(false);
        setMicStream(null);
      } else {
        const stream = await client.enableMic();
        setMicEnabled(true);
        setMicStream(stream);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Mic error');
    }
  }, [micEnabled]);

  const handleSendText = useCallback(() => {
    const client = clientRef.current;
    if (!client || !inputText.trim()) return;

    try {
      client.sendText(inputText);
      setMessages(prev => [...prev, {
        direction: 'sent',
        text: inputText,
        timestamp: Date.now(),
      }]);
      setInputText('');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Send error');
    }
  }, [inputText]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendText();
    }
  }, [handleSendText]);

  const isConnected = connectionState === 'connected';

  return (
    <div class="panel">
      <h2>WebRTC Real-Time</h2>

      {/* Connection controls */}
      <div class="webrtc-connection-bar">
        {connectionState === 'disconnected' ? (
          <button class="btn btn-primary" onClick={handleConnect}>
            Connect
          </button>
        ) : connectionState === 'connecting' ? (
          <button class="btn btn-secondary" disabled>
            Connecting...
          </button>
        ) : (
          <button class="btn btn-danger" onClick={handleDisconnect}>
            Disconnect
          </button>
        )}
        <span class={`status-dot ${isConnected ? 'connected' : 'disconnected'}`} />
        <span class="webrtc-state">{connectionState}</span>
        {transport?.address && (
          <span class="transport-url">Signaling: {transport.address}</span>
        )}
      </div>

      {error && <div class="error">{error}</div>}

      {/* Audio section */}
      {isConnected && (
        <div class="webrtc-section">
          <label>Audio</label>
          <div class="btn-group">
            <button
              class={`btn ${micEnabled ? 'btn-danger' : 'btn-secondary'}`}
              onClick={handleToggleMic}
            >
              {micEnabled ? 'Stop Mic' : 'Start Mic'}
            </button>
          </div>
          {micEnabled && <AudioVisualizer stream={micStream} />}
          <audio ref={audioRef} autoplay style={{ display: 'none' }} />
        </div>
      )}

      {/* Data channel section */}
      {isConnected && (
        <div class="webrtc-section">
          <label>Data Channel</label>
          <div class="webrtc-messages">
            {messages.map((msg, i) => (
              <div key={i} class={`webrtc-msg webrtc-msg-${msg.direction}`}>
                <span class="webrtc-msg-dir">{msg.direction === 'sent' ? '>' : '<'}</span>
                <span class="webrtc-msg-text">{msg.text}</span>
              </div>
            ))}
            <div ref={messagesEndRef} />
          </div>
          <div class="webrtc-input-row">
            <input
              type="text"
              placeholder="Type a message..."
              value={inputText}
              onInput={(e) => setInputText((e.target as HTMLInputElement).value)}
              onKeyDown={handleKeyDown}
            />
            <button class="btn btn-primary" onClick={handleSendText} disabled={!inputText.trim()}>
              Send
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Add WebRTC panel styles to style.css**

Append to `crates/ui/frontend/src/style.css`:

```css
/* WebRTC Panel */
.webrtc-connection-bar {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  flex-wrap: wrap;
}

.webrtc-state {
  font-size: 0.85rem;
  color: var(--text-secondary);
  text-transform: capitalize;
}

.webrtc-section {
  margin-top: 1.25rem;
  padding-top: 1.25rem;
  border-top: 1px solid var(--border);
}

.webrtc-messages {
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0.75rem;
  min-height: 120px;
  max-height: 240px;
  overflow-y: auto;
  font-family: var(--font-mono);
  font-size: 0.8rem;
  margin-bottom: 0.5rem;
}

.webrtc-msg {
  padding: 2px 0;
  line-height: 1.5;
}

.webrtc-msg-dir {
  color: var(--text-muted);
  margin-right: 0.5rem;
  user-select: none;
}

.webrtc-msg-sent .webrtc-msg-text {
  color: var(--text-primary);
}

.webrtc-msg-received .webrtc-msg-text {
  color: var(--success);
}

.webrtc-input-row {
  display: flex;
  gap: 0.5rem;
}

.webrtc-input-row input[type="text"] {
  flex: 1;
}

.webrtc-input-row .btn {
  flex-shrink: 0;
}
```

- [ ] **Step 3: Build frontend**

Run: `cd crates/ui/frontend && npx tsc --noEmit && npm run build`
Expected: Build succeeds, produces `dist/` output

- [ ] **Step 4: Commit**

```bash
git add crates/ui/frontend/src/components/WebRtcPanel.tsx crates/ui/frontend/src/style.css
git commit -m "feat(ui): implement WebRTC panel with audio, data channel, and connection UI"
```

---

### Task 5: CLI Flag Rename and Transport Address Fix

Rename `--ws-port` to `--signal-port`, add `--signal-type`, and make `transport.address` point to the signal port when `--signal-port` is specified.

**Files:**
- Modify: `examples/cli/remotemedia-cli/src/commands/serve.rs`

- [ ] **Step 1: Update ServeArgs**

In `serve.rs`, replace the `ws_port` field and add `signal_type`:

Replace:
```rust
    /// WebSocket signaling port (when --transport webrtc)
    ///
    /// Starts a WebSocket signaling server alongside the gRPC one,
    /// enabling browser-based WebRTC connections via ws://host:ws-port/ws
    #[arg(long)]
    pub ws_port: Option<u16>,
```

With:
```rust
    /// Signaling port for browser-accessible WebRTC connections
    ///
    /// Starts a signaling server (WebSocket or gRPC-Web) alongside the
    /// main transport, enabling browser-based WebRTC connections.
    /// The UI's transport.address will point to this port.
    #[arg(long)]
    pub signal_port: Option<u16>,

    /// Signaling protocol type
    ///
    /// Determines which signaling server to start on --signal-port.
    /// "websocket" (default) starts a WebSocket JSON-RPC 2.0 server.
    /// "grpc" starts a gRPC-Web compatible server (not yet implemented).
    #[arg(long, default_value = "websocket")]
    pub signal_type: String,
```

- [ ] **Step 2: Update the execute function**

In `execute()`, update two things:

a) In the UI startup block, when `signal_port` is set, use it as the transport address:

Replace:
```rust
        let address = format!("{}:{}", args.host, args.port);
```

With:
```rust
        let address = if let Some(sp) = args.signal_port {
            format!("{}:{}", args.host, sp)
        } else {
            format!("{}:{}", args.host, args.port)
        };
```

b) In the WebRTC transport block, replace `args.ws_port` with `args.signal_port`:

Replace:
```rust
                let _ws_handle = if let Some(ws_port) = args.ws_port {
```

With:
```rust
                let _ws_handle = if let Some(signal_port) = args.signal_port {
```

And replace all `ws_port` references in that block with `signal_port`.

- [ ] **Step 3: Build CLI to verify**

Run: `cd examples/cli/remotemedia-cli && cargo build --features ui,webrtc`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add examples/cli/remotemedia-cli/src/commands/serve.rs
git commit -m "feat(cli): rename --ws-port to --signal-port, add --signal-type flag"
```

---

### Task 6: Update Playwright Config and E2E Tests

Update the Playwright config to use `--signal-port` instead of `--ws-port`, and add panel interaction tests.

**Files:**
- Modify: `crates/ui/e2e/playwright.config.ts`
- Modify: `crates/ui/e2e/tests/webrtc-signaling.spec.ts`

- [ ] **Step 1: Update playwright.config.ts**

Replace `--ws-port` with `--signal-port` in the `webServer.command`:

Replace:
```
--ws-port ${WS_PORT}
```

With:
```
--signal-port ${WS_PORT}
```

Rename the env var from `WS_PORT` to `SIGNAL_PORT` for clarity:

```typescript
const SIGNAL_PORT = process.env.SIGNAL_PORT || '18091';
```

Update all references from `WS_PORT` to `SIGNAL_PORT`.

- [ ] **Step 2: Update webrtc-signaling.spec.ts env var**

Replace:
```typescript
const WS_PORT = process.env.WS_PORT || '18091';
const WS_URL = `ws://127.0.0.1:${WS_PORT}/ws`;
```

With:
```typescript
const SIGNAL_PORT = process.env.SIGNAL_PORT || '18091';
const WS_URL = `ws://127.0.0.1:${SIGNAL_PORT}/ws`;
```

- [ ] **Step 3: Add WebRTC panel interaction tests**

Add these tests to `webrtc-signaling.spec.ts` in a new `describe('WebRTC Panel Interaction')` block:

```typescript
  test.describe('WebRTC Panel Interaction', () => {
    test('Connect button establishes connection', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      // Switch to WebRTC tab
      await page.getByRole('button', { name: 'WebRTC' }).click();

      // Click Connect
      await page.getByRole('button', { name: 'Connect' }).click();

      // Should show connected state
      await expect(page.locator('.webrtc-state')).toContainText('connected', { timeout: 15000 });

      // Data channel input should be visible
      await expect(page.locator('.webrtc-input-row input')).toBeVisible();

      // Disconnect
      await page.getByRole('button', { name: 'Disconnect' }).click();
      await expect(page.locator('.webrtc-state')).toContainText('disconnected', { timeout: 5000 });
    });

    test('data channel text roundtrip', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      await page.getByRole('button', { name: 'WebRTC' }).click();
      await page.getByRole('button', { name: 'Connect' }).click();
      await expect(page.locator('.webrtc-state')).toContainText('connected', { timeout: 15000 });

      // Type and send text
      await page.locator('.webrtc-input-row input').fill('hello webrtc');
      await page.getByRole('button', { name: 'Send' }).click();

      // Sent message should appear
      await expect(page.locator('.webrtc-msg-sent')).toContainText('hello webrtc');

      // With a passthrough pipeline, output should come back
      await expect(page.locator('.webrtc-msg-received')).toBeVisible({ timeout: 10000 });

      // Cleanup
      await page.getByRole('button', { name: 'Disconnect' }).click();
    });

    test('Disconnect cleans up UI', async ({ page }) => {
      await page.goto('/');
      await expect(page.locator('.status-dot.connected')).toBeVisible({ timeout: 10000 });

      await page.getByRole('button', { name: 'WebRTC' }).click();
      await page.getByRole('button', { name: 'Connect' }).click();
      await expect(page.locator('.webrtc-state')).toContainText('connected', { timeout: 15000 });

      await page.getByRole('button', { name: 'Disconnect' }).click();

      // Audio section and data channel should disappear
      await expect(page.locator('.webrtc-messages')).not.toBeVisible();
      // Connect button should reappear
      await expect(page.getByRole('button', { name: 'Connect' })).toBeVisible();
    });
  });
```

- [ ] **Step 4: Run all tests**

Run: `cd crates/ui/e2e && npx playwright test --reporter=line`
Expected: All tests pass (existing + new)

- [ ] **Step 5: Commit**

```bash
git add crates/ui/e2e/playwright.config.ts crates/ui/e2e/tests/webrtc-signaling.spec.ts
git commit -m "test(e2e): update CLI flags, add WebRTC panel interaction tests"
```

---

### Task 7: Rebuild and Full Verification

Rebuild the entire frontend + CLI and run all test suites.

**Files:**
- No new files — verification only

- [ ] **Step 1: Rebuild frontend**

Run: `cd crates/ui/frontend && npm run build`
Expected: Build succeeds

- [ ] **Step 2: Rebuild Rust UI crate (re-embed frontend assets)**

Run: `cd /path/to/workspace && cargo clean -p remotemedia-ui && cargo build -p remotemedia-ui`
Expected: Compiles successfully

- [ ] **Step 3: Rebuild CLI**

Run: `cd examples/cli/remotemedia-cli && cargo build --features ui,webrtc`
Expected: Compiles successfully

- [ ] **Step 4: Run Rust integration tests**

Run: `cargo test -p remotemedia-ui --test webrtc_ui_integration`
Expected: All 13 tests pass

- [ ] **Step 5: Run Playwright E2E tests**

Run: `cd crates/ui/e2e && npx playwright test --reporter=line`
Expected: All tests pass (existing 32 + new panel tests)

- [ ] **Step 6: Update TESTING.md with new test counts**

Update `crates/ui/TESTING.md` to reflect the added tests in the `webrtc-signaling.spec.ts` coverage table.

- [ ] **Step 7: Final commit**

```bash
git add crates/ui/TESTING.md
git commit -m "docs: update testing docs with WebRTC panel test coverage"
```
