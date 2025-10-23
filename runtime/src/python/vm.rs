//! RustPython VM Integration (Phase 1.5)
//!
//! This module provides:
//! - RustPython VM embedding and initialization
//! - VM lifecycle management (create, reuse, cleanup)
//! - VM isolation for concurrent execution
//! - Custom module injection (logging bridge, SDK helpers)
//! - Python path and sys.modules initialization
//!
//! Note: This is a simplified implementation for Phase 1.5.
//! Full Python node execution will be implemented in Phase 1.6.

use crate::{Error, Result};
use rustpython::vm::{Interpreter, PyObjectRef};
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for RustPython VM
#[derive(Debug, Clone)]
pub struct VmConfig {
    /// Python path directories to add to sys.path
    pub python_path: Vec<String>,

    /// Enable debugging output
    pub debug: bool,

    /// Maximum memory limit (in bytes, None = unlimited)
    pub memory_limit: Option<usize>,

    /// Execution timeout (in seconds, None = unlimited)
    pub timeout_seconds: Option<u64>,

    /// Enable custom logging bridge
    pub enable_logging_bridge: bool,

    /// Custom environment variables
    pub env_vars: HashMap<String, String>,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            python_path: Vec::new(),
            debug: false,
            memory_limit: None,
            timeout_seconds: None,
            enable_logging_bridge: true,
            env_vars: HashMap::new(),
        }
    }
}

/// RustPython VM manager
///
/// Manages a single RustPython VM instance with initialization,
/// lifecycle management, and custom module support.
pub struct PythonVm {
    /// The underlying RustPython interpreter
    interpreter: Interpreter,

    /// VM configuration
    config: VmConfig,

    /// VM initialization state
    initialized: bool,

    /// Persistent globals dictionary to maintain state across execute() calls
    /// Initialized once and reused for all executions
    globals: Option<Arc<PyObjectRef>>,
}

impl PythonVm {
    /// Create a new Python VM instance with default configuration
    pub fn new() -> Result<Self> {
        Self::with_config(VmConfig::default())
    }

    /// Create a new Python VM instance with custom configuration
    pub fn with_config(config: VmConfig) -> Result<Self> {
        tracing::info!("Initializing RustPython VM");

        // Create the interpreter with stdlib
        let interpreter = rustpython::InterpreterConfig::new()
            .init_stdlib()
            .interpreter();

        Ok(Self {
            interpreter,
            config,
            initialized: false,
            globals: None,
        })
    }

