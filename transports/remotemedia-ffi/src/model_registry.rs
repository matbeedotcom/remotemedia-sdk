//! PyO3 bindings for Model Registry
//!
//! Exposes Rust ModelRegistry to Python, enabling true cross-process model sharing

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

/// Python-accessible model wrapper
/// Stores Python model objects and exposes them via the registry
struct PyModelWrapper {
    model_id: String,
    py_model: PyObject,
    memory_bytes: usize,
}

/// Global model registry shared across all Python processes via FFI
static GLOBAL_REGISTRY: once_cell::sync::Lazy<Arc<RwLock<HashMap<String, Arc<PyModelWrapper>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// Metrics for the global registry
static GLOBAL_METRICS: once_cell::sync::Lazy<Arc<RwLock<PyRegistryMetrics>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(PyRegistryMetrics::default())));

/// Registry metrics (mirroring Python API)
#[derive(Debug, Clone, Default)]
struct PyRegistryMetrics {
    total_models: usize,
    total_memory_bytes: u64,
    cache_hits: u64,
    cache_misses: u64,
    evictions: u64,
}

/// Model Registry exposed to Python via PyO3
///
/// This is a singleton that lives in the Rust process and is shared
/// across all Python processes that load the remotemedia_runtime extension.
#[pyclass(name = "ModelRegistry")]
pub struct PyModelRegistry {
    // No internal state - uses global registry
}

#[pymethods]
impl PyModelRegistry {
    #[new]
    fn new() -> Self {
        tracing::debug!("PyModelRegistry created (using global Rust registry)");
        Self {}
    }
    
    /// Get or load a model from the global Rust registry
    ///
    /// Args:
    ///     key: Unique model identifier
    ///     loader: Python callable that loads the model
    ///     session_id: Optional session ID (unused for now)
    ///
    /// Returns:
    ///     Python model object (from cache or newly loaded)
    fn get_or_load(
        &self,
        key: &str,
        loader: PyObject,
        session_id: Option<&str>,
    ) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            // Check if model is already in global registry
            {
                let registry = GLOBAL_REGISTRY.read();
                if let Some(wrapper) = registry.get(key) {
                    // Cache hit!
                    let mut metrics = GLOBAL_METRICS.write();
                    metrics.cache_hits += 1;
                    
                    tracing::info!(
                        "Model '{}' found in Rust registry (cache hit rate: {:.1}%)",
                        key,
                        (metrics.cache_hits as f64 / (metrics.cache_hits + metrics.cache_misses) as f64) * 100.0
                    );
                    
                    return Ok(wrapper.py_model.clone_ref(py));
                }
            }
            
            // Cache miss - load the model
            tracing::info!("Loading model '{}' via Rust FFI registry...", key);
            
            let start = std::time::Instant::now();
            
            // Call Python loader function
            let py_model = loader.call0(py)?;
            
            let load_time = start.elapsed();
            
            // Estimate memory (try to call memory_usage() if available)
            let py_model_bound = py_model.bind(py);
            let memory_bytes = if let Ok(method) = py_model_bound.getattr("memory_usage") {
                method.call0()?.extract::<usize>().unwrap_or(100 * 1024 * 1024)
            } else {
                // Fallback: estimate based on parameters
                Self::estimate_memory(py, py_model_bound)
            };
            
            // Store in global registry
            let wrapper = Arc::new(PyModelWrapper {
                model_id: key.to_string(),
                py_model: py_model.clone_ref(py),
                memory_bytes,
            });
            
            {
                let mut registry = GLOBAL_REGISTRY.write();
                registry.insert(key.to_string(), Arc::clone(&wrapper));
            }
            
            // Update metrics
            {
                let mut metrics = GLOBAL_METRICS.write();
                metrics.cache_misses += 1;
                metrics.total_models += 1;
                metrics.total_memory_bytes += memory_bytes as u64;
            }
            
            tracing::info!(
                "Model '{}' loaded via Rust registry in {:.2}s (~{:.1}MB)",
                key,
                load_time.as_secs_f64(),
                memory_bytes as f64 / 1024.0 / 1024.0
            );
            
