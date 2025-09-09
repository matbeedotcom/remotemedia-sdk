/**
 * TypeScript interfaces for TextProcessorNode
 * Auto-generated from Python TypedDict classes
 */
/**
 * Error output structure for TextProcessorNode.
 */
export interface TextProcessorNodeTextProcessorError {
    error: string;
    text: string;
    operations: Array<string>;
    processed_by: string;
}
/**
 * Input data structure for TextProcessorNode.
 */
export interface TextProcessorNodeTextProcessorInput {
    text: string;
    operations: Array<string>;
}
/**
 * Output data structure for TextProcessorNode.
 */
export interface TextProcessorNodeTextProcessorOutput {
    original_text: string;
    operations: Array<string>;
    results: Record<string, any>;
    processed_by: string;
    node_config: Record<string, any>;
}
/**
 * TextProcessorNode Interface
 *
 *
    Text Processor node - performs various text processing operations.
    
    Expects input data in the format:
    {
        "text": "string_to_process",
        "operations": ["uppercase", "lowercase", "reverse", "word_count", "char_count"]
    }
    
 */
export interface TextProcessorNode {
    /** Whether to enable state management (default: True) (default: true) */
    enable_state?: boolean;
    /** Maximum number of concurrent sessions (default: None/unlimited) */
    max_sessions?: number;
    /** Optional name for the node (defaults to class name) */
    name?: string;
    /** Time-to-live for session states (default: 24 hours) */
    state_ttl?: any;
    args: any;
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
    process(data: TextProcessorNodeTextProcessorInput | any): TextProcessorNodeTextProcessorOutput | TextProcessorNodeTextProcessorError;
    /** Set the current session ID for state management. */
    set_session_id(session_id: string): null;
    /** Split data into content and metadata components. */
    split_data_metadata(data: any): any | any;
}
//# sourceMappingURL=text-processor-node.d.ts.map