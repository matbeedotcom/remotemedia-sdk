//! CPython Node wrapper for NodeRegistry (Phase 5: T065-T067)
//!
//! This module provides a factory wrapper around CPythonNodeExecutor to enable
//! Python fallback nodes in the NodeRegistry system.
//!
//! Note: CPythonNodeExecutor implements crate::nodes::NodeExecutor, but the
//! NodeRegistry expects crate::executor::node_executor::NodeExecutor. This
//! wrapper adapts between the two trait definitions.

use crate::executor::node_executor::{NodeExecutor as ExecutorNodeExecutor, NodeContext as ExecutorNodeContext};
use crate::nodes::{NodeExecutor as NodesNodeExecutor, NodeContext as NodesNodeContext};
use crate::python::cpython_executor::CPythonNodeExecutor;
use crate::nodes::registry::NodeFactory;
use crate::error::{Error, Result};
use serde_json::Value;
use std::sync::Arc;

/// Adapter to bridge CPythonNodeExecutor (implements nodes::NodeExecutor)
/// to work with NodeRegistry (expects executor::node_executor::NodeExecutor)
pub struct CPythonNodeAdapter {
    executor: CPythonNodeExecutor,
}

impl CPythonNodeAdapter {
    pub fn new(node_type: impl Into<String>) -> Self {
        Self {
            executor: CPythonNodeExecutor::new(node_type),
        }
    }

    /// Convert ExecutorNodeContext to NodesNodeContext
    fn convert_context(ctx: &ExecutorNodeContext) -> NodesNodeContext {
        NodesNodeContext {
            node_id: ctx.node_id.clone(),
            node_type: ctx.node_type.clone(),
            params: ctx.params.clone(),
            session_id: None,
            metadata: ctx.metadata.clone(),
        }
    }
}

#[async_trait::async_trait]
impl ExecutorNodeExecutor for CPythonNodeAdapter {
    async fn initialize(&mut self, ctx: &ExecutorNodeContext) -> Result<()> {
        let nodes_ctx = Self::convert_context(ctx);
        self.executor.initialize(&nodes_ctx).await
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        self.executor.process(input).await
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.executor.cleanup().await
    }

    fn is_streaming(&self) -> bool {
        self.executor.is_streaming()
    }

    async fn finish_streaming(&mut self) -> Result<Vec<Value>> {
        self.executor.finish_streaming().await
    }
}

/// Factory for creating CPython nodes (Python fallback)
pub struct CPythonNodeFactory {
    node_type: String,
}

impl CPythonNodeFactory {
    /// Create a new CPython node factory
    pub fn new(node_type: impl Into<String>) -> Self {
        Self {
            node_type: node_type.into(),
        }
    }
}

impl NodeFactory for CPythonNodeFactory {
    fn create(&self, _params: Value) -> Result<Box<dyn ExecutorNodeExecutor>> {
        Ok(Box::new(CPythonNodeAdapter::new(self.node_type.clone())))
    }

    fn node_type(&self) -> &str {
        &self.node_type
    }

    fn is_rust_native(&self) -> bool {
        false // This is a Python fallback
    }
}

/// Helper functions for Python data conversion (T067)

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Convert a HashMap of JSON Values to a Python dictionary
///
/// This uses the existing marshal.rs infrastructure but provides
/// a convenient HashMap interface for node inputs.
pub fn inputs_to_pydict(py: Python, inputs: &std::collections::HashMap<String, Value>) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    
    for (key, value) in inputs {
        let py_value = crate::python::marshal::json_to_python(py, value)?;
        dict.set_item(key, py_value)?;
    }
    
    Ok(dict.into())
}

/// Convert a Python dictionary to a HashMap of JSON Values
///
/// This uses the existing marshal.rs infrastructure but provides
/// a convenient HashMap interface for node outputs.
pub fn pydict_to_outputs(py: Python, dict: &Bound<'_, PyDict>) -> PyResult<std::collections::HashMap<String, Value>> {
    let mut outputs = std::collections::HashMap::new();
    
    for (key, value) in dict.iter() {
        let key_str = key.extract::<String>()?;
        let json_value = crate::python::marshal::python_to_json(py, value.as_any())?;
        outputs.insert(key_str, json_value);
    }
    
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpython_node_factory_creation() {
        let factory = CPythonNodeFactory::new("TestNode");
        assert_eq!(factory.node_type(), "TestNode");
        assert!(!factory.is_rust_native());
    }

    #[test]
    fn test_cpython_node_factory_create() {
        let factory = CPythonNodeFactory::new("PassThrough");
        let result = factory.create(Value::Null);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inputs_to_pydict() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            let mut inputs = std::collections::HashMap::new();
            inputs.insert("key1".to_string(), Value::String("value1".to_string()));
            inputs.insert("key2".to_string(), Value::Number(42.into()));

            let result = inputs_to_pydict(py, &inputs);
            assert!(result.is_ok());

            let dict = result.unwrap();
            let bound_dict = dict.bind(py);
            assert_eq!(bound_dict.len(), 2);
            assert!(bound_dict.contains("key1").unwrap());
            assert!(bound_dict.contains("key2").unwrap());
        });
    }

    #[test]
    fn test_pydict_to_outputs() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            let dict = PyDict::new(py);
            dict.set_item("result", "test_value").unwrap();
            dict.set_item("count", 123).unwrap();

            let result = pydict_to_outputs(py, &dict);
            assert!(result.is_ok());

            let outputs = result.unwrap();
            assert_eq!(outputs.len(), 2);
            assert!(outputs.contains_key("result"));
            assert!(outputs.contains_key("count"));
        });
    }

    #[test]
    fn test_inputs_outputs_roundtrip() {
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            // Create inputs
            let mut inputs = std::collections::HashMap::new();
            inputs.insert("data".to_string(), Value::Array(vec![
                Value::Number(1.into()),
                Value::Number(2.into()),
                Value::Number(3.into()),
            ]));
            inputs.insert("name".to_string(), Value::String("test".to_string()));

            // Convert to Python dict
            let py_dict = inputs_to_pydict(py, &inputs).unwrap();
            let bound_dict = py_dict.bind(py);

            // Convert back to outputs
            let outputs = pydict_to_outputs(py, bound_dict).unwrap();

            // Verify roundtrip
            assert_eq!(outputs.len(), inputs.len());
            assert_eq!(outputs.get("name"), inputs.get("name"));
            assert_eq!(outputs.get("data"), inputs.get("data"));
        });
    }
}
