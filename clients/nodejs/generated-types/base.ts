/**
 * Base TypeScript interfaces for RemoteMedia Processing SDK
 * Generated at: 2025-08-03T01:45:43.781111
 * Service version: 0.1.0
 */

export interface RemoteMediaNode {
  name?: string;
  config?: Record<string, any>;
  process(data: any): any | Promise<any>;
  initialize?(): Promise<void>;
  cleanup?(): Promise<void>;
  flush?(): any | Promise<any>;
}

export interface RemoteExecutorConfig {
  host: string;
  port: number;
  protocol?: 'grpc' | 'http';
  authToken?: string;
  timeout?: number;
  maxRetries?: number;
  sslEnabled?: boolean;
  pipPackages?: string[];
}

export interface ExecutionOptions {
  timeout?: number;
  maxMemoryMb?: number;
  cpuLimit?: number;
  enableGpu?: boolean;
  priority?: 'low' | 'normal' | 'high';
}

export interface ExecutionResponse<T = any> {
  status: 'success' | 'error';
  data?: T;
  error?: {
    message: string;
    traceback?: string;
  };
  metrics?: {
    startTimestamp: number;
    endTimestamp: number;
    durationMs: number;
    memoryPeakMb?: number;
    cpuTimeMs?: number;
  };
}

export interface StreamHandle {
  send(data: any): Promise<void>;
  close(): Promise<void>;
  readonly sessionId: string;
}

export interface NodeInfo {
  node_type: string;
  category: string;
  description: string;
  parameters: NodeParameter[];
}

export interface NodeParameter {
  name: string;
  type: string;
  required: boolean;
  default_value?: any;
  description?: string;
  source_class?: string;
}

export type SerializationFormat = 'json' | 'pickle';
