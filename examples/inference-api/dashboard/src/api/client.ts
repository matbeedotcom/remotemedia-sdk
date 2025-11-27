/**
 * API client for the Inference API server.
 */

const API_BASE = import.meta.env.VITE_API_URL || 'http://localhost:8000';

interface FetchOptions {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
}

async function apiFetch<T>(path: string, options: FetchOptions = {}): Promise<T> {
  const { method = 'GET', body, headers = {} } = options;

  const response = await fetch(`${API_BASE}${path}`, {
    method,
    headers: {
      'Content-Type': 'application/json',
      ...headers,
    },
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ message: response.statusText }));
    throw new Error(error.message || 'Request failed');
  }

  return response.json();
}

// Health check
export interface HealthResponse {
  status: string;
  version: string;
  pipelines_loaded: number;
  active_sessions: number;
  runtime_available: boolean;
}

export async function checkHealth(): Promise<HealthResponse> {
  return apiFetch<HealthResponse>('/health');
}

// Pipelines
export interface PipelineInfo {
  name: string;
  description: string;
  version: string;
  input_type: string;
  output_type: string;
  streaming: boolean;
}

export interface PipelinesResponse {
  pipelines: PipelineInfo[];
}

export async function listPipelines(): Promise<PipelinesResponse> {
  return apiFetch<PipelinesResponse>('/pipelines');
}

export async function getPipeline(name: string): Promise<PipelineInfo> {
  return apiFetch<PipelineInfo>(`/pipelines/${name}`);
}

// Prediction
export interface PredictRequest {
  pipeline: string;
  input_data: string | null;
  input_type?: string;
  config?: Record<string, unknown>;
}

export interface PredictResponse {
  output_type: string;
  output_data: string | null;
  metadata: Record<string, unknown>;
  timing_ms: number;
}

export async function predict(request: PredictRequest): Promise<PredictResponse> {
  return apiFetch<PredictResponse>('/predict', {
    method: 'POST',
    body: request,
  });
}

// Streaming
export interface StreamSessionResponse {
  session_id: string;
  pipeline: string;
  status: string;
}

export async function startStream(
  pipeline: string,
  config?: Record<string, unknown>
): Promise<StreamSessionResponse> {
  return apiFetch<StreamSessionResponse>('/stream', {
    method: 'POST',
    body: { pipeline, config },
  });
}

export async function sendStreamInput(
  sessionId: string,
  inputData: string | null,
  inputType: string = 'audio'
): Promise<void> {
  await apiFetch(`/stream/${sessionId}/input`, {
    method: 'POST',
    body: { input_data: inputData, input_type: inputType },
  });
}

export async function closeStream(sessionId: string): Promise<void> {
  await apiFetch(`/stream/${sessionId}`, { method: 'DELETE' });
}

export function streamOutput(
  sessionId: string,
  onMessage: (event: MessageEvent) => void,
  onError?: (error: Event) => void
): EventSource {
  const source = new EventSource(`${API_BASE}/stream/${sessionId}/output`);

  source.onmessage = onMessage;
  source.onerror = onError || (() => source.close());

  return source;
}
