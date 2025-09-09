/**
 * TypeScript interfaces for PassThroughNode
 * Auto-generated from Python TypedDict classes
 */

/**
 * Error output structure for BufferNode.
 */
export interface PassThroughNodeBufferError {
  error: string;
  input: any;
  processed_by: string;
}

/**
 * Output data structure for BufferNode.
 */
export interface PassThroughNodeBufferOutput {
  buffer: Array<any>;
  count: number;
  processed_by: string;
}

/**
 * Error output structure for PassThroughNode.
 */
export interface PassThroughNodePassThroughError {
  error: string;
  input: any;
  processed_by: string;
}


/**
 * PassThroughNode Interface
 * 
 * A node that passes data through without modification.
 */
export interface PassThroughNode {
  // Configuration properties (constructor arguments)
  /** Whether to enable state management (default: True) (default: true) */
  enable_state?: boolean;
  /** Maximum number of concurrent sessions (default: None/unlimited) */
  max_sessions?: number;
  /** Optional name for the node (defaults to class name) */
  name?: string;
  /** Time-to-live for session states (default: 24 hours) */
  state_ttl?: any;
  args: any;

  // Available methods
  /** Clean up resources used by the node. */
  cleanup(): null;
  /** Extract session ID from input data. */
  extract_session_id(data: any): string | null;
  /** Get the node configuration. */
  get_config(): Record<string, any>;
  /** Get the current session ID. */
  get_session_id(): string | null;
  /** Get the session state for the given session ID. */
  get_session_state(session_id?: string | null): any | null;
  /** Initialize the node before processing. */
  initialize(): null;
  /** Merge processed data with metadata. */
  merge_data_metadata(data: any, metadata: Record<string, any> | null): any;
  /** Process input data through this node. */
  process(data: any): any;
  /** Set the current session ID for state management. */
  set_session_id(session_id: string): null;
  /** Split data into content and metadata components. */
  split_data_metadata(data: any): any | any;
}
