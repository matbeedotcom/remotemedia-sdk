'use client';

/**
 * WebRTC Calculator Demo
 *
 * This page demonstrates:
 * 1. Server Action to provision WebRTC pipeline session (Node.js FFI)
 * 2. WebSocket signaling for WebRTC connection establishment
 * 3. Data channel communication with protobuf-encoded messages
 * 4. Pipeline processing (CalculatorNode) with JSON request/response
 *
 * Architecture:
 *   Browser                        Next.js Server                 Rust WebRTC Server
 *   ──────────────────────────────────────────────────────────────────────────────────
 *   1. createSession() ──────────> Server Action ───────────────> Start pipeline server
 *   2. <── signalingUrl ─────────< Return WebSocket URL <────────
 *   3. WebSocket connect ─────────────────────────────────────────> JSON-RPC signaling
 *   4. peer.announce ─────────────────────────────────────────────> Register peer
 *   5. peer.offer (SDP) ──────────────────────────────────────────> Create answer
 *   6. <── answer SDP ────────────────────────────────────────────< Return answer
 *   7. <── ICE candidates ────────────────────────────────────────< Trickle ICE
 *   8. WebRTC connected ──────────────────────────────────────────
 *   9. Data channel: { operation: 'add', operands: [10, 20] } ───> Pipeline processes
 *   10. <── Data channel: { result: 30 } ─────────────────────────< Return result
 */

import { useState, useRef, useCallback, useEffect } from 'react';
import {
  createPipelineSession,
  terminateSession,
  type CreateSessionResult,
} from '@/app/actions/webrtc-pipeline';
import { encodeJsonData, decodeDataBuffer, parseJsonFromDataBuffer } from '@/lib/proto-utils';

// Calculator operations
type Operation = 'add' | 'subtract' | 'multiply' | 'divide';

interface CalculatorRequest {
  operation: Operation;
  operands: number[];
}

interface CalculatorResponse {
  operation: Operation;
  operands: number[];
  result: number;
}

interface CalculationHistory {
  request: CalculatorRequest;
  response: CalculatorResponse | null;
  timestamp: number;
  status: 'pending' | 'success' | 'error';
  error?: string;
}

// JSON-RPC helper
function createJsonRpcRequest(
  method: string,
  params: Record<string, unknown>,
  id: string
) {
  return JSON.stringify({ jsonrpc: '2.0', method, params, id });
}

