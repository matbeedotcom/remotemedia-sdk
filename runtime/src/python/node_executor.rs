//! Python Node Executor (Phase 1.6)
//!
//! This module provides high-level execution of Python nodes in RustPython VM.
//! It handles:
//! - Node lifecycle (load, initialize, process, cleanup)
//! - Instance state preservation across calls
//! - Async node support (aprocess)
//! - Streaming nodes (generators/async generators)
//! - Error handling and exception marshaling

use crate::Result;
use super::vm::PythonVm;
use serde_json::Value;
use std::collections::HashMap;

/// Python node instance with lifecycle management
pub struct PythonNodeInstance {
    /// The underlying VM
    vm: PythonVm,

    /// Instance ID in the VM
    instance_id: String,

    /// Node class name
    class_name: String,

    /// Whether the node is initialized
    initialized: bool,

    /// Node configuration/parameters
    params: Value,
}

impl PythonNodeInstance {
    /// Create a new Python node instance from source code
    ///
    /// Phase 1.6.1: Load class
    /// Phase 1.6.2: Invoke __init__ with parameters
    pub fn from_source(
        source_code: &str,
        class_name: &str,
        params: Value,
    ) -> Result<Self> {
        let mut vm = PythonVm::new()?;

        // Load the class from source code
        vm.load_class(source_code, class_name)?;

        // Create instance with parameters (calls __init__)
        let instance_id = vm.create_instance(class_name, &params)?;

        tracing::info!("Created Python node instance: {} ({})", instance_id, class_name);

        Ok(Self {
            vm,
            instance_id,
            class_name: class_name.to_string(),
            initialized: false,
            params,
        })
    }

