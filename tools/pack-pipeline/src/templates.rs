//! Code generation templates for Python packages
//!
//! These templates generate packages that depend on `remotemedia-ffi` to leverage
//! the existing pipeline execution infrastructure, marshaling utilities, and numpy support.

/// Generate Cargo.toml for the Python package
/// 
/// The generated package depends on local crates:
/// - `remotemedia-ffi` for marshaling and Python FFI utilities
/// - `remotemedia-core` for pipeline execution
/// 
/// Dependencies use path references to compile everything into the wheel.
pub fn generate_cargo_toml(package_name: &str, version: &str, workspace_root: &str) -> String {
    format!(
        r#"[package]
name = "{package_name}"
version = "{version}"
edition = "2021"
description = "RemoteMedia pipeline package - auto-generated"

[lib]
name = "{package_name}"
crate-type = ["cdylib"]

[dependencies]
# RemoteMedia FFI transport - provides Python bindings and marshaling
remotemedia-ffi = {{ path = "{workspace_root}/crates/transports/ffi", features = ["extension-module"] }}

# RemoteMedia core - provides PipelineExecutor and RuntimeData types
remotemedia-core = {{ path = "{workspace_root}/crates/core" }}

# Re-export required crates for our bindings
pyo3 = {{ version = "0.26", features = ["abi3-py310", "extension-module"] }}
pyo3-async-runtimes = {{ version = "0.26", features = ["tokio-runtime"] }}

# Async runtime
tokio = {{ version = "1.35", features = ["sync", "rt-multi-thread"] }}

# Serialization
serde_json = "1.0"
serde_yaml = "0.9"

# Error handling
anyhow = "1.0"

# Logging (for embedded node initialization)
tracing = "0.1"
"#,
        package_name = package_name,
        version = version,
        workspace_root = workspace_root,
    )
}