    /// Initialize the VM with Python path and custom modules
    ///
    /// This must be called before executing any Python code.
    pub fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            tracing::warn!("VM already initialized, skipping re-initialization");
            return Ok(());
        }

        tracing::debug!("Initializing Python VM with sys.path and custom modules");

        // Create persistent globals dictionary
        self.globals = Some(Arc::new(self.interpreter.enter(|vm| {
            vm.ctx.new_dict().into()
        })));

        // Initialize sys.path
        if !self.config.python_path.is_empty() {
            self.init_sys_path()?;
        }

        // Inject custom modules
        if self.config.enable_logging_bridge {
            self.inject_logging_bridge()?;
        }

        // Inject SDK helper modules
        self.inject_sdk_helpers()?;

        // Set environment variables
        if !self.config.env_vars.is_empty() {
            self.set_environment_vars()?;
        }

        self.initialized = true;
        tracing::info!("Python VM initialized successfully");
        Ok(())
    }

    /// Initialize sys.path with configured directories
    fn init_sys_path(&mut self) -> Result<()> {
        tracing::debug!("Adding {} paths to sys.path", self.config.python_path.len());

        let code = format!(
            r#"
import sys
paths = {:?}
for path in paths:
    if path not in sys.path:
        sys.path.append(path)
"#,
            self.config.python_path
        );

        self.interpreter
            .enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                vm.run_code_string(scope, &code, "<sys_path>".to_owned())
                    .map_err(|e| Error::PythonVm(format!("Failed to set sys.path: {:?}", e)))
            })
            .map(|_| ())
    }

    /// Inject custom logging bridge module (Phase 1.6.7)
    ///
    /// This creates a Python logging configuration that bridges Python logging to Rust tracing.
    /// Python logging calls will be captured and emitted as Rust tracing events.
    fn inject_logging_bridge(&mut self) -> Result<()> {
        tracing::debug!("Injecting logging bridge module");

        let bridge_code = r#"
import sys
import logging

class RustTracingHandler(logging.Handler):
    """Bridge Python logging to Rust tracing"""

    def __init__(self):
        super().__init__()
        self.logs = []
        # Store logs in a list that Rust can retrieve
        self._log_buffer = []

    def emit(self, record):
        """Emit a log record"""
        try:
            msg = self.format(record)
            level = record.levelname
            module = record.name

            # Store in buffer for Rust to retrieve
            self._log_buffer.append({
                'level': level,
                'message': msg,
                'module': module,
                'line': record.lineno,
                'timestamp': record.created
            })

            # Also print for immediate visibility (will be replaced with Rust FFI)
            print(f"[PY:{level}] {module}: {msg}", file=sys.stderr)

        except Exception:
            self.handleError(record)

    def get_logs(self):
        """Get buffered logs and clear buffer"""
        logs = self._log_buffer.copy()
        self._log_buffer.clear()
        return logs

# Create and configure the handler
_rust_handler = RustTracingHandler()
_rust_handler.setLevel(logging.DEBUG)

# Configure root logger to use our handler
_root_logger = logging.getLogger()
_root_logger.addHandler(_rust_handler)
_root_logger.setLevel(logging.DEBUG)

# Create a formatter
_formatter = logging.Formatter('[%(name)s] %(message)s')
_rust_handler.setFormatter(_formatter)

# Helper function to get logs from Rust
def _get_python_logs():
    """Retrieve buffered logs for Rust consumption"""
    return _rust_handler.get_logs()
"#;

        self.interpreter
            .enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                vm.run_code_string(scope, bridge_code, "<logging_bridge>".to_owned())
                    .map_err(|e| Error::PythonVm(format!("Failed to inject logging bridge: {:?}", e)))
            })
            .map(|_| ())
    }

    /// Retrieve Python logs and emit them as Rust tracing events (Phase 1.6.7)
    ///
    /// Call this periodically or after executing Python code to flush Python logs to Rust tracing.
    pub fn flush_python_logs(&mut self) -> Result<()> {
        if !self.initialized {
            return Ok(());
        }

        let code = r#"
import json
try:
    logs = _get_python_logs()
    json.dumps(logs)
except:
    "[]"
"#;

        let result = self.execute(code)?;

        if let Some(logs_str) = result.get("result").and_then(|v| v.as_str()) {
            if let Ok(logs) = serde_json::from_str::<Vec<serde_json::Value>>(logs_str) {
                for log in logs {
                    let level = log.get("level").and_then(|v| v.as_str()).unwrap_or("INFO");
                    let message = log.get("message").and_then(|v| v.as_str()).unwrap_or("");
                    let module = log.get("module").and_then(|v| v.as_str()).unwrap_or("python");

                    // Emit as Rust tracing event
                    // Note: We use the "python" target for all logs since tracing requires constant targets
                    let formatted_msg = format!("[{}] {}", module, message);
                    match level {
                        "DEBUG" => tracing::debug!(target: "python", "{}", formatted_msg),
                        "INFO" => tracing::info!(target: "python", "{}", formatted_msg),
                        "WARNING" => tracing::warn!(target: "python", "{}", formatted_msg),
                        "ERROR" => tracing::error!(target: "python", "{}", formatted_msg),
                        "CRITICAL" => tracing::error!(target: "python", "[CRITICAL] {}", formatted_msg),
                        _ => tracing::info!(target: "python", "{}", formatted_msg),
                    }
                }
            }
        }

        Ok(())
    }

    /// Inject SDK helper modules
    ///
    /// This creates Python modules that provide helpers for the RemoteMedia SDK
    fn inject_sdk_helpers(&mut self) -> Result<()> {
        tracing::debug!("Injecting SDK helper modules");

        let helpers_code = r#"
"""RemoteMedia SDK helpers for RustPython runtime"""

class NodeBase:
    """Base class for RemoteMedia nodes running in RustPython"""

    def __init__(self, **params):
        self.params = params
        self._initialized = False

    def process(self, data):
        """Process a single data item"""
        raise NotImplementedError("Nodes must implement process()")

    async def aprocess(self, data):
        """Async process method (optional)"""
        return self.process(data)

class SDKHelpers:
    """Helper functions for SDK integration"""

    @staticmethod
    def serialize(obj):
        """Serialize object to JSON-compatible format"""
        import json
        return json.dumps(obj)

    @staticmethod
    def deserialize(data):
        """Deserialize JSON data"""
        import json
        return json.loads(data)

# Make helpers available globally
__all__ = ['NodeBase', 'SDKHelpers']
"#;

        self.interpreter
            .enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                vm.run_code_string(scope, helpers_code, "<sdk_helpers>".to_owned())
                    .map_err(|e| Error::PythonVm(format!("Failed to inject SDK helpers: {:?}", e)))
            })
            .map(|_| ())
    }

    /// Set environment variables in the VM
    fn set_environment_vars(&mut self) -> Result<()> {
        tracing::debug!(
            "Setting {} environment variables",
            self.config.env_vars.len()
        );

        // Build Python code to set environment variables
        let env_assignments: Vec<String> = self
            .config
            .env_vars
            .iter()
            .map(|(k, v)| format!("os.environ[{:?}] = {:?}", k, v))
            .collect();

        let code = format!(
            r#"
import os
{}
"#,
            env_assignments.join("\n")
        );

        self.interpreter
            .enter(|vm| {
                let scope = vm.new_scope_with_builtins();
                vm.run_code_string(scope, &code, "<env_vars>".to_owned())
                    .map_err(|e| Error::PythonVm(format!("Failed to set environment variables: {:?}", e)))
            })
            .map(|_| ())
    }

    /// Execute Python code in the VM
    ///
    /// Returns the result as a JSON value
    ///
    /// Executes code using persistent globals to maintain state across calls
    pub fn execute(&mut self, code: &str) -> Result<serde_json::Value> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Executing Python code in VM");

        let globals_ref = self.globals.as_ref()
            .ok_or_else(|| Error::PythonVm("Globals not initialized".to_string()))?
            .clone();

        let result = self.interpreter.enter(|vm| {
            use rustpython::vm::compiler::Mode;
            use rustpython::vm::builtins::PyDict;

            // Get the globals dict as PyRef
            let globals_dict = (*globals_ref).clone()
                .downcast::<PyDict>()
                .map_err(|_| Error::PythonVm("Failed to downcast globals".to_string()))?;

            // Create scope with our persistent globals
            let scope = rustpython::vm::scope::Scope::with_builtins(None, globals_dict, vm);

            // Try eval mode first (for expressions like "1 + 1")
            match vm.compile(code, Mode::Eval, "<string>".to_owned()) {
                Ok(code_obj) => {
                    // It's an expression, execute and return the value
                    match vm.run_code_obj(code_obj, scope) {
                        Ok(result) => {
                            let result_str = result.str(vm)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| "None".to_string());
                            Ok(result_str)
                        }
                        Err(e) => Err(Error::PythonVm(format!("Execution error: {:?}", e))),
                    }
                }
                Err(_) => {
                    // Not an expression, compile as exec mode
                    match vm.compile(code, Mode::Exec, "<string>".to_owned()) {
                        Ok(code_obj) => {
                            // Use the same scope for exec to persist state
                            match vm.run_code_obj(code_obj, scope) {
                                Ok(_) => Ok("None".to_string()),
                                Err(e) => Err(Error::PythonVm(format!("Execution error: {:?}", e))),
                            }
                        }
                        Err(e) => Err(Error::PythonVm(format!("Compilation error: {:?}", e))),
                    }
                }
            }
        })?;

        Ok(serde_json::json!({
            "result": result,
            "status": "success"
        }))
    }

    /// Execute a Python function by name
    pub fn call_function(
        &mut self,
        module_name: &str,
        function_name: &str,
        _args: Vec<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!(
            "Calling Python function: {}.{}",
            module_name,
            function_name
        );

        // Build code to import and call the function
        let code = format!(
            r#"
import {}
result = {}.{}()
result
"#,
            module_name, module_name, function_name
        );

        self.execute(&code)
    }

    /// Check if the VM has been initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the VM configuration
    pub fn config(&self) -> &VmConfig {
        &self.config
    }

    /// Reset the VM to initial state
    pub fn reset(&mut self) -> Result<()> {
        tracing::info!("Resetting Python VM");

        // Recreate the VM
        let new_vm = Self::with_config(self.config.clone())?;
        *self = new_vm;

        Ok(())
    }

    /// Load a Python class from source code (Phase 1.6.1)
    ///
    /// This loads Python code and makes the specified class available for instantiation.
    /// Returns a handle (class name) that can be used to create instances.
    pub fn load_class(&mut self, source_code: &str, class_name: &str) -> Result<String> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Loading Python class: {}", class_name);

        // Execute the source code in the main module to make it globally available
        let code = format!(
            r#"
{}

# Verify class was defined
assert '{}' in dir(), "Class {} was not defined"
"#,
            source_code, class_name, class_name
        );

        self.execute(&code)?;

        tracing::info!("Successfully loaded Python class: {}", class_name);
        Ok(class_name.to_string())
    }

    /// Create an instance of a loaded Python class (Phase 1.6.2)
    ///
    /// This instantiates a Python class that was previously loaded via `load_class`.
    /// The params are passed as keyword arguments to the class constructor (__init__).
    ///
    /// Supports:
    /// - Null/empty params for no-argument constructors
    /// - JSON object params converted to Python kwargs
    /// - Basic type marshaling (string, number, bool, list, dict)
    pub fn create_instance(
        &mut self,
        class_name: &str,
        params: &serde_json::Value,
    ) -> Result<String> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Creating instance of Python class: {} with params: {:?}", class_name, params);

        let instance_id = format!("_instance_{}", class_name.to_lowercase());

        let code = if params.is_null() || params.as_object().map_or(true, |o| o.is_empty()) {
            // No parameters - simple instantiation
            format!("{} = {}()", instance_id, class_name)
        } else {
            // Convert JSON params to Python kwargs string
            let kwargs = self.json_to_python_kwargs(params)?;
            format!("{} = {}({})", instance_id, class_name, kwargs)
        };

        self.execute(&code)?;

        tracing::info!("Successfully created instance: {} of class {}", instance_id, class_name);
        Ok(instance_id)
    }

    /// Convert JSON params to Python kwargs string (Phase 1.6.2)
    ///
    /// Handles basic marshaling of JSON types to Python:
    /// - String → "string"
    /// - Number → number
    /// - Bool → True/False
    /// - Array → [item1, item2, ...]
    /// - Object → {"key": value}
    /// - Null → None
    fn json_to_python_kwargs(&self, params: &serde_json::Value) -> Result<String> {
        let obj = params.as_object()
            .ok_or_else(|| Error::PythonVm("Params must be a JSON object".to_string()))?;

        let kwargs_vec: Vec<String> = obj.iter()
            .map(|(k, v)| {
                let python_value = self.json_to_python_value(v);
                format!("{}={}", k, python_value)
            })
            .collect();

        Ok(kwargs_vec.join(", "))
    }

    /// Convert a JSON value to Python value representation
    fn json_to_python_value(&self, value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Null => "None".to_string(),
            serde_json::Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => {
                // Escape quotes and backslashes
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\"", escaped)
            }
            serde_json::Value::Array(arr) => {
                let items: Vec<String> = arr.iter()
                    .map(|v| self.json_to_python_value(v))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            serde_json::Value::Object(obj) => {
                let items: Vec<String> = obj.iter()
                    .map(|(k, v)| {
                        format!("\"{}\": {}", k, self.json_to_python_value(v))
                    })
                    .collect();
                format!("{{{}}}", items.join(", "))
            }
        }
    }

    /// Call a method on a Python object instance (Phase 1.6.3)
    ///
    /// This calls a method on a previously created Python instance.
    /// Primary use case: calling node.process(data) and capturing the result.
    ///
    /// Supports:
    /// - No arguments (args = null)
    /// - Single positional argument (most common: process(data))
    /// - Multiple positional arguments (array)
    /// - Keyword arguments (object)
    /// - Mixed positional and keyword arguments
    pub fn call_method(
        &mut self,
        instance_id: &str,
        method_name: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        if !self.initialized {
            self.initialize()?;
        }

        tracing::debug!("Calling method {}.{} with args: {:?}", instance_id, method_name, args);

        // Build code to call the method
        let code = if args.is_null() {
            // No arguments
            format!("{}.{}()", instance_id, method_name)
        } else if args.as_object().is_some() {
            // Keyword arguments (dict)
            let kwargs = self.json_to_python_kwargs(args)?;
            format!("{}.{}({})", instance_id, method_name, kwargs)
        } else if let Some(arr) = args.as_array() {
            // Positional arguments (array)
            let args_str: Vec<String> = arr.iter()
                .map(|v| self.json_to_python_value(v))
                .collect();
            format!("{}.{}({})", instance_id, method_name, args_str.join(", "))
        } else {
            // Single positional argument (most common case for process(data))
            let arg_str = self.json_to_python_value(args);
            format!("{}.{}({})", instance_id, method_name, arg_str)
        };

        // Execute the method call and get the result in one step
        self.execute(&code)
    }
}

