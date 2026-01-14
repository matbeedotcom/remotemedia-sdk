/**
 * TypeScript interfaces for AudioResampler
 * Auto-generated from Python TypedDict classes
 */
/**
 * Error output structure for AudioResampler.
 */
export interface AudioResamplerError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Input data structure for AudioResampler.
 */
export interface AudioResamplerInput {
    audio_data: Float32Array | number[];
    sample_rate: number;
}
/**
 * Output data structure for AudioResampler.
 */
export interface AudioResamplerOutput {
    audio_data: Float32Array | number[];
    sample_rate: number;
    processed_by: string;
}
/**
 * Error output structure for AudioTransform.
 */
export interface AudioResamplerAudioTransformError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Input data structure for AudioTransform.
 */
export interface AudioResamplerAudioTransformInput {
    audio_data: Float32Array | number[];
    sample_rate: number;
}
/**
 * Output data structure for AudioTransform.
 */
export interface AudioResamplerAudioTransformOutput {
    audio_data: Float32Array | number[];
    sample_rate: number;
    processed_by: string;
    node_config: Record<string, any>;
}
/**
 * Error output structure for ExtractAudioDataNode.
 */
export interface AudioResamplerExtractAudioDataError {
    error: string;
    input: any;
    processed_by: string;
}
/**
 * Input data structure for ExtractAudioDataNode.
 */
export interface AudioResamplerExtractAudioDataInput {
    audio_data: Float32Array | number[];
    sample_rate: number;
}
/**
 * AudioResampler Interface
 *
 * Audio resampling node.
 */
export interface AudioResampler {
    target_sample_rate?: number;
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
    process(data: AudioResamplerInput | any): AudioResamplerOutput | AudioResamplerError;
    /** Set the current session ID for state management. */
    set_session_id(session_id: string): null;
    /** Split data into content and metadata components. */
    split_data_metadata(data: any): any | any;
}
//# sourceMappingURL=audio-resampler.d.ts.map