/// Generate pyproject.toml for Maturin build
pub fn generate_pyproject_toml(
    package_name: &str,
    version: &str,
    description: &str,
    python_requires: &str,
    extra_deps: &[String],
) -> String {
    let deps_str = if extra_deps.is_empty() {
        String::new()
    } else {
        format!(
            "\ndependencies = [\n{}]\n",
            extra_deps.iter()
                .map(|d| format!("    \"{}\",", d))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    format!(
        r#"[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "{package_name}"
version = "{version}"
description = "{description}"
requires-python = "{python_requires}"
license = {{ text = "MIT" }}
classifiers = [
    "Development Status :: 4 - Beta",
    "Intended Audience :: Developers",
    "Programming Language :: Rust",
    "Programming Language :: Python :: Implementation :: CPython",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
]
{deps}
[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "numpy>=1.24",
]

[tool.maturin]
module-name = "{package_name}"
python-source = "python"
python-packages = ["{package_name}", "remotemedia"]
strip = true
"#,
        package_name = package_name,
        version = version,
        description = description.replace('"', "'"),
        python_requires = python_requires,
        deps = deps_str,
    )
}

/// Generate lib.rs that wraps remotemedia-ffi for the embedded pipeline
pub fn generate_lib_rs(
    package_name: &str,
    class_name: &str,
    description: &str,
    is_streaming: bool,
) -> String {
    // Delegate to the version with no embedded nodes
    generate_lib_rs_with_nodes(package_name, class_name, description, is_streaming, &[])
}

/// Generate lib.rs with embedded Python nodes
/// 
/// This version embeds Python node source files directly into the binary
/// and registers them at module initialization time.
pub fn generate_lib_rs_with_nodes(
    package_name: &str,
    class_name: &str,
    description: &str,
    is_streaming: bool,
    embedded_python_files: &[String],
) -> String {
    let streaming_impl = if is_streaming {
        generate_streaming_session(class_name)
    } else {
        generate_batch_api(class_name)
    };

    // Generate the embedded Python nodes constant
    let embedded_nodes_const = generate_embedded_nodes_const(embedded_python_files);
    
    // Generate the init function for embedded nodes
    let init_embedded_nodes_fn = generate_init_embedded_nodes_fn(embedded_python_files);

    format!(
        r##"//! {description}
//!
//! Auto-generated RemoteMedia pipeline package.
//! Built on top of `remotemedia-ffi` for pipeline execution.
//!
//! # Example
//!
//! ```python
//! import asyncio
//! from {package_name} import {class_name}Session
//!
//! async def main():
//!     session = {class_name}Session()
//!     await session.send(audio_data)
//!     result = await session.recv()
//!     print(result)
//!
//! asyncio.run(main())
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes::tokio::future_into_py;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::Once;

// Re-export marshal utilities from remotemedia-ffi
use remotemedia_ffi::marshal::{{python_to_runtime_data, runtime_data_to_python}};

/// Embedded pipeline YAML (compiled at package build time)
const PIPELINE_YAML: &str = include_str!("pipeline.yaml");

{embedded_nodes_const}

{init_embedded_nodes_fn}

{streaming_impl}

/// Get the embedded pipeline YAML
#[pyfunction]
fn get_pipeline_yaml() -> String {{
    PIPELINE_YAML.to_string()
}}

/// Get package version
#[pyfunction]
fn get_version() -> String {{
    env!("CARGO_PKG_VERSION").to_string()
}}

/// Python module definition
#[pymodule]
fn {package_name}(m: &Bound<'_, PyModule>) -> PyResult<()> {{
    // Initialize embedded Python nodes on first import
    init_embedded_nodes(m.py())?;
    
    m.add_class::<{class_name}Session>()?;
    m.add_function(wrap_pyfunction!(get_pipeline_yaml, m)?)?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    m.add_function(wrap_pyfunction!(process, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("PIPELINE_YAML", PIPELINE_YAML)?;
    Ok(())
}}
"##,
        description = description,
        package_name = package_name,
        class_name = class_name,
        streaming_impl = streaming_impl,
        embedded_nodes_const = embedded_nodes_const,
        init_embedded_nodes_fn = init_embedded_nodes_fn,
    )
}

/// Generate the EMBEDDED_PYTHON_BYTECODE constant with include_bytes!() macros
fn generate_embedded_nodes_const(embedded_files: &[String]) -> String {
    if embedded_files.is_empty() {
        return r#"/// No embedded Python nodes (all nodes are built-in Rust)
const EMBEDDED_PYTHON_BYTECODE: &[(&str, &[u8])] = &[];"#.to_string();
    }
    
    let entries: Vec<String> = embedded_files.iter()
        .map(|file| {
            // Convert .pyc path to module name (e.g., "tts.pyc" -> "tts", "ml/lfm2_audio.pyc" -> "ml.lfm2_audio")
            let module_name = file
                .trim_end_matches(".pyc")
                .replace('/', ".");
            format!(r#"    ("{}", include_bytes!("nodes/{}"))"#, module_name, file)
        })
        .collect();
    
    format!(
        r#"/// Embedded Python node bytecode (compiled at pack time)
/// Format: (module_name, bytecode)
const EMBEDDED_PYTHON_BYTECODE: &[(&str, &[u8])] = &[
{}
];"#,
        entries.join(",\n")
    )
}

/// Generate the init_embedded_nodes function
fn generate_init_embedded_nodes_fn(embedded_files: &[String]) -> String {
    if embedded_files.is_empty() {
        return r#"/// Initialize embedded Python nodes (no-op when no nodes embedded)
fn init_embedded_nodes(_py: Python<'_>) -> PyResult<()> {
    Ok(())
}"#.to_string();
    }

    r##"/// Initialize embedded Python nodes by loading pre-compiled bytecode
/// 
/// This function is called once when the module is first imported.
/// It loads each embedded Python bytecode file using marshal.loads() and executes it
/// to register the node classes.
///
/// The bytecode was compiled at pack time with all dependencies available,
/// so this is a self-contained executable - no external remotemedia package needed.
fn init_embedded_nodes(py: Python<'_>) -> PyResult<()> {
    static INIT: Once = Once::new();
    static mut INIT_RESULT: Option<PyResult<()>> = None;
    
    INIT.call_once(|| {
        let result = (|| -> PyResult<()> {
            use pyo3::types::{PyBytes, PyModule, PyAnyMethods};
            
            tracing::info!("Loading {} embedded Python bytecode modules...", EMBEDDED_PYTHON_BYTECODE.len());
            
            // Get marshal.loads and exec builtins
            let marshal = py.import("marshal")?;
            let loads_fn = marshal.getattr("loads")?;
            let builtins = py.import("builtins")?;
            let exec_fn = builtins.getattr("exec")?;
            
            for (module_name, bytecode) in EMBEDDED_PYTHON_BYTECODE {
                // Use a namespaced module name to avoid conflicts with the package
                // e.g., "tts" becomes "_embedded_nodes.tts"
                let namespaced_module = format!("_embedded_nodes.{}", module_name);
                
                tracing::debug!("Loading embedded bytecode module: {} as {} ({} bytes)", 
                    module_name, namespaced_module, bytecode.len());
                
                // Step 1: Convert bytecode to Python bytes and load with marshal
                let py_bytes = PyBytes::new(py, bytecode);
                let code_object = loads_fn.call1((py_bytes,))
                    .map_err(|e| {
                        tracing::error!("Failed to load bytecode for '{}': {}", module_name, e);
                        e
                    })?;
                
                // Step 2: Create a module to execute the code in
                let module = PyModule::new(py, &namespaced_module)?;
                let globals = module.dict();
                
                // Set __name__ and __file__ for the module
                globals.set_item("__name__", &namespaced_module)?;
                globals.set_item("__file__", format!("<embedded>/{}.pyc", module_name))?;
                globals.set_item("__builtins__", &builtins)?;
                
                // Step 3: Execute the compiled bytecode in the module's namespace
                exec_fn.call1((&code_object, &globals, &globals))
                    .map_err(|e| {
                        tracing::error!("Failed to execute bytecode for '{}': {}", module_name, e);
                        e
                    })?;
                
                // Register the module in sys.modules with namespaced name
                // This avoids overwriting the main package module
                let sys = py.import("sys")?;
                let sys_modules = sys.getattr("modules")?;
                sys_modules.set_item(&namespaced_module, &module)?;
                
                tracing::debug!("Successfully loaded embedded Python node: {} -> {}", module_name, namespaced_module);
            }
            
            tracing::info!("All {} embedded Python bytecode modules loaded successfully", EMBEDDED_PYTHON_BYTECODE.len());
            Ok(())
        })();
        
        // Safety: This is only written once during INIT.call_once
        unsafe { INIT_RESULT = Some(result); }
    });
    
    // Safety: After call_once completes, INIT_RESULT is set and never modified
    unsafe {
        match &INIT_RESULT {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to initialize embedded nodes: {}", e)
            )),
            None => unreachable!("INIT_RESULT should always be set after call_once"),
        }
    }
}"##.to_string()
}

/// Generate the streaming session class implementation using remotemedia-ffi
fn generate_streaming_session(class_name: &str) -> String {
    format!(
        r##"
use remotemedia_core::{{
    data::RuntimeData,
    manifest::Manifest,
    transport::{{PipelineExecutor, TransportData}},
}};

/// {class_name} processing session
///
/// Provides a streaming interface for processing data through the embedded pipeline.
/// Use `send()` to feed data and `recv()` to get results.
#[pyclass]
pub struct {class_name}Session {{
    executor: Arc<Mutex<Option<PipelineExecutor>>>,
    manifest: Arc<Manifest>,
    pending_outputs: Arc<Mutex<Vec<RuntimeData>>>,
}}

#[pymethods]
impl {class_name}Session {{
    /// Create a new processing session
    #[new]
    fn new() -> PyResult<Self> {{
        // Parse embedded manifest
        let manifest: Manifest = serde_yaml::from_str(PIPELINE_YAML)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to parse embedded pipeline: {{}}", e)
            ))?;
        
        // Create executor
        let executor = PipelineExecutor::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to create executor: {{}}", e)
            ))?;
        
        Ok(Self {{
            executor: Arc::new(Mutex::new(Some(executor))),
            manifest: Arc::new(manifest),
            pending_outputs: Arc::new(Mutex::new(Vec::new())),
        }})
    }}

    /// Send data for processing
    ///
    /// # Arguments
    /// * `data` - Input data (numpy array, dict, string, bytes, etc.)
    ///
    /// # Returns
    /// Awaitable that resolves to the processing result
    fn send<'py>(
        &self,
        py: Python<'py>,
        data: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {{
        // Convert Python data to RuntimeData
        let runtime_data = python_to_runtime_data(py, &data)?;
        
        let executor_arc = self.executor.clone();
        let manifest_arc = self.manifest.clone();
        let outputs_arc = self.pending_outputs.clone();
        
        future_into_py(py, async move {{
            let executor_guard = executor_arc.lock().await;
            let executor = executor_guard.as_ref()
                .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Session has been closed"
                ))?;
            
            // Execute pipeline
            let input = TransportData::new(runtime_data);
            let output = executor
                .execute_unary(manifest_arc, input)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("Pipeline execution failed: {{}}", e)
                ))?;
            
            // Store output for recv()
            outputs_arc.lock().await.push(output.data.clone());
            
            // Convert output to Python
            Python::attach(|py| {{
                runtime_data_to_python(py, &output.data)
            }})
        }})
    }}

    /// Receive processed output
    ///
    /// # Returns
    /// The next pending output, or None if no output available
    fn recv<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {{
        let outputs_arc = self.pending_outputs.clone();
        
        future_into_py(py, async move {{
            let mut outputs = outputs_arc.lock().await;
            if let Some(data) = outputs.pop() {{
                Python::attach(|py| {{
                    runtime_data_to_python(py, &data)
                }})
            }} else {{
                Ok(Python::attach(|py| py.None().into_any().into()))
            }}
        }})
    }}

    /// Check if session has pending output
    fn has_output<'py>(&self, py: Python<'py>) -> PyResult<bool> {{
        // Synchronous check - need to block briefly
        let outputs_arc = self.pending_outputs.clone();
        let rt = tokio::runtime::Handle::try_current()
            .unwrap_or_else(|_| {{
                tokio::runtime::Runtime::new().unwrap().handle().clone()
            }});
        
        Ok(rt.block_on(async {{
            !outputs_arc.lock().await.is_empty()
        }}))
    }}

    /// Close the session and release resources
    fn close(&self) -> PyResult<()> {{
        let executor_arc = self.executor.clone();
        let rt = tokio::runtime::Handle::try_current()
            .unwrap_or_else(|_| {{
                tokio::runtime::Runtime::new().unwrap().handle().clone()
            }});
        
        rt.block_on(async {{
            let mut guard = executor_arc.lock().await;
            *guard = None;
        }});
        
        Ok(())
    }}

    /// Get session info as dictionary
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {{
        let dict = PyDict::new(py);
        dict.set_item("pipeline_yaml", PIPELINE_YAML)?;
        dict.set_item("version", env!("CARGO_PKG_VERSION"))?;
        Ok(dict)
    }}
}}

