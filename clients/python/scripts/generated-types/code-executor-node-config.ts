/**
 * Configuration interface for CodeExecutorNode
 * 
    Code Executor node - executes arbitrary Python code.
    
    WARNING: This is INSECURE and should only be used in trusted environments!
    
    Expects input data in the format:
    {
        "code": "python_code_string",
        "input": optional_input_data
    }
    
 */
export interface CodeExecutorNodeConfig {
  // No configuration parameters
}
