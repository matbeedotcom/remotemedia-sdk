import { useState, useEffect } from 'preact/hooks';
import { GenericPanel } from './components/GenericPanel';
import { WebRtcPanel } from './components/WebRtcPanel';
import { ManifestView } from './components/ManifestView';

interface StatusInfo {
  version: string;
  transport: { transport_type: string; address: string } | null;
  active_sessions: number;
}

export function App() {
  const [status, setStatus] = useState<StatusInfo | null>(null);
  const [tab, setTab] = useState<'pipeline' | 'webrtc' | 'manifest'>('pipeline');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/status')
      .then(r => r.json())
      .then(setStatus)
      .catch(e => setError(e.message));
  }, []);

  const showWebRtc = status?.transport?.transport_type === 'webrtc';

  return (
    <div class="app">
      <header class="header">
        <h1>RemoteMedia</h1>
        <div class="status-bar">
          {status ? (
            <>
              <span class="status-dot connected" />
              <span>Connected</span>
              {status.transport && (
                <span class="transport-badge">{status.transport.transport_type}</span>
              )}
              <span class="session-count">{status.active_sessions} sessions</span>
            </>
          ) : error ? (
            <>
              <span class="status-dot disconnected" />
              <span>{error}</span>
            </>
          ) : (
            <span>Connecting...</span>
          )}
        </div>
      </header>

      <nav class="tabs">
        <button
          class={`tab ${tab === 'pipeline' ? 'active' : ''}`}
          onClick={() => setTab('pipeline')}
        >
          Pipeline
        </button>
        {showWebRtc && (
          <button
            class={`tab ${tab === 'webrtc' ? 'active' : ''}`}
            onClick={() => setTab('webrtc')}
          >
            WebRTC
          </button>
        )}
        <button
          class={`tab ${tab === 'manifest' ? 'active' : ''}`}
          onClick={() => setTab('manifest')}
        >
          Manifest
        </button>
      </nav>

      <main class="content">
        {tab === 'pipeline' && <GenericPanel />}
        {tab === 'webrtc' && <WebRtcPanel transport={status?.transport} />}
        {tab === 'manifest' && <ManifestView />}
      </main>
    </div>
  );
}