/// Process data through the pipeline (one-shot)
///
/// # Arguments
/// * `data` - Input data to process
///
/// # Returns
/// Awaitable that resolves to the processing result
#[pyfunction]
fn process<'py>(
    py: Python<'py>,
    data: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {{
    // Convert input
    let runtime_data = python_to_runtime_data(py, &data)?;
    
    future_into_py(py, async move {{
        // Parse manifest
        let manifest: Manifest = serde_yaml::from_str(PIPELINE_YAML)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Failed to parse pipeline: {{}}", e)
            ))?;
        let manifest = Arc::new(manifest);
        
        // Create executor
        let executor = PipelineExecutor::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Failed to create executor: {{}}", e)
            ))?;
        
        // Execute
        let input = TransportData::new(runtime_data);
        let output = executor
            .execute_unary(manifest, input)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("Execution failed: {{}}", e)
            ))?;
        
        // Convert output
        Python::attach(|py| {{
            runtime_data_to_python(py, &output.data)
        }})
    }})
}}
"##,
        class_name = class_name,
    )
}

/// Generate batch (non-streaming) API 
fn generate_batch_api(class_name: &str) -> String {
    // For non-streaming pipelines, we still provide the session API but also
    // emphasize the simple process() function
    generate_streaming_session(class_name)
}

