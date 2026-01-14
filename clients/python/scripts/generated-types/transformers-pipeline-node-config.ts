/**
 * Configuration interface for TransformersPipelineNode
 * 
    A generic node that wraps a Hugging Face Transformers pipeline.

    This node can be configured to run various tasks like text-classification,
    automatic-speech-recognition, etc., by leveraging the `transformers.pipeline`
    factory.

    See: https://huggingface.co/docs/transformers/main_classes/pipelines
    
 */
export interface TransformersPipelineNodeConfig {
  // No configuration parameters
}
