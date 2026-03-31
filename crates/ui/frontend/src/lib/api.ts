export async function getStatus() {
  const res = await fetch('/api/status');
  return res.json();
}

export async function getManifest() {
  const res = await fetch('/api/manifest');
  if (!res.ok) return null;
  return res.json();
}

export async function executePipeline(input: any, manifest?: any) {
  // Wrap input as TransportData { data: RuntimeData, metadata: {} }
  const transportData = input?.data ? input : { data: input };
  if (!transportData.metadata) transportData.metadata = {};
  const body: any = { input: transportData };
  if (manifest) body.manifest = manifest;
  const res = await fetch('/api/execute', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.message || 'Execution failed');
  }
  return res.json();
}

export async function createStream(manifest?: any) {
  const body: any = {};
  if (manifest) body.manifest = manifest;
  const res = await fetch('/api/stream', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error('Failed to create stream');
  return res.json();
}

export async function sendStreamInput(sessionId: string, data: any) {
  await fetch(`/api/stream/${sessionId}/input`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ data }),
  });
}

export function subscribeToStream(sessionId: string, onData: (data: any) => void): EventSource {
  const es = new EventSource(`/api/stream/${sessionId}/output`);
  es.onmessage = (e) => onData(JSON.parse(e.data));
  return es;
}

export async function closeStream(sessionId: string) {
  await fetch(`/api/stream/${sessionId}`, { method: 'DELETE' });
}
