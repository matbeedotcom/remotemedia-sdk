/**
 * TypeScript interfaces for CalculatorNode
 * Auto-generated from Python TypedDict classes
 */

/**
 * Error output structure for CalculatorNode.
 */
export interface CalculatorNodeCalculatorError {
  error: string;
  operation: "add" | "multiply" | "subtract" | "divide" | "power" | "modulo" | string | null;
  args: Array<number | number> | any | null;
  processed_by: string;
}

/**
 * Input data structure for CalculatorNode.
 */
export interface CalculatorNodeCalculatorInput {
  operation: "add" | "multiply" | "subtract" | "divide" | "power" | "modulo";
  args: Array<number | number>;
}

/**
 * Output data structure for CalculatorNode.
 */
export interface CalculatorNodeCalculatorOutput {
  operation: "add" | "multiply" | "subtract" | "divide" | "power" | "modulo";
  args: Array<number | number>;
  result: number | number;
  processed_by: string;
  node_config: Record<string, any>;
}


/**
 * CalculatorNode Interface
 * 
 * 
    Calculator node - performs mathematical operations.
    
    Expects input data in the format:
    {
        "operation": "add|multiply|subtract|divide|power|modulo",
        "args": [number1, number2, ...]
    }
    
 */
export interface CalculatorNode {
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
  process(data: CalculatorNodeCalculatorInput | any): CalculatorNodeCalculatorOutput | CalculatorNodeCalculatorError;
  /** Set the current session ID for state management. */
  set_session_id(session_id: string): null;
  /** Split data into content and metadata components. */
  split_data_metadata(data: any): any | any;
}