impl Default for PythonVm {
    fn default() -> Self {
        Self::new().expect("Failed to create default Python VM")
    }
}

/// Pool of Python VMs for concurrent execution (Phase 1.5.4)
///
/// Provides VM isolation by maintaining multiple VM instances
pub struct VmPool {
    /// Pool of available VMs
    vms: Vec<PythonVm>,

    /// Pool configuration
    config: VmConfig,

    /// Maximum pool size
    max_size: usize,
}

impl VmPool {
    /// Create a new VM pool with specified size
    pub fn new(size: usize, config: VmConfig) -> Result<Self> {
        tracing::info!("Creating VM pool with {} instances", size);

        let mut vms = Vec::with_capacity(size);

        // Pre-create VMs
        for i in 0..size {
            tracing::debug!("Creating VM instance {}/{}", i + 1, size);
            let mut vm = PythonVm::with_config(config.clone())?;
            vm.initialize()?;
            vms.push(vm);
        }

        Ok(Self {
            vms,
            config,
            max_size: size,
        })
    }

    /// Get a VM from the pool
    pub fn acquire(&mut self) -> Result<PythonVm> {
        if let Some(vm) = self.vms.pop() {
            tracing::debug!("Acquired VM from pool, {} remaining", self.vms.len());
            Ok(vm)
        } else {
            tracing::warn!("Pool exhausted, creating new VM instance");
            let mut vm = PythonVm::with_config(self.config.clone())?;
            vm.initialize()?;
            Ok(vm)
        }
    }

