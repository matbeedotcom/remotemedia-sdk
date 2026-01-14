/**
 * TypeScript interfaces for AudioBuffer
 * Auto-generated from Python TypedDict classes
 */
/**
 * Error output structure for AudioResampler.
 */
export interface AudioBufferAudioResamplerError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Input data structure for AudioResampler.
 */
export interface AudioBufferAudioResamplerInput {
    audio_data: Float32Array | number[];
    sample_rate: number;
}
/**
 * Output data structure for AudioResampler.
 */
export interface AudioBufferAudioResamplerOutput {
    audio_data: Float32Array | number[];
    sample_rate: number;
    processed_by: string;
}
/**
 * Error output structure for AudioTransform.
 */
export interface AudioBufferAudioTransformError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Input data structure for AudioTransform.
 */
export interface AudioBufferAudioTransformInput {
    audio_data: Float32Array | number[];
    sample_rate: number;
}
/**
 * Output data structure for AudioTransform.
 */
export interface AudioBufferAudioTransformOutput {
    audio_data: Float32Array | number[];
    sample_rate: number;
    processed_by: string;
    node_config: Record<string, any>;
}
/**
 * Error output structure for ExtractAudioDataNode.
 */
export interface AudioBufferExtractAudioDataError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Input data structure for ExtractAudioDataNode.
 */
export interface AudioBufferExtractAudioDataInput {
    audio_data: Float32Array | number[];
    sample_rate: number;
}
/**
 * AudioBuffer Interface
 *
 *
    Audio buffering node that accumulates audio data until a target size is reached.
    
 */
export interface AudioBuffer {
    /** The number of samples to buffer before outputting. */
    buffer_size_samples: number;
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
    process(data: AudioBufferAudioResamplerInput | any): AudioBufferAudioResamplerOutput | AudioBufferAudioResamplerError;
    /** Set the current session ID for state management. */
    set_session_id(session_id: string): null;
    /** Split data into content and metadata components. */
    split_data_metadata(data: any): any | any;
}
//# sourceMappingURL=audio-buffer.d.ts.map