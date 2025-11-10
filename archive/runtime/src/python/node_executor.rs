//! Python Node Executor (Deprecated - RustPython VM removed in v0.2.0)
//!
//! This module provided high-level execution of Python nodes in RustPython VM.
//! As of v0.2.0, we use CPython via PyO3 instead (see cpython_executor.rs).
//!
//! This file is kept for reference but the functionality has been moved to:
//! - cpython_executor.rs - Direct CPython execution via PyO3
//! - For node execution, use CPythonNodeExecutor instead

// This module is deprecated - kept for compilation compatibility only
#![allow(dead_code)]

use crate::Result;
use serde_json::Value;
use std::collections::HashMap;

/// DEPRECATED: Python node instance (RustPython-based)
///
/// Use CPythonNodeExecutor instead.
pub struct PythonNodeInstance {
    instance_id: String,
    class_name: String,
    params: Value,
}

impl PythonNodeInstance {
    pub fn from_source(_source: &str, class: &str, params: Value) -> Result<Self> {
        Ok(Self {
            instance_id: String::new(),
            class_name: class.to_string(),
            params,
        })
    }

    pub fn get_info(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}
