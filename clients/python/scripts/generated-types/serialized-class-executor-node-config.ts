/**
 * Configuration interface for SerializedClassExecutorNode
 * 
    Serialized Class Executor node - executes cloudpickle-serialized Python classes.
    
    This node implements the Phase 3 requirement for executing user-defined Python
    classes with local dependencies using cloudpickle serialization.
    
    Expects input data in the format:
    {
        "serialized_object": "base64_encoded_cloudpickle_data",
        "method_name": "method_to_call",
        "method_args": [args],
        "method_kwargs": {kwargs}
    }
    
 */
export interface SerializedClassExecutorNodeConfig {
  // No configuration parameters
}