/// Generate Python __init__.py with re-exports
pub fn generate_init_py(package_name: &str, class_name: &str, is_streaming: bool) -> String {
    format!(
        r#""""{package_name} - RemoteMedia Pipeline Package

Auto-generated pipeline bindings using remotemedia-ffi.
"""

from .{package_name} import (
    {class_name}Session,
    process,
    get_pipeline_yaml,
    get_version,
    __version__,
    PIPELINE_YAML,
)

__all__ = [
    "{class_name}Session",
    "process",
    "get_pipeline_yaml",
    "get_version",
    "__version__",
    "PIPELINE_YAML",
]
"#,
        package_name = package_name,
        class_name = class_name,
    )
}

/// Generate README.md for the package
pub fn generate_readme(
    package_name: &str,
    class_name: &str,
    description: &str,
    is_streaming: bool,
) -> String {
    format!(
        r#"# {package_name}

{description}

## Installation

```bash
pip install {package_name}
```

Or build from source:

```bash
maturin build --release
pip install target/wheels/*.whl
```

## Usage

### Session-based (for streaming)

```python
import asyncio
from {package_name} import {class_name}Session

async def main():
    session = {class_name}Session()
    
    # Send data for processing
    result = await session.send(audio_data)
    print(result)
    
    # Or use recv() pattern
    await session.send(audio_data)
    while session.has_output():
        output = await session.recv()
        print(output)
    
    session.close()

asyncio.run(main())
```

### One-shot processing

```python
import asyncio
from {package_name} import process

async def main():
    result = await process(input_data)
    print(result)

asyncio.run(main())
```

## API Reference

### {class_name}Session

Session class for streaming processing.

- `__init__()` - Create a new session
- `send(data)` - Send data and get result (async)
- `recv()` - Receive pending output (async)
- `has_output()` - Check if output is available
- `close()` - Release resources
- `info()` - Get session information

### Functions

- `process(data)` - One-shot processing (async)
- `get_pipeline_yaml()` - Get the embedded pipeline YAML
- `get_version()` - Get package version

### Constants

- `__version__` - Package version string
- `PIPELINE_YAML` - Embedded pipeline YAML content

## License

MIT
"#,
        package_name = package_name,
        description = description,
        class_name = class_name,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_cargo_toml() {
        let toml = generate_cargo_toml("test_pipeline", "0.1.0", "/path/to/workspace");
        assert!(toml.contains("name = \"test_pipeline\""));
        assert!(toml.contains("remotemedia-ffi"));
    }

    #[test]
    fn test_generate_pyproject() {
        let toml = generate_pyproject_toml(
            "test_pipeline",
            "0.1.0",
            "Test description",
            ">=3.10",
            &[],
        );
        assert!(toml.contains("name = \"test_pipeline\""));
        assert!(toml.contains("maturin"));
    }

    #[test]
    fn test_generate_lib_rs() {
        let lib_rs = generate_lib_rs(
            "test_pipeline",
            "TestPipeline",
            "Test pipeline",
            true,
        );
        assert!(lib_rs.contains("TestPipelineSession"));
        assert!(lib_rs.contains("remotemedia_ffi"));
        assert!(lib_rs.contains("PipelineExecutor"));
    }
}
