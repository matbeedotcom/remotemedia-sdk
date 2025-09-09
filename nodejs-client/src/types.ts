/**
 * Type definitions for RemoteMedia Node.js Client
 */

/**
 * Configuration for connecting to a remote executor service
 */
export interface RemoteExecutorConfig {
  /** Hostname or IP address of the remote service */
  host: string;

  /** Port number of the remote service */
  port: number;

  /** Protocol to use (currently only gRPC is supported) */
  protocol?: 'grpc' | 'http';

  /** Connection timeout in seconds */
  timeout?: number;

  /** Enable SSL/TLS for the connection */
  sslEnabled?: boolean;

  /** Custom SSL credentials (if not using default) */
  sslCredentials?: any;

  /** Maximum message size in bytes */
  maxMessageSize?: number;

  /** Retry configuration */
  retry?: {
    maxAttempts?: number;
    initialBackoff?: number;
    maxBackoff?: number;
    backoffMultiplier?: number;
  };
}

/**
 * Configuration for a node instance
 */
export interface NodeConfig {
  [key: string]: any;
}

/**
 * Options for node execution
 */
export interface ExecutionOptions {
  /** Execution timeout in seconds */
  timeout?: number;

  /** Enable GPU acceleration if available */
  enable_gpu?: boolean;

  /** Execution priority */
  priority?: 'low' | 'normal' | 'high';

  /** Custom metadata for the execution */
  metadata?: Record<string, string>;
}

/**
 * Remote node proxy interface
 */
export interface RemoteNodeProxy {
  /** 
   * Process data through the node
   * @param data Input data for the node
   * @returns Promise resolving to the processed output
   */
  process(data: any): Promise<any>;

  /** 
   * Process data through the node with streaming
   * @param onData Callback for each data chunk
   * @param onError Optional error callback
   * @returns Stream handle for bidirectional communication
   */
  processStream?(onData: (data: any) => void, onError?: (error: Error) => void): StreamHandle;
}

/**
 * Handle for managing streaming operations
 */
export interface StreamHandle {
  /** Send data to the stream */
  send(data: any): Promise<void>;

  /** Close the stream */
  close(): Promise<void>;

  /** Unique session identifier */
  sessionId: string;
}

/**
 * Node information returned by server
 */
export interface NodeInfo {
  /** Type identifier for the node */
  node_type: string;

  /** Category of the node */
  category: string;

  /** Human-readable description */
  description: string;

  /** Node parameters */
  parameters?: NodeParameter[];

  /** Whether the node supports streaming */
  is_streaming?: boolean;
}

/**
 * Parameter definition for a node
 */
export interface NodeParameter {
  /** Parameter name */
  name: string;

  /** Parameter type */
  type: string;

  /** Whether the parameter is required */
  required: boolean;

  /** Default value if not provided */
  default_value?: any;

  /** Parameter description */
  description?: string;

  /** Allowed values for enum types */
  allowed_values?: any[];

  /** Class that defined this parameter */
  source_class?: string;
}

/**
 * Server status information
 */
export interface ServerStatus {
  /** Service status */
  status: 'SERVICE_STATUS_UNKNOWN' | 'SERVICE_STATUS_HEALTHY' | 'SERVICE_STATUS_UNHEALTHY';

  /** Server version */
  version: string;

  /** Uptime in seconds */
  uptime_seconds: number;

  /** Server metrics */
  metrics?: ServerMetrics;

  /** Active sessions */
  active_sessions?: SessionInfo[];
}

/**
 * Server performance metrics
 */
export interface ServerMetrics {
  /** Total number of requests */
  total_requests: number;

  /** Success rate (0-1) */
  success_rate: number;

  /** Average latency in milliseconds */
  average_latency_ms: number;

  /** Current CPU usage (0-1) */
  cpu_usage?: number;

  /** Current memory usage in bytes */
  memory_usage?: number;
}

/**
 * Information about an active session
 */
export interface SessionInfo {
  /** Session identifier */
  session_id: string;

  /** Node type being executed */
  node_type: string;

  /** Session start time */
  start_time: string;

  /** Session status */
  status: string;
}

/**
 * Error response from the server
 */
export interface RemoteExecutionError extends Error {
  /** Error code from the server */
  code?: string;

  /** Additional error details */
  details?: any;

  /** Stack trace from the server */
  serverStack?: string;
}