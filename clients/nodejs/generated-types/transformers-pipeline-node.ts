/**
 * TypeScript interfaces for TransformersPipelineNode
 * Auto-generated from Python TypedDict classes
 */


/**
 * TransformersPipelineNode Interface
 * 
 * 
    A generic node that wraps a Hugging Face Transformers pipeline.

    This node can be configured to run various tasks like text-classification,
    automatic-speech-recognition, etc., by leveraging the `transformers.pipeline`
    factory.

    See: https://huggingface.co/docs/transformers/main_classes/pipelines
    
 */
export interface TransformersPipelineNode {
  // Configuration properties (constructor arguments)
  /** The device to run the model on (e.g., "cpu", "cuda", 0). */
  device?: any;
  /** The model identifier from the Hugging Face Hub. */
  model?: string;
  /** Extra keyword arguments for the model. */
  model_kwargs?: Record<string, any>;
  /** The task for the pipeline (e.g., "text-classification"). */
  task: string;
  /** The torch dtype to use (e.g., "float16", "bfloat16"). */
  torch_dtype?: string;
  args: any;
  /** Whether to enable state management (default: True) (default: true) */
  enable_state?: boolean;
  /** Maximum number of concurrent sessions (default: None/unlimited) */
  max_sessions?: number;
  /** Optional name for the node (defaults to class name) */
  name?: string;
  /** Time-to-live for session states (default: 24 hours) */
  state_ttl?: any;

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
