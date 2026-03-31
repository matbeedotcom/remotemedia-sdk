import { useState, useEffect } from 'preact/hooks';
import { getManifest } from '../lib/api';

interface ManifestNode {
  id: string;
  node_type: string;
  params?: Record<string, any>;
  executor?: string;
}

interface ManifestConnection {
  from: string;
  to: string;
}

interface Manifest {
  nodes: ManifestNode[];
  connections?: ManifestConnection[];
}

export function ManifestView() {
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getManifest()
      .then((data) => {
        setManifest(data);
        setLoading(false);
      })
      .catch((e) => {
        setError(e.message);
        setLoading(false);
      });
  }, []);

  if (loading) return <div class="loading">Loading manifest...</div>;
  if (error) return <div class="error">{error}</div>;
  if (!manifest) return <div class="panel"><p>No manifest loaded.</p></div>;

  // Build ordered list: for each connection, show from -> to
  const connectionMap = new Map<string, string[]>();
  if (manifest.connections) {
    for (const conn of manifest.connections) {
      const targets = connectionMap.get(conn.from) || [];
      targets.push(conn.to);
      connectionMap.set(conn.from, targets);
    }
  }

  return (
    <div class="panel">
      <h2>Pipeline Manifest</h2>

      {manifest.nodes.map((node, i) => (
        <div key={node.id}>
          <div class="node-card">
            <div class="node-header">
              <span class="node-id">{node.id}</span>
              <span class="node-type">{node.node_type}</span>
              {node.executor && <span class="node-type">{node.executor}</span>}
            </div>
            {node.params && Object.keys(node.params).length > 0 && (
              <div class="node-params">
                {JSON.stringify(node.params, null, 2)}
              </div>
            )}
          </div>

          {connectionMap.has(node.id) && (
            <div class="connection-arrow">
              {connectionMap.get(node.id)!.map((target) => (
                <div key={target}>&#8595; {target}</div>
              ))}
            </div>
          )}

          {i < manifest.nodes.length - 1 && !connectionMap.has(node.id) && (
            <div class="connection-arrow">&#8595;</div>
          )}
        </div>
      ))}

      <details style={{ marginTop: '1rem' }}>
        <summary style={{ cursor: 'pointer', color: 'var(--text-secondary)', fontSize: '0.85rem' }}>
          Raw JSON
        </summary>
        <pre class="result-json" style={{ marginTop: '0.5rem' }}>
          {JSON.stringify(manifest, null, 2)}
        </pre>
      </details>
    </div>
  );
}