    /// Return a VM to the pool
    pub fn release(&mut self, vm: PythonVm) {
        if self.vms.len() < self.max_size {
            tracing::debug!("Returning VM to pool, {} total", self.vms.len() + 1);
            self.vms.push(vm);
        } else {
            tracing::debug!("Pool full, dropping VM instance");
            // VM will be dropped
        }
    }

    /// Get the number of available VMs in the pool
    pub fn available(&self) -> usize {
        self.vms.len()
    }

    /// Get the total pool capacity
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_creation() {
        let vm = PythonVm::new();
        assert!(vm.is_ok());
    }

    #[test]
    fn test_vm_initialization() {
        let mut vm = PythonVm::new().unwrap();
        assert!(!vm.is_initialized());

        let result = vm.initialize();
        assert!(result.is_ok());
        assert!(vm.is_initialized());

        // Double initialization should be idempotent
        let result2 = vm.initialize();
        assert!(result2.is_ok());
    }

    #[test]
    fn test_vm_with_custom_config() {
        let mut config = VmConfig::default();
        config.debug = true;
        config.python_path.push("/custom/path".to_string());
        config
            .env_vars
            .insert("TEST_VAR".to_string(), "test_value".to_string());

        let vm = PythonVm::with_config(config.clone());
        assert!(vm.is_ok());

        let vm = vm.unwrap();
        assert_eq!(vm.config().debug, true);
        assert_eq!(vm.config().python_path.len(), 1);
    }

