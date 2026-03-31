import { useEffect, useRef, useState } from 'preact/hooks';
import { WebRtcClient, type ConnectionState } from '../lib/webrtc';
import { DataType, decodeText } from '../lib/wire-format';
import { AudioVisualizer } from './AudioVisualizer';

interface Message {
  direction: 'sent' | 'received';
  text: string;
  timestamp: number;
}

function getSignalingUrl(address: string): string {
  const host = address.replace('0.0.0.0', window.location.hostname);
  return `ws://${host}/ws`;
}

function isFirefox(): boolean {
  return navigator.userAgent.includes('Firefox');
}

function isLoopback(): boolean {
  const h = window.location.hostname;
  return h === 'localhost' || h === '127.0.0.1' || h === '::1';
}

/** Query the signaling server for its LAN IP via server.info JSON-RPC. */
async function discoverLanIp(signalingUrl: string): Promise<string | null> {
  try {
    return new Promise((resolve) => {
      const ws = new WebSocket(signalingUrl);
      const timeout = setTimeout(() => { ws.close(); resolve(null); }, 5000);

      ws.onopen = () => {
        ws.send(JSON.stringify({
          jsonrpc: '2.0',
          method: 'server.info',
          params: {},
          id: 'lan-discovery',
        }));
      };
      ws.onmessage = (e) => {
        try {
          const msg = JSON.parse(e.data);
          if (msg.id === 'lan-discovery' && msg.result?.addresses) {
            const defaultAddr = msg.result.addresses.find((a: any) => a.is_default);
            clearTimeout(timeout);
            ws.close();
            resolve(defaultAddr?.ip || null);
          }
        } catch { /* ignore parse errors */ }
      };
      ws.onerror = () => { clearTimeout(timeout); resolve(null); };
    });
  } catch {
    return null;
  }
}

function dataTypeName(dt: DataType): string {
  switch (dt) {
    case DataType.Audio: return 'Audio';
    case DataType.Video: return 'Video';
    case DataType.Text: return 'Text';
    case DataType.Tensor: return 'Tensor';
    case DataType.ControlMessage: return 'ControlMessage';
    case DataType.Numpy: return 'Numpy';
    case DataType.File: return 'File';
    default: return 'Unknown';
  }
}

