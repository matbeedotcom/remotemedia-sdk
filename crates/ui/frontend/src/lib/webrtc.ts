/**
 * WebRTC client for RemoteMedia SDK.
 *
 * Handles WebSocket signaling (JSON-RPC 2.0), RTCPeerConnection lifecycle,
 * data channel ("pipeline"), and mic audio tracks.
 */

import {
  DataType,
  type DecodedMessage,
  decode,
  encode,
  encodeText,
  encodeAudio,
} from './wire-format';

export { DataType };
export type { DecodedMessage };

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'failed';

export interface WebRtcClientEvents {
  onStateChange?: (state: ConnectionState) => void;
  onAudioTrack?: (track: MediaStreamTrack, stream: MediaStream) => void;
  onData?: (msg: DecodedMessage) => void;
  onPeerJoined?: (peerId: string) => void;
  onPeerLeft?: (peerId: string) => void;
  onError?: (error: string) => void;
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

interface JsonRpcRequest {
  jsonrpc: '2.0';
  method: string;
  params: Record<string, unknown>;
  id?: string;
}

interface JsonRpcResponse {
  jsonrpc: '2.0';
  id?: string;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
  method?: string;
  params?: Record<string, unknown>;
}

const SERVER_PEER_ID = 'remotemedia-server';
const STUN_SERVERS: RTCIceServer[] = [
  { urls: 'stun:stun.l.google.com:19302' },
  { urls: 'stun:stun1.l.google.com:19302' },
];
const RPC_TIMEOUT_MS = 10_000;

// ---------------------------------------------------------------------------
// WebRtcClient
// ---------------------------------------------------------------------------

export class WebRtcClient {
  private _state: ConnectionState = 'disconnected';
  private _micEnabled = false;

  private readonly events: WebRtcClientEvents;
  private readonly peerId: string;
  private readonly sessionId: string;

  private ws: WebSocket | null = null;
  private pc: RTCPeerConnection | null = null;
  private dataChannel: RTCDataChannel | null = null;
  private localStream: MediaStream | null = null;

  /** Buffered ICE candidates that arrived before remoteDescription was set. */
  private pendingIceCandidates: RTCIceCandidateInit[] = [];

  /** Pending RPC response callbacks keyed by request id. */
  private rpcCallbacks = new Map<
    string,
    { resolve: (result: unknown) => void; reject: (err: Error) => void }
  >();

  /** Running counter for RPC request ids. */
  private rpcCounter = 0;

  constructor(events: WebRtcClientEvents = {}) {
    this.events = events;
    this.peerId = `browser-${Math.random().toString(36).slice(2, 8)}`;
    this.sessionId = `session-${Date.now()}`;
  }

  // ---------------------------------------------------------------------------
  // Public accessors
  // ---------------------------------------------------------------------------

  get state(): ConnectionState {
    return this._state;
  }

  get micEnabled(): boolean {
    return this._micEnabled;
  }

  // ---------------------------------------------------------------------------
  // Connect / Disconnect
  // ---------------------------------------------------------------------------

