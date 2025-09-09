/**
 * TypeScript interfaces for CodeExecutorNode
 * Auto-generated from Python TypedDict classes
 */
/**
 * Error output structure for CodeExecutorNode.
 */
export interface CodeExecutorNodeCodeExecutorError {
    error: string;
    code?: string;
    input?: any;
    processed_by: string;
}
/**
 * Input data structure for CodeExecutorNode.
 */
export interface CodeExecutorNodeCodeExecutorInput {
    code: string;
    input?: any;
}
/**
 * Output data structure for CodeExecutorNode.
 */
export interface CodeExecutorNodeCodeExecutorOutput {
    executed_code: string;
    input: any;
    result: any;
    processed_by: string;
    node_config: Record<string, any>;
}
/**
 * CodeExecutorNode Interface
 *
 *
    Code Executor node - executes arbitrary Python code.
    
    WARNING: This is INSECURE and should only be used in trusted environments!
    
    Expects input data in the format:
    {
        "code": "python_code_string",
        "input": optional_input_data
    }
    
 */
export interface CodeExecutorNode {
    /** List of additional safe modules to allow. */
    allowed_modules?: Array<string>;
    /** Python code to execute. Can be provided at initialization or runtime. */
    code?: string;
    /** Whether to enable cloudpickle module for advanced serialization. (default: false) */
    enable_cloudpickle?: boolean;
    /** Whether to enable pickle module (DANGEROUS). (default: false) */
    enable_pickle?: boolean;
    /** Whether to enable safe module imports like math, json. (default: false) */
    enable_safe_imports?: boolean;
    /** Input data to make available during code execution. */
    input_data?: any;
    args: any;
    /** Whether to enable state management (default: True) (default: true) */
    enable_state?: boolean;
    /** Maximum number of concurrent sessions (default: None/unlimited) */
    max_sessions?: number;
    /** Optional name for the node (defaults to class name) */
    name?: string;
    /** Time-to-live for session states (default: 24 hours) */
    state_ttl?: any;
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
    process(data: CodeExecutorNodeCodeExecutorInput | any): CodeExecutorNodeCodeExecutorOutput | CodeExecutorNodeCodeExecutorError;
    /** Set the current session ID for state management. */
    set_session_id(session_id: string): null;
    /** Split data into content and metadata components. */
    split_data_metadata(data: any): any | any;
}
//# sourceMappingURL=code-executor-node.d.ts.map