export function WebRtcPanel({ transport }: { transport: any }) {
  const clientRef = useRef<WebRtcClient | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const messagesRef = useRef<HTMLDivElement>(null);

  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [micEnabled, setMicEnabled] = useState(false);
  const [micStream, setMicStream] = useState<MediaStream | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [inputText, setInputText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [lanIp, setLanIp] = useState<string | null>(null);
  const showFirefoxWarning = isFirefox() && isLoopback();

  // Auto-scroll messages to bottom on new messages
  useEffect(() => {
    const el = messagesRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages]);

  // Discover LAN IP for Firefox loopback warning
  useEffect(() => {
    if (showFirefoxWarning) {
      const address = transport?.address || '';
      if (address) {
        const url = getSignalingUrl(address);
        discoverLanIp(url).then(setLanIp);
      }
    }
  }, [showFirefoxWarning]);

  // Clean up client on unmount
  useEffect(() => {
    return () => {
      clientRef.current?.disconnect();
    };
  }, []);

  function createClient(): WebRtcClient {
    const client = new WebRtcClient({
      onStateChange: (state) => {
        setConnectionState(state);
        if (state === 'disconnected' || state === 'failed') {
          setMicEnabled(false);
          setMicStream(null);
        }
      },
      onAudioTrack: (_track, stream) => {
        if (audioRef.current) {
          audioRef.current.srcObject = stream;
        }
      },
      onData: (msg) => {
        let text: string;
        if (msg.dataType === DataType.Text) {
          text = decodeText(msg.payload);
        } else {
          text = `[${dataTypeName(msg.dataType)}] ${msg.payload.byteLength} bytes`;
        }
        setMessages((prev) => [
          ...prev,
          { direction: 'received', text, timestamp: Date.now() },
        ]);
      },
      onError: (err) => {
        setError(err);
      },
    });
    return client;
  }

  async function handleConnect() {
    setError(null);
    const client = createClient();
    clientRef.current = client;

    const address = transport?.address || 'localhost:8080';
    const url = getSignalingUrl(address);

    try {
      await client.connect(url);
    } catch (err) {
      setError(String(err));
    }
  }

  function handleDisconnect() {
    clientRef.current?.disconnect();
    clientRef.current = null;
    setConnectionState('disconnected');
    setMicEnabled(false);
    setMicStream(null);
  }

  async function handleMicToggle() {
    const client = clientRef.current;
    if (!client) return;

    if (micEnabled) {
      client.disableMic();
      setMicEnabled(false);
      setMicStream(null);
    } else {
      try {
        const stream = await client.enableMic();
        setMicEnabled(true);
        setMicStream(stream);
      } catch (err) {
        setError(`Mic error: ${err}`);
      }
    }
  }

  function handleSend() {
    const text = inputText.trim();
    if (!text || !clientRef.current) return;

    clientRef.current.sendText(text);
    setMessages((prev) => [
      ...prev,
      { direction: 'sent', text, timestamp: Date.now() },
    ]);
    setInputText('');
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  const isConnected = connectionState === 'connected';
  const isConnecting = connectionState === 'connecting';

  return (
    <div class="panel">
      <h2>WebRTC Real-Time</h2>

      {/* Firefox loopback warning */}
      {showFirefoxWarning && (
        <div class="warning">
          Firefox does not support WebRTC ICE on localhost.{' '}
          {lanIp ? (
            <span>
              Use your LAN IP instead:{' '}
              <a href={`http://${lanIp}:${window.location.port}${window.location.pathname}`}>
                {`http://${lanIp}:${window.location.port}`}
              </a>
            </span>
          ) : (
            <span>Access this page via your machine's LAN IP address.</span>
          )}
        </div>
      )}

      {/* Connection bar */}
      <div class="webrtc-connection-bar">
        <span class={`status-dot ${isConnected ? 'connected' : 'disconnected'}`} />
        <span class="webrtc-state">{connectionState}</span>
        <span class="transport-url">
          Signaling: {transport?.address || 'unknown'}
        </span>
        <div class="btn-group">
          {isConnecting ? (
            <button class="btn btn-secondary" disabled>Connecting...</button>
          ) : isConnected ? (
            <button class="btn btn-danger" onClick={handleDisconnect}>Disconnect</button>
          ) : (
            <button class="btn btn-primary" onClick={handleConnect}>Connect</button>
          )}
        </div>
      </div>

      {error && <p class="error">{error}</p>}

      {/* Audio section */}
      {isConnected && (
        <div class="webrtc-section">
          <div class="btn-group">
            <button
              class={`btn ${micEnabled ? 'btn-danger' : 'btn-secondary'}`}
              onClick={handleMicToggle}
            >
              {micEnabled ? 'Stop Mic' : 'Start Mic'}
            </button>
          </div>
          {micEnabled && micStream && (
            <AudioVisualizer stream={micStream} height={60} />
          )}
          {/* Hidden audio element for pipeline output playback */}
          <audio ref={audioRef} autoplay style={{ display: 'none' }} />
        </div>
      )}

      {/* Data channel section */}
      {isConnected && (
        <div class="webrtc-section">
          <div class="webrtc-messages" ref={messagesRef}>
            {messages.map((msg, i) => (
              <div key={i} class={`webrtc-msg ${msg.direction === 'sent' ? 'webrtc-msg-sent' : 'webrtc-msg-received'}`}>
                <span class="webrtc-msg-dir">{msg.direction === 'sent' ? '>' : '<'}</span>
                <span class="webrtc-msg-text">{msg.text}</span>
              </div>
            ))}
          </div>
          <div class="webrtc-input-row">
            <input
              type="text"
              value={inputText}
              onInput={(e) => setInputText((e.target as HTMLInputElement).value)}
              onKeyDown={handleKeyDown}
              placeholder="Send a message..."
            />
            <button class="btn btn-primary" onClick={handleSend} disabled={!inputText.trim()}>
              Send
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
