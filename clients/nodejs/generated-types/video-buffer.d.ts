/**
 * TypeScript interfaces for VideoBuffer
 * Auto-generated from Python TypedDict classes
 */
/**
 * Error output structure for VideoBuffer.
 */
export interface VideoBufferError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Error output structure for VideoResizer.
 */
export interface VideoBufferVideoResizerError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Error output structure for VideoTransform.
 */
export interface VideoBufferVideoTransformError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * VideoBuffer Interface
 *
 * Video buffering node.
 */
export interface VideoBuffer {
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
    process(data: any): any;
    /** Set the current session ID for state management. */
    set_session_id(session_id: string): null;
    /** Split data into content and metadata components. */
    split_data_metadata(data: any): any | any;
}
//# sourceMappingURL=video-buffer.d.ts.map