  connect(signalingUrl: string): Promise<void> {
    return new Promise((resolve, reject) => {
      this.setState('connecting');

      const ws = new WebSocket(signalingUrl);
      this.ws = ws;

      ws.onopen = async () => {
        try {
          await this.onWsOpen();
          resolve();
        } catch (err) {
          this.handleError(`Connection setup failed: ${err}`);
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      };

      ws.onerror = (ev) => {
        const msg = 'WebSocket error';
        this.handleError(msg);
        reject(new Error(msg));
      };

      ws.onclose = () => {
        if (this._state !== 'disconnected') {
          this.setState('disconnected');
        }
      };

      ws.onmessage = (ev) => {
        try {
          this.handleWsMessage(JSON.parse(ev.data as string) as JsonRpcResponse);
        } catch (err) {
          this.handleError(`Failed to parse signaling message: ${err}`);
        }
      };
    });
  }

  disconnect(): void {
    this.setState('disconnected');

    // Stop mic tracks
    if (this.localStream) {
      for (const track of this.localStream.getTracks()) {
        track.stop();
      }
      this.localStream = null;
    }
    this._micEnabled = false;

    // Close data channel
    if (this.dataChannel) {
      this.dataChannel.close();
      this.dataChannel = null;
    }

    // Close peer connection
    if (this.pc) {
      this.pc.close();
      this.pc = null;
    }

    // Close WebSocket
    if (this.ws) {
      this.ws.onclose = null; // prevent state update loop
      this.ws.close();
      this.ws = null;
    }

    // Reject any pending RPC calls
    for (const { reject } of this.rpcCallbacks.values()) {
      reject(new Error('Client disconnected'));
    }
    this.rpcCallbacks.clear();
    this.pendingIceCandidates = [];
  }

  // ---------------------------------------------------------------------------
  // Mic
  // ---------------------------------------------------------------------------

  async enableMic(): Promise<MediaStream> {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    this.localStream = stream;
    this._micEnabled = true;

    if (this.pc) {
      for (const track of stream.getAudioTracks()) {
        this.pc.addTrack(track, stream);
      }
      // Renegotiate
      await this.doOfferAnswer();
    }

    return stream;
  }

  disableMic(): void {
    if (!this.localStream) return;
    for (const track of this.localStream.getTracks()) {
      track.stop();
    }
    this.localStream = null;
    this._micEnabled = false;
  }

  // ---------------------------------------------------------------------------
  // Sending data
  // ---------------------------------------------------------------------------

  sendText(text: string): void {
    const buf = encodeText(text);
    this.sendBuffer(buf);
  }

  sendAudio(samples: Float32Array, sampleRate: number, channels: number): void {
    const buf = encodeAudio(samples, sampleRate, channels);
    this.sendBuffer(buf);
  }

  sendRaw(dataType: DataType, payload: Uint8Array): void {
    const buf = encode(dataType, payload);
    this.sendBuffer(buf);
  }

  // ---------------------------------------------------------------------------
  // Private: connection setup
  // ---------------------------------------------------------------------------

  private async onWsOpen(): Promise<void> {
    // 1. Announce presence
    await this.sendRpc('peer.announce', {
      peer_id: this.peerId,
      capabilities: ['audio', 'data'],
      user_data: {},
    });

    // 2. Build peer connection
    this.pc = new RTCPeerConnection({ iceServers: STUN_SERVERS });
    this.setupPeerConnectionHandlers(this.pc);

    // 3. Create data channel
    const dc = this.pc.createDataChannel('pipeline', {
      ordered: true,
      // maxRetransmits omitted → reliable
    });
    dc.binaryType = 'arraybuffer';
    this.setupDataChannelHandlers(dc);
    this.dataChannel = dc;

    // 4. Create offer and exchange with server
    await this.doOfferAnswer();

    // 5. Wait for data channel to open (proves ICE + DTLS succeeded)
    await this.waitForDataChannelOpen(dc);

    this.setState('connected');
  }

  /** Wait for a data channel to reach 'open' state, or reject on failure. */
  private waitForDataChannelOpen(dc: RTCDataChannel, timeoutMs = 15000): Promise<void> {
    if (dc.readyState === 'open') return Promise.resolve();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        dc.removeEventListener('open', onOpen);
        reject(new Error('Data channel open timed out'));
      }, timeoutMs);
      const onOpen = () => {
        clearTimeout(timer);
        resolve();
      };
      dc.addEventListener('open', onOpen, { once: true });
      dc.addEventListener('error', () => {
        clearTimeout(timer);
        reject(new Error('Data channel error during open'));
      }, { once: true });
    });
  }

  private setupPeerConnectionHandlers(pc: RTCPeerConnection): void {
    pc.onicecandidate = (ev) => {
      if (ev.candidate) {
        this.sendRpcNotification('peer.ice_candidate', {
          from: this.peerId,
          to: SERVER_PEER_ID,
          candidate: ev.candidate.candidate,
          sdp_mid: ev.candidate.sdpMid ?? '',
          sdp_m_line_index: ev.candidate.sdpMLineIndex ?? 0,
        });
      }
    };

    pc.ontrack = (ev) => {
      const track = ev.track;
      const stream = ev.streams[0] ?? new MediaStream([track]);
      this.events.onAudioTrack?.(track, stream);
    };

    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'failed') {
        this.handleError('RTCPeerConnection failed');
        this.setState('failed');
      }
    };
  }

  private setupDataChannelHandlers(dc: RTCDataChannel): void {
    dc.onmessage = (ev) => {
      try {
        const msg = decode(ev.data as ArrayBuffer);
        this.events.onData?.(msg);
      } catch (err) {
        this.handleError(`Failed to decode data channel message: ${err}`);
      }
    };

    dc.onerror = (ev) => {
      this.handleError(`Data channel error: ${ev}`);
    };
  }

  /** Create an SDP offer, send it to the server, apply the answer. */
  private async doOfferAnswer(): Promise<void> {
    if (!this.pc || !this.ws) return;

    const offer = await this.pc.createOffer();
    await this.pc.setLocalDescription(offer);

    const result = await this.sendRpc('peer.offer', {
      from: this.peerId,
      to: SERVER_PEER_ID,
      sdp: offer.sdp,
    });

    const answer = result as { type: string; sdp: string; from: string; to: string };
    await this.pc.setRemoteDescription(
      new RTCSessionDescription({ type: 'answer', sdp: answer.sdp }),
    );

    // Flush any buffered ICE candidates
    for (const candidate of this.pendingIceCandidates) {
      await this.pc.addIceCandidate(new RTCIceCandidate(candidate));
    }
    this.pendingIceCandidates = [];
  }

  // ---------------------------------------------------------------------------
  // Private: WebSocket message handling
  // ---------------------------------------------------------------------------

  private handleWsMessage(msg: JsonRpcResponse): void {
    // RPC response (has an id and result/error)
    if (msg.id !== undefined && !msg.method) {
      const cb = this.rpcCallbacks.get(msg.id);
      if (cb) {
        this.rpcCallbacks.delete(msg.id);
        if (msg.error) {
          cb.reject(new Error(`RPC error ${msg.error.code}: ${msg.error.message}`));
        } else {
          cb.resolve(msg.result);
        }
      }
      return;
    }

    // Server notification (no id, has method)
    if (msg.method) {
      this.handleNotification(msg.method, msg.params ?? {});
    }
  }

  private handleNotification(method: string, params: Record<string, unknown>): void {
    switch (method) {
      case 'peer.joined':
        this.events.onPeerJoined?.(params.peer_id as string);
        break;

      case 'peer.left':
        this.events.onPeerLeft?.(params.peer_id as string);
        break;

      case 'peer.ice_candidate': {
        const candidate = params.candidate as string;
        // Empty candidate = end of gathering, skip
        if (!candidate) break;
        this.handleRemoteIceCandidate({
          candidate,
          sdpMid: (params.sdp_mid as string) || '0',
          sdpMLineIndex: (params.sdp_m_line_index as number) ?? 0,
        });
        break;
      }

      default:
        // Ignore unknown notifications
        break;
    }
  }

  private handleRemoteIceCandidate(init: RTCIceCandidateInit): void {
    if (!this.pc || !this.pc.remoteDescription) {
      // Buffer until remoteDescription is set
      this.pendingIceCandidates.push(init);
      return;
    }
    this.pc
      .addIceCandidate(new RTCIceCandidate(init))
      .catch((err) => this.handleError(`addIceCandidate failed: ${err}`));
  }

  // ---------------------------------------------------------------------------
  // Private: JSON-RPC helpers
  // ---------------------------------------------------------------------------

  private sendRpc(method: string, params: Record<string, unknown>): Promise<unknown> {
    const id = `rpc-${++this.rpcCounter}`;
    const msg: JsonRpcRequest = { jsonrpc: '2.0', method, params, id };

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.rpcCallbacks.delete(id);
        reject(new Error(`RPC timeout: ${method} (id=${id})`));
      }, RPC_TIMEOUT_MS);

      this.rpcCallbacks.set(id, {
        resolve: (result) => {
          clearTimeout(timer);
          resolve(result);
        },
        reject: (err) => {
          clearTimeout(timer);
          reject(err);
        },
      });

      this.wsSend(JSON.stringify(msg));
    });
  }

  /** Send a JSON-RPC notification (no id, no response expected). */
  private sendRpcNotification(method: string, params: Record<string, unknown>): void {
    const msg: JsonRpcRequest = { jsonrpc: '2.0', method, params };
    this.wsSend(JSON.stringify(msg));
  }

  private wsSend(data: string): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      this.handleError('WebSocket is not open');
      return;
    }
    this.ws.send(data);
  }

  // ---------------------------------------------------------------------------
  // Private: data channel sending
  // ---------------------------------------------------------------------------

  private sendBuffer(buf: ArrayBuffer): void {
    if (!this.dataChannel || this.dataChannel.readyState !== 'open') {
      this.handleError('Data channel is not open');
      return;
    }
    this.dataChannel.send(buf);
  }

  // ---------------------------------------------------------------------------
  // Private: state / error helpers
  // ---------------------------------------------------------------------------

  private setState(state: ConnectionState): void {
    if (this._state === state) return;
    this._state = state;
    this.events.onStateChange?.(state);
  }

  private handleError(message: string): void {
    this.events.onError?.(message);
  }
}