    #[test]
    fn test_vm_execute_simple() {
        let mut vm = PythonVm::new().unwrap();

        let result = vm.execute("1 + 1");
        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json["status"], "success");
        assert_eq!(json["result"], "2");
    }

    #[test]
    fn test_vm_execute_with_print() {
        let mut vm = PythonVm::new().unwrap();

        // Execute print statement
        let result = vm.execute("print('Hello from RustPython!')");
        assert!(result.is_ok());

        // Execute expression
        let result = vm.execute("42");
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["result"], "42");
    }

    #[test]
    fn test_vm_pool_creation() {
        let config = VmConfig::default();
        let pool = VmPool::new(3, config);

        assert!(pool.is_ok());
        let pool = pool.unwrap();
        assert_eq!(pool.capacity(), 3);
        assert_eq!(pool.available(), 3);
    }

    #[test]
    fn test_vm_pool_acquire_release() {
        let config = VmConfig::default();
        let mut pool = VmPool::new(2, config).unwrap();

        assert_eq!(pool.available(), 2);

        // Acquire a VM
        let vm1 = pool.acquire();
        assert!(vm1.is_ok());
        assert_eq!(pool.available(), 1);

        // Acquire another
        let vm2 = pool.acquire();
        assert!(vm2.is_ok());
        assert_eq!(pool.available(), 0);

        // Return one
        pool.release(vm1.unwrap());
        assert_eq!(pool.available(), 1);

        // Return the other
        pool.release(vm2.unwrap());
        assert_eq!(pool.available(), 2);
    }

    #[test]
    fn test_vm_reset() {
        let mut vm = PythonVm::new().unwrap();
        vm.initialize().unwrap();

        assert!(vm.is_initialized());

        let result = vm.reset();
        assert!(result.is_ok());

        // After reset, should not be initialized
        assert!(!vm.is_initialized());
    }

    #[test]
    fn test_sys_path_initialization() {
        let mut config = VmConfig::default();
        config.python_path.push("/test/path1".to_string());
        config.python_path.push("/test/path2".to_string());

        let mut vm = PythonVm::with_config(config).unwrap();
        let result = vm.initialize();

        assert!(result.is_ok());
    }

    #[test]
    fn test_environment_variables() {
        let mut config = VmConfig::default();
        config
            .env_vars
            .insert("TEST_VAR".to_string(), "test_value".to_string());

        let mut vm = PythonVm::with_config(config).unwrap();
        vm.initialize().unwrap();

        // Verify the environment variable was set
        let result = vm.execute("import os");
        assert!(result.is_ok());

        let result = vm.execute("os.environ.get('TEST_VAR', 'not_found')");
        assert!(result.is_ok());
        let json = result.unwrap();
        assert_eq!(json["result"], "test_value");
    }

    #[test]
    fn test_load_simple_class() {
        let mut vm = PythonVm::new().unwrap();

        let source_code = r#"
class SimpleNode:
    def __init__(self):
        self.value = 42

    def get_value(self):
        return self.value
"#;

        let result = vm.load_class(source_code, "SimpleNode");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "SimpleNode");
    }

    #[test]
    fn test_create_instance() {
        let mut vm = PythonVm::new().unwrap();

        let source_code = r#"
class Counter:
    def __init__(self):
        self.count = 0

    def increment(self):
        self.count += 1
        return self.count
"#;

        vm.load_class(source_code, "Counter").unwrap();

        let instance_id = vm
            .create_instance("Counter", &serde_json::Value::Null)
            .unwrap();

        assert_eq!(instance_id, "_instance_counter");
    }

    #[test]
    fn test_call_method() {
        let mut vm = PythonVm::new().unwrap();

        let source_code = r#"
class Adder:
    def __init__(self):
        self.total = 0

    def add(self, x):
        self.total += x
        return self.total
"#;

        vm.load_class(source_code, "Adder").unwrap();
        let instance_id = vm
            .create_instance("Adder", &serde_json::Value::Null)
            .unwrap();

        // Call the add method
        let result = vm.call_method(&instance_id, "add", &serde_json::json!(5));
        assert!(result.is_ok());
    }

    #[test]
    fn test_node_with_parameters() {
        let mut vm = PythonVm::new().unwrap();

        let source_code = r#"
class ProcessNode:
    def __init__(self, multiplier=1):
        self.multiplier = multiplier

    def process(self, data):
        return data * self.multiplier
"#;

        vm.load_class(source_code, "ProcessNode").unwrap();

        // Create instance with parameters
        let params = serde_json::json!({
            "multiplier": 3
        });

        let instance_id = vm.create_instance("ProcessNode", &params).unwrap();
        assert_eq!(instance_id, "_instance_processnode");
    }

    #[test]
    fn test_complete_node_workflow() {
        let mut vm = PythonVm::new().unwrap();

        // Define a RemoteMedia-style node
        let source_code = r#"
class TransformNode:
    def __init__(self, operation="identity"):
        self.operation = operation
        self.processed_count = 0

    def process(self, data):
        self.processed_count += 1
        if self.operation == "double":
            return data * 2
        elif self.operation == "square":
            return data ** 2
        else:
            return data

    def get_stats(self):
        return {"processed": self.processed_count}
"#;

        // Load the class
        vm.load_class(source_code, "TransformNode").unwrap();

        // Create instance with params
        let params = serde_json::json!({"operation": "double"});
        let instance_id = vm.create_instance("TransformNode", &params).unwrap();

        // Process some data
        let result1 = vm.call_method(&instance_id, "process", &serde_json::json!(5));
        assert!(result1.is_ok());

        let result2 = vm.call_method(&instance_id, "process", &serde_json::json!(10));
        assert!(result2.is_ok());

        // Get stats
        let stats = vm.call_method(&instance_id, "get_stats", &serde_json::Value::Null);
        assert!(stats.is_ok());
    }
}
