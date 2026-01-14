/**
 * Configuration interface for TextProcessorNode
 * 
    Text Processor node - performs various text processing operations.
    
    Expects input data in the format:
    {
        "text": "string_to_process",
        "operations": ["uppercase", "lowercase", "reverse", "word_count", "char_count"]
    }
    
 */
export interface TextProcessorNodeConfig {
  // No configuration parameters
}