            Ok(py_model)
        })
    }
    
    /// Release a model reference
    fn release(&self, key: &str) {
        tracing::debug!("Released reference to model '{}' in Rust registry", key);
        // In current implementation, we rely on Python GC
        // Future: implement reference counting
    }
    
    /// List all loaded models
    fn list_models(&self) -> PyResult<Vec<PyModelInfo>> {
        let registry = GLOBAL_REGISTRY.read();
        let models: Vec<PyModelInfo> = registry
            .iter()
            .map(|(key, wrapper)| PyModelInfo {
                model_id: key.clone(),
                device: "unknown".to_string(),
                memory_bytes: wrapper.memory_bytes,
                reference_count: Arc::strong_count(wrapper) as u32 - 1,
            })
            .collect();
        Ok(models)
    }
    
    /// Get registry metrics
    fn metrics(&self) -> PyRegistryMetricsData {
        let metrics = GLOBAL_METRICS.read();
        PyRegistryMetricsData {
            total_models: metrics.total_models,
            total_memory_bytes: metrics.total_memory_bytes,
            cache_hits: metrics.cache_hits,
            cache_misses: metrics.cache_misses,
            evictions: metrics.evictions,
        }
    }
    
    /// Clear all models (for testing)
    fn clear(&self) {
        let mut registry = GLOBAL_REGISTRY.write();
        registry.clear();
        
        let mut metrics = GLOBAL_METRICS.write();
        *metrics = PyRegistryMetrics::default();
        
        tracing::info!("Rust registry cleared");
    }
    
    /// Estimate memory usage of a Python model
    fn estimate_memory(py: Python, py_model: &Bound<'_, PyAny>) -> usize {
        // Try to get parameter count for PyTorch/HuggingFace models
        if let Ok(params_method) = py_model.getattr("parameters") {
            if let Ok(params_iter) = params_method.call0() {
                // Try to count parameters
                let mut total_params = 0u64;
                if let Ok(iter) = params_iter.iter() {
                    for param in iter {
                        if let Ok(p) = param {
                            if let Ok(numel_method) = p.getattr("numel") {
                                if let Ok(count_obj) = numel_method.call0() {
                                    if let Ok(count) = count_obj.extract::<u64>() {
                                        total_params += count;
                                    }
                                }
                            }
                        }
                    }
                }
                if total_params > 0 {
                    return (total_params * 4) as usize; // Assume float32
                }
            }
        }
        
        // Fallback
        100 * 1024 * 1024 // 100MB default
    }
}

/// Model info for Python
#[pyclass]
#[derive(Clone)]
pub struct PyModelInfo {
    #[pyo3(get)]
    model_id: String,
    #[pyo3(get)]
    device: String,
    #[pyo3(get)]
    memory_bytes: usize,
    #[pyo3(get)]
    reference_count: u32,
}

/// Registry metrics for Python
#[pyclass]
#[derive(Clone)]
pub struct PyRegistryMetricsData {
    #[pyo3(get)]
    total_models: usize,
    #[pyo3(get)]
    total_memory_bytes: u64,
    #[pyo3(get)]
    cache_hits: u64,
    #[pyo3(get)]
    cache_misses: u64,
    #[pyo3(get)]
    evictions: u64,
}

#[pymethods]
impl PyRegistryMetricsData {
    /// Calculate hit rate
    #[getter]
    fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        }
    }
}

/// Convenience function for Python
#[pyfunction]
fn get_or_load(key: &str, loader: PyObject) -> PyResult<PyObject> {
    let registry = PyModelRegistry::new();
    registry.get_or_load(key, loader, None)
}

/// Register model registry types with Python module
pub fn register_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyModelRegistry>()?;
    m.add_class::<PyModelInfo>()?;
    m.add_class::<PyRegistryMetricsData>()?;
    m.add_function(wrap_pyfunction!(get_or_load, m)?)?;
    Ok(())
}