export default function WebRTCCalculatorPage() {
  // Session state
  const [session, setSession] = useState<CreateSessionResult | null>(null);
  const [connectionStatus, setConnectionStatus] = useState<
    'disconnected' | 'connecting' | 'signaling' | 'connected' | 'error'
  >('disconnected');
  const [error, setError] = useState<string | null>(null);

  // Calculator state
  const [operand1, setOperand1] = useState<string>('10');
  const [operand2, setOperand2] = useState<string>('20');
  const [operation, setOperation] = useState<Operation>('add');
  const [history, setHistory] = useState<CalculationHistory[]>([]);

  // WebRTC refs
  const wsRef = useRef<WebSocket | null>(null);
  const pcRef = useRef<RTCPeerConnection | null>(null);
  const dataChannelRef = useRef<RTCDataChannel | null>(null);
  const requestIdCounter = useRef(0);
  const pendingRequests = useRef<
    Map<
      string,
      { resolve: (value: unknown) => void; reject: (error: Error) => void }
    >
  >(new Map());

  // ICE configuration
  const rtcConfig: RTCConfiguration = {
    iceServers: [
      { urls: 'stun:stun.l.google.com:19302' },
      { urls: 'stun:stun1.l.google.com:19302' },
    ],
  };

  /**
   * Create session via Server Action and connect
   */
  const connect = useCallback(async () => {
    try {
      setConnectionStatus('connecting');
      setError(null);

      // 1. Create session via Server Action
      const result = await createPipelineSession('calculator');

      if (!result.success || !result.signalingUrl) {
        throw new Error(result.error || 'Failed to create session');
      }

      setSession(result);
      setConnectionStatus('signaling');

      // 2. Connect to WebSocket signaling server
      const ws = new WebSocket(result.signalingUrl);
      wsRef.current = ws;

      ws.onopen = async () => {
        console.log('[Signaling] WebSocket connected');

        // 3. Create RTCPeerConnection
        const pc = new RTCPeerConnection(rtcConfig);
        pcRef.current = pc;

        // 4. Create data channel BEFORE creating offer
        const dataChannel = pc.createDataChannel('pipeline-data', {
          ordered: true,
        });
        dataChannelRef.current = dataChannel;

        dataChannel.onopen = () => {
          console.log('[DataChannel] Opened');
          setConnectionStatus('connected');
        };

        dataChannel.onclose = () => {
          console.log('[DataChannel] Closed');
          if (connectionStatus === 'connected') {
            setConnectionStatus('disconnected');
          }
        };

        dataChannel.onmessage = (event) => {
          handleDataChannelMessage(event.data);
        };

        dataChannel.onerror = (err) => {
          console.error('[DataChannel] Error:', err);
          setError('Data channel error');
        };

        // Handle ICE candidates
        pc.onicecandidate = ({ candidate }) => {
          if (candidate && ws.readyState === WebSocket.OPEN) {
            const msg = createJsonRpcRequest(
              'peer.ice_candidate',
              {
                from: 'calculator-client',
                to: 'remotemedia-server',
                candidate: candidate.candidate,
                sdp_m_line_index: candidate.sdpMLineIndex,
                sdp_mid: candidate.sdpMid,
              },
              `ice-${++requestIdCounter.current}`
            );
            ws.send(msg);
          }
        };

        pc.onconnectionstatechange = () => {
          console.log('[WebRTC] Connection state:', pc.connectionState);
          if (
            pc.connectionState === 'failed' ||
            pc.connectionState === 'disconnected'
          ) {
            setConnectionStatus('error');
            setError(`WebRTC connection ${pc.connectionState}`);
          }
        };

        // 5. Announce peer
        const announceId = `announce-${++requestIdCounter.current}`;
        sendSignalingRequest(ws, 'peer.announce', {
          peer_id: 'calculator-client',
          capabilities: ['audio', 'data'],
          user_data: { type: 'calculator-demo' },
        }, announceId);

        // 6. Add audio transceiver (required for WebRTC)
        pc.addTransceiver('audio', { direction: 'sendrecv' });

        // 7. Create and send offer
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);

        const offerId = `offer-${++requestIdCounter.current}`;
        const answerResult = await sendSignalingRequestWithResponse<{
          type: string;
          sdp: string;
          from: string;
          to: string;
        }>(ws, 'peer.offer', {
          from: 'calculator-client',
          to: 'remotemedia-server',
          sdp: offer.sdp,
          can_trickle_ice_candidates: true,
        }, offerId);

        // 8. Apply answer
        const answer = new RTCSessionDescription({
          type: 'answer',
          sdp: answerResult.sdp,
        });
        await pc.setRemoteDescription(answer);
        console.log('[WebRTC] Answer applied, waiting for data channel...');
      };

      ws.onmessage = (event) => {
        handleSignalingMessage(JSON.parse(event.data));
      };

      ws.onerror = (err) => {
        console.error('[Signaling] WebSocket error:', err);
        setError('WebSocket connection failed');
        setConnectionStatus('error');
      };

      ws.onclose = () => {
        console.log('[Signaling] WebSocket closed');
        if (connectionStatus !== 'disconnected') {
          setConnectionStatus('disconnected');
        }
      };
    } catch (err) {
      console.error('[Connect] Error:', err);
      setError(err instanceof Error ? err.message : 'Connection failed');
      setConnectionStatus('error');
    }
  }, []);

  /**
   * Send signaling request without waiting for response
   */
  function sendSignalingRequest(
    ws: WebSocket,
    method: string,
    params: Record<string, unknown>,
    id: string
  ) {
    const msg = createJsonRpcRequest(method, params, id);
    ws.send(msg);
  }

  /**
   * Send signaling request and wait for response
   */
  function sendSignalingRequestWithResponse<T>(
    ws: WebSocket,
    method: string,
    params: Record<string, unknown>,
    id: string
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      pendingRequests.current.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
      });

      const msg = createJsonRpcRequest(method, params, id);
      ws.send(msg);

      // Timeout after 10 seconds
      setTimeout(() => {
        if (pendingRequests.current.has(id)) {
          pendingRequests.current.delete(id);
          reject(new Error(`Request ${method} timed out`));
        }
      }, 10000);
    });
  }

  /**
   * Handle signaling messages from server
   */
  function handleSignalingMessage(msg: {
    id?: string;
    result?: unknown;
    error?: { message: string };
    method?: string;
    params?: Record<string, unknown>;
  }) {
    // Response to a request
    if (msg.id && pendingRequests.current.has(msg.id)) {
      const pending = pendingRequests.current.get(msg.id)!;
      pendingRequests.current.delete(msg.id);
      if (msg.error) {
        pending.reject(new Error(msg.error.message));
      } else {
        pending.resolve(msg.result);
      }
      return;
    }

    // Notification
    if (msg.method === 'peer.ice_candidate' && msg.params) {
      const pc = pcRef.current;
      if (pc && msg.params.candidate) {
        const candidate = new RTCIceCandidate({
          candidate: msg.params.candidate as string,
          sdpMLineIndex: msg.params.sdp_m_line_index as number,
          sdpMid: msg.params.sdp_mid as string,
        });
        pc.addIceCandidate(candidate).catch(console.error);
      }
    } else if (msg.method === 'peer.state_change' && msg.params) {
      console.log('[WebRTC] State change:', msg.params);
    }
  }

  /**
   * Handle data channel messages (calculator responses)
   */
  function handleDataChannelMessage(data: ArrayBuffer) {
    try {
      const decoded = decodeDataBuffer(data);
      const response = parseJsonFromDataBuffer<CalculatorResponse>(decoded);

      if (response) {
        console.log('[Calculator] Response:', response);

        // Update history with response
        setHistory((prev) => {
          const updated = [...prev];
          const pendingIdx = updated.findIndex(
            (h) =>
              h.status === 'pending' &&
              h.request.operation === response.operation &&
              JSON.stringify(h.request.operands) ===
                JSON.stringify(response.operands)
          );
          if (pendingIdx !== -1) {
            updated[pendingIdx] = {
              ...updated[pendingIdx],
              response,
              status: 'success',
            };
          }
          return updated;
        });
      }
    } catch (err) {
      console.error('[Calculator] Failed to decode response:', err);
    }
  }

  /**
   * Send calculator request through data channel
   */
  const calculate = useCallback(() => {
    const dc = dataChannelRef.current;
    if (!dc || dc.readyState !== 'open') {
      setError('Data channel not connected');
      return;
    }

    const num1 = parseFloat(operand1);
    const num2 = parseFloat(operand2);

    if (isNaN(num1) || isNaN(num2)) {
      setError('Please enter valid numbers');
      return;
    }

    const request: CalculatorRequest = {
      operation,
      operands: [num1, num2],
    };

    // Add to history as pending
    const historyEntry: CalculationHistory = {
      request,
      response: null,
      timestamp: Date.now(),
      status: 'pending',
    };
    setHistory((prev) => [historyEntry, ...prev].slice(0, 20));

    // Encode and send
    const encoded = encodeJsonData(request, 'CalculatorRequest');
    dc.send(encoded);

    console.log('[Calculator] Sent request:', request);
    setError(null);
  }, [operand1, operand2, operation]);

  /**
   * Disconnect and cleanup
   */
  const disconnect = useCallback(async () => {
    // Close data channel
    if (dataChannelRef.current) {
      dataChannelRef.current.close();
      dataChannelRef.current = null;
    }

    // Close peer connection
    if (pcRef.current) {
      pcRef.current.close();
      pcRef.current = null;
    }

    // Close WebSocket
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }

    // Terminate session
    if (session?.sessionId) {
      await terminateSession(session.sessionId);
    }

    setSession(null);
    setConnectionStatus('disconnected');
    pendingRequests.current.clear();
  }, [session]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (wsRef.current) wsRef.current.close();
      if (pcRef.current) pcRef.current.close();
      if (dataChannelRef.current) dataChannelRef.current.close();
    };
  }, []);

  // Helper to get operation symbol
  const getOperationSymbol = (op: Operation): string => {
    switch (op) {
      case 'add':
        return '+';
      case 'subtract':
        return '-';
      case 'multiply':
        return '*';
      case 'divide':
        return '/';
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-blue-50 to-indigo-50 p-8">
      <div className="max-w-4xl mx-auto">
        {/* Header */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <h1 className="text-3xl font-bold text-gray-800 mb-2">
            WebRTC Calculator Demo
          </h1>
          <p className="text-gray-600">
            Real-time pipeline processing via WebRTC data channels using Node.js
            FFI
          </p>
        </div>

        {/* Connection Status */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-xl font-semibold text-gray-800 mb-2">
                Connection Status
              </h2>
              <p className="text-lg mb-1">
                Status:{' '}
                <span
                  className={`font-semibold ${
                    connectionStatus === 'connected'
                      ? 'text-green-600'
                      : connectionStatus === 'error'
                        ? 'text-red-600'
                        : connectionStatus === 'connecting' ||
                            connectionStatus === 'signaling'
                          ? 'text-yellow-600'
                          : 'text-gray-600'
                  }`}
                >
                  {connectionStatus.charAt(0).toUpperCase() +
                    connectionStatus.slice(1)}
                </span>
              </p>
              {session?.sessionId && (
                <p className="text-sm text-gray-500">
                  Session: {session.sessionId.slice(0, 8)}...
                </p>
              )}
            </div>

            <div className="flex gap-3">
              {connectionStatus === 'disconnected' ||
              connectionStatus === 'error' ? (
                <button
                  onClick={connect}
                  className="px-6 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors"
                >
                  Connect
                </button>
              ) : connectionStatus === 'connecting' ||
                connectionStatus === 'signaling' ? (
                <button
                  disabled
                  className="px-6 py-2 bg-gray-400 text-white rounded-lg cursor-not-allowed"
                >
                  Connecting...
                </button>
              ) : (
                <button
                  onClick={disconnect}
                  className="px-6 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors"
                >
                  Disconnect
                </button>
              )}
            </div>
          </div>
        </div>

        {/* Error Display */}
        {error && (
          <div className="bg-red-50 border border-red-200 rounded-lg p-4 mb-6">
            <p className="text-red-800">
              <strong>Error:</strong> {error}
            </p>
          </div>
        )}

        {/* Calculator */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <h2 className="text-xl font-semibold text-gray-800 mb-4">
            Calculator
          </h2>

          <div className="flex flex-wrap items-center gap-4 mb-4">
            <input
              type="number"
              value={operand1}
              onChange={(e) => setOperand1(e.target.value)}
              className="w-32 px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              placeholder="First number"
              disabled={connectionStatus !== 'connected'}
            />

            <select
              value={operation}
              onChange={(e) => setOperation(e.target.value as Operation)}
              className="px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              disabled={connectionStatus !== 'connected'}
            >
              <option value="add">+ Add</option>
              <option value="subtract">- Subtract</option>
              <option value="multiply">* Multiply</option>
              <option value="divide">/ Divide</option>
            </select>

            <input
              type="number"
              value={operand2}
              onChange={(e) => setOperand2(e.target.value)}
              className="w-32 px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              placeholder="Second number"
              disabled={connectionStatus !== 'connected'}
            />

            <button
              onClick={calculate}
              disabled={connectionStatus !== 'connected'}
              className="px-6 py-2 bg-green-600 text-white rounded-lg hover:bg-green-700 transition-colors disabled:bg-gray-400 disabled:cursor-not-allowed"
            >
              = Calculate
            </button>
          </div>

          {/* Preview */}
          <div className="text-sm text-gray-500">
            Preview: {operand1 || '?'} {getOperationSymbol(operation)}{' '}
            {operand2 || '?'} = ?
          </div>
        </div>

        {/* History */}
        <div className="bg-white rounded-lg shadow-lg p-6 mb-6">
          <h2 className="text-xl font-semibold text-gray-800 mb-4">
            Calculation History
          </h2>

          {history.length === 0 ? (
            <p className="text-gray-500">
              No calculations yet. Connect and try some operations!
            </p>
          ) : (
            <div className="space-y-2">
              {history.map((entry, idx) => (
                <div
                  key={idx}
                  className={`p-3 rounded-lg ${
                    entry.status === 'success'
                      ? 'bg-green-50 border border-green-200'
                      : entry.status === 'pending'
                        ? 'bg-yellow-50 border border-yellow-200'
                        : 'bg-red-50 border border-red-200'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div>
                      <span className="font-mono">
                        {entry.request.operands[0]}{' '}
                        {getOperationSymbol(entry.request.operation)}{' '}
                        {entry.request.operands[1]} ={' '}
                        {entry.response?.result ?? '...'}
                      </span>
                    </div>
                    <div className="text-xs text-gray-500">
                      {entry.status === 'pending' && '(calculating...)'}
                      {entry.status === 'success' &&
                        new Date(entry.timestamp).toLocaleTimeString()}
                      {entry.status === 'error' && `Error: ${entry.error}`}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* How It Works */}
        <div className="bg-white rounded-lg shadow-lg p-6">
          <h2 className="text-xl font-semibold text-gray-800 mb-4">
            How It Works
          </h2>

          <div className="space-y-3 text-gray-700">
            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                1
              </span>
              <p>
                <strong>Server Action:</strong> Click Connect to call{' '}
                <code className="bg-gray-100 px-1 rounded">
                  createPipelineSession()
                </code>{' '}
                which starts a Rust WebRTC server with the Calculator pipeline
                via Node.js FFI
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                2
              </span>
              <p>
                <strong>WebSocket Signaling:</strong> Connect to the returned
                signaling URL for JSON-RPC 2.0 WebRTC signaling
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                3
              </span>
              <p>
                <strong>SDP Exchange:</strong> peer.offer sends our SDP, server
                responds with answer SDP directly in the JSON-RPC response
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-blue-600 text-white rounded-full flex items-center justify-center text-sm">
                4
              </span>
              <p>
                <strong>ICE Candidates:</strong> Exchange ICE candidates via
                peer.ice_candidate notifications
              </p>
            </div>

            <div className="flex items-start gap-3">
              <span className="flex-shrink-0 w-6 h-6 bg-green-600 text-white rounded-full flex items-center justify-center text-sm">
                5
              </span>
              <p>
                <strong>Data Channel:</strong> Once connected, send protobuf-encoded
                JSON calculator requests, receive results via data channel
              </p>
            </div>
          </div>

          <div className="mt-6 p-4 bg-blue-50 rounded-lg">
            <h3 className="font-semibold text-blue-900 mb-2">Technical Details</h3>
            <ul className="text-sm text-blue-800 space-y-1">
              <li>
                <strong>Server:</strong> Rust WebRTC server (webrtc-rs) via Node.js FFI
              </li>
              <li>
                <strong>Signaling:</strong> JSON-RPC 2.0 over WebSocket
              </li>
              <li>
                <strong>Data Channel:</strong> Ordered, reliable delivery
              </li>
              <li>
                <strong>Encoding:</strong> Protobuf DataBuffer with JsonData variant
              </li>
              <li>
                <strong>Pipeline:</strong> CalculatorNode processes JSON operations
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