    /// Initialize the node (Phase 1.6.6 - State preservation)
    ///
    /// Calls node.initialize() if the method exists.
    /// This is separate from __init__ and is called once before processing.
    pub fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            tracing::warn!("Node {} already initialized", self.instance_id);
            return Ok(());
        }

        tracing::debug!("Initializing Python node: {}", self.instance_id);

        // Check if the node has an initialize method
        let check_code = format!(
            "hasattr({}, 'initialize')",
            self.instance_id
        );

        let has_init = self.vm.execute(&check_code)?;

        if let Some(result) = has_init.get("result").and_then(|v| v.as_str()) {
            if result == "True" {
                // Call initialize()
                self.vm.call_method(&self.instance_id, "initialize", &Value::Null)?;
                tracing::info!("Node {} initialized successfully", self.instance_id);
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// Process data through the node (Phase 1.6.3)
    ///
    /// Calls node.process(data) and captures the result.
    /// The node instance state is preserved across calls.
    pub fn process(&mut self, data: Value) -> Result<Value> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Processing data through node: {}", self.instance_id);

        // Execute the process method and serialize to JSON in one go
        // Use exec to run the code, then eval to get the variable value
        let exec_code = format!(
            r#"
import json
_node_result = {}.process({})
if _node_result is None:
    _json_output = json.dumps(None)
elif isinstance(_node_result, (str, int, float, bool)):
    _json_output = json.dumps(_node_result)
elif isinstance(_node_result, (list, dict)):
    _json_output = json.dumps(_node_result)
else:
    try:
        _json_output = json.dumps(str(_node_result))
    except Exception as e:
        _json_output = json.dumps({{"__error__": str(e), "__type__": str(type(_node_result))}})"#,
            self.instance_id,
            self.vm_json_to_python_value(&data)
        );

        // Execute the code (stores result in _json_output)
        self.vm.execute(&exec_code)?;

        // Now retrieve the _json_output variable by evaluating it
        let result = self.vm.execute("_json_output")?;

        // Flush Python logs to Rust tracing (Phase 1.6.7)
        let _ = self.vm.flush_python_logs();

        // Extract and parse the JSON result
        if let Some(result_str) = result.get("result").and_then(|v| v.as_str()) {
            // Parse the JSON string
            let parsed: Value = serde_json::from_str(result_str)
                .unwrap_or_else(|_| serde_json::json!(result_str));
            Ok(parsed)
        } else {
            Ok(result)
        }
    }

    /// Check if node supports async processing (Phase 1.6.4)
    pub fn has_aprocess(&mut self) -> Result<bool> {
        let check_code = format!(
            "hasattr({}, 'aprocess')",
            self.instance_id
        );

        let result = self.vm.execute(&check_code)?;

        if let Some(result_str) = result.get("result").and_then(|v| v.as_str()) {
            Ok(result_str == "True")
        } else {
            Ok(false)
        }
    }

    /// Process data asynchronously (Phase 1.6.4)
    ///
    /// Calls node.aprocess(data) for async nodes.
    /// Note: RustPython's async support is limited, so this may fall back to process()
    pub fn aprocess(&mut self, data: Value) -> Result<Value> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Async processing data through node: {}", self.instance_id);

        // Try to call aprocess first
        if self.has_aprocess()? {
            // For now, we'll use a simple asyncio.run wrapper
            // TODO: Improve async support when RustPython's async implementation matures
            let code = format!(
                r#"
import asyncio
try:
    # Try to run async method
    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)
    _async_result = loop.run_until_complete({}.aprocess({}))
    loop.close()
    _async_result
except Exception as e:
    # Fall back to sync process
    {}.process({})
"#,
                self.instance_id,
                self.vm_json_to_python_value(&data),
                self.instance_id,
                self.vm_json_to_python_value(&data)
            );

            let result = self.vm.execute(&code)?;

            if let Some(inner_result) = result.get("result") {
                Ok(inner_result.clone())
            } else {
                Ok(result)
            }
        } else {
            // Fall back to synchronous process
            self.process(data)
        }
    }

    /// Check if node is a generator/streaming node (Phase 1.6.5)
    pub fn is_streaming(&mut self) -> Result<bool> {
        let check_code = format!(
            r#"
import inspect
_is_gen = inspect.isgeneratorfunction({}.process) if hasattr({}, 'process') else False
_is_async_gen = inspect.isasyncgenfunction({}.process) if hasattr({}, 'process') else False
_is_gen or _is_async_gen
"#,
            self.instance_id, self.instance_id,
            self.instance_id, self.instance_id
        );

        let result = self.vm.execute(&check_code)?;

        if let Some(result_str) = result.get("result").and_then(|v| v.as_str()) {
            Ok(result_str == "True")
        } else {
            Ok(false)
        }
    }

    /// Process streaming data (Phase 1.6.5)
    ///
    /// For generator/async generator nodes, this method collects all yielded values.
    /// Note: This is a simplified implementation; full streaming support requires
    /// more sophisticated iterator handling.
    pub fn process_streaming(&mut self, data: Value) -> Result<Vec<Value>> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Streaming processing data through node: {}", self.instance_id);

        // Call process(data) and collect generator results
        let exec_code = format!(
            r#"
import json
_stream_results = []
try:
    _gen = {}.process({})

    # Check if it's a generator
    if hasattr(_gen, '__iter__') and not isinstance(_gen, (str, bytes, dict)):
        for item in _gen:
            _stream_results.append(item)
    else:
        # Not a generator, just return the single result
        _stream_results.append(_gen)
except Exception as e:
    _stream_results.append({{"error": str(e)}})

_stream_json = json.dumps(_stream_results)
"#,
            self.instance_id,
            self.vm_json_to_python_value(&data)
        );

        // Execute the code (stores result in _stream_json)
        self.vm.execute(&exec_code)?;

        // Now retrieve the _stream_json variable
        let result = self.vm.execute("_stream_json")?;

        // Parse the results
        if let Some(result_str) = result.get("result").and_then(|v| v.as_str()) {
            let parsed: Vec<Value> = serde_json::from_str(result_str)
                .unwrap_or_else(|_| vec![serde_json::json!(result_str)]);
            Ok(parsed)
        } else {
            Ok(vec![result])
        }
    }

    /// Clean up the node (Phase 1.6.6)
    ///
    /// Calls node.cleanup() if the method exists.
    pub fn cleanup(&mut self) -> Result<()> {
        tracing::debug!("Cleaning up Python node: {}", self.instance_id);

        // Check if the node has a cleanup method
        let check_code = format!(
            "hasattr({}, 'cleanup')",
            self.instance_id
        );

        let has_cleanup = self.vm.execute(&check_code)?;

        if let Some(result) = has_cleanup.get("result").and_then(|v| v.as_str()) {
            if result == "True" {
                // Call cleanup()
                self.vm.call_method(&self.instance_id, "cleanup", &Value::Null)?;
                tracing::info!("Node {} cleaned up successfully", self.instance_id);
            }
        }

        self.initialized = false;
        Ok(())
    }

    /// Get node information
    pub fn get_info(&self) -> HashMap<String, String> {
        let mut info = HashMap::new();
        info.insert("instance_id".to_string(), self.instance_id.clone());
        info.insert("class_name".to_string(), self.class_name.clone());
        info.insert("initialized".to_string(), self.initialized.to_string());
        info
    }

    /// Helper to convert JSON value to Python value string (delegates to VM)
    fn vm_json_to_python_value(&self, value: &Value) -> String {
        // This is a workaround since json_to_python_value is private
        // We'll reconstruct it here
        match value {
            Value::Null => "None".to_string(),
            Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            }
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter()
                    .map(|v| self.vm_json_to_python_value(v))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            Value::Object(obj) => {
                let items: Vec<String> = obj.iter()
                    .map(|(k, v)| {
                        format!("\"{}\": {}", k, self.vm_json_to_python_value(v))
                    })
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation_and_execution() {
        let source_code = r#"
class SimpleNode:
    def __init__(self, multiplier=1):
        self.multiplier = multiplier
        self.count = 0

    def process(self, data):
        self.count += 1
        return data * self.multiplier
"#;

        let params = serde_json::json!({
            "multiplier": 3
        });

        let mut node = PythonNodeInstance::from_source(
            source_code,
            "SimpleNode",
            params
        ).unwrap();

        // Process some data
        let result1 = node.process(serde_json::json!(5)).unwrap();
        assert_eq!(result1, serde_json::json!(15));

        // State should be preserved
        let result2 = node.process(serde_json::json!(10)).unwrap();
        assert_eq!(result2, serde_json::json!(30));

        node.cleanup().unwrap();
    }

    #[test]
    fn test_node_with_initialize() {
        let source_code = r#"
class NodeWithInit:
    def __init__(self):
        self.ready = False

    def initialize(self):
        self.ready = True
        return "initialized"

    def process(self, data):
        if not self.ready:
            return {"error": "not initialized"}
        return {"data": data, "ready": self.ready}
"#;

        let mut node = PythonNodeInstance::from_source(
            source_code,
            "NodeWithInit",
            Value::Null
        ).unwrap();

        node.initialize().unwrap();

        let result = node.process(serde_json::json!("test")).unwrap();
        assert!(result.is_object());

        node.cleanup().unwrap();
    }

    #[test]
    fn test_streaming_node() {
        let source_code = r#"
class StreamingNode:
    def __init__(self):
        pass

    def process(self, data):
        # Generator function
        for i in range(data):
            yield i * 2
"#;

        let mut node = PythonNodeInstance::from_source(
            source_code,
            "StreamingNode",
            Value::Null
        ).unwrap();

        let results = node.process_streaming(serde_json::json!(5)).unwrap();

        // Should yield [0, 2, 4, 6, 8]
        assert_eq!(results.len(), 5);

        node.cleanup().unwrap();
    }
}
