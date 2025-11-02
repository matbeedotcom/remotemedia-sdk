//! Async generator and event loop handling for CPython nodes
//!
//! This module encapsulates all asyncio/event loop complexity.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::Value;

use crate::{Error, Result};

/// Event loop manager for async Python operations
pub struct EventLoopManager {
    event_loop: Option<Py<PyAny>>,
}

impl EventLoopManager {
    pub fn new() -> Self {
        Self { event_loop: None }
    }

    /// Create or get the event loop
    pub fn get_or_create<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if let Some(loop_py) = &self.event_loop {
            return Ok(loop_py.bind(py).clone());
        }

        let asyncio = py.import("asyncio")?;

        #[cfg(target_os = "windows")]
        let event_loop = {
            tracing::debug!("Creating SelectorEventLoop for Windows");
            let code = std::ffi::CString::new(
                "import asyncio; _loop = asyncio.SelectorEventLoop(); asyncio.set_event_loop(_loop)"
            ).unwrap();
            py.run(&code, None, None)?;
            let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
            locals.get_item("_loop")?
        };

        #[cfg(not(target_os = "windows"))]
        let event_loop = {
            let loop_obj = asyncio.call_method0("new_event_loop")?;
            asyncio.call_method1("set_event_loop", (&loop_obj,))?;
            loop_obj
        };

        self.event_loop = Some(event_loop.clone().unbind());
        Ok(event_loop)
    }

    /// Run a coroutine to completion
    pub fn run_until_complete<'py>(
        &mut self,
        py: Python<'py>,
        coro: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let event_loop = self.get_or_create(py)?;
        event_loop.call_method1("run_until_complete", (coro,))
    }
}

/// Async generator iterator
pub struct AsyncGenerator {
    generator: Py<PyAny>,
}

impl AsyncGenerator {
    pub fn new(generator: Py<PyAny>) -> Self {
        Self { generator }
    }

    /// Check if a Python object is an async generator
    pub fn is_async_generator(py: Python, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let inspect = py.import("inspect")?;
        inspect
            .call_method1("isasyncgen", (obj,))?
            .extract::<bool>()
    }

    /// Check if a Python object is a coroutine
    pub fn is_coroutine(py: Python, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        let inspect = py.import("inspect")?;
        inspect
            .call_method1("iscoroutine", (obj,))?
            .extract::<bool>()
    }

    /// Get the next item from the async generator
    /// Returns None if exhausted
    pub fn get_next<'py>(
        &self,
        py: Python<'py>,
        event_loop: &Bound<'py, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        // Define helper function to get next item
        let code = std::ffi::CString::new(
            r#"
async def _get_next(agen):
    try:
        return (True, await anext(agen))
    except StopAsyncIteration:
        return (False, None)
"#,
        )
        .unwrap();

        py.run(&code, None, None)?;

        let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
        let get_next_fn = locals.get_item("_get_next")?;

        let gen = self.generator.bind(py);
        let coro = get_next_fn.call1((gen,))?;

        // Run the coroutine
        let result = event_loop.call_method1("run_until_complete", (coro,))?;

        // Extract (has_value, value) tuple
        let has_value: bool = result.get_item(0)?.extract()?;

        if !has_value {
            return Ok(None);
        }

        let item = result.get_item(1)?;
        Ok(Some(item))
    }

    /// Collect all items from the async generator
    pub fn collect_all<'py>(
        &self,
        py: Python<'py>,
        event_loop: &Bound<'py, PyAny>,
    ) -> PyResult<Vec<Bound<'py, PyAny>>> {
        let code = std::ffi::CString::new(
            r#"
async def _collect_all(agen):
    results = []
    async for item in agen:
        results.append(item)
    return results
"#,
        )
        .unwrap();

        py.run(&code, None, None)?;

        let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
        let collect_fn = locals.get_item("_collect_all")?;

        let gen = self.generator.bind(py);
        let coro = collect_fn.call1((gen,))?;

        let result = event_loop.call_method1("run_until_complete", (coro,))?;

        // Convert to Vec
        let mut items = Vec::new();
        if let Ok(py_list) = result.downcast::<pyo3::types::PyList>() {
            for item in py_list.iter() {
                items.push(item);
            }
        }

        Ok(items)
    }
}

/// Initialize async method with thread isolation (PyTorch workaround)
pub fn call_initialize_async(py: Python, instance: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    tracing::info!("Calling async initialize() with thread isolation");

    let result = instance.call_method0("initialize")?;

    // Check if it's a coroutine
    if !AsyncGenerator::is_coroutine(py, &result)? {
        return Ok(result.unbind());
    }

    // Run in isolated thread to avoid PyTorch heap corruption on Windows
    let code = std::ffi::CString::new(
        r#"
import asyncio

def _run_init_in_thread(coro):
    """Run async initialization in isolated thread."""
    new_loop = asyncio.new_event_loop()
    asyncio.set_event_loop(new_loop)
    try:
        return new_loop.run_until_complete(coro)
    finally:
        new_loop.close()

async def _run_with_thread_isolation(run_fn, coro):
    return await asyncio.to_thread(run_fn, coro)
"#,
    )
    .unwrap();

    py.run(&code, None, None)?;

    let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
    let run_init_fn = locals.get_item("_run_init_in_thread")?;
    let thread_wrapper_fn = locals.get_item("_run_with_thread_isolation")?;

    let wrapped_coro = thread_wrapper_fn.call1((run_init_fn, result))?;

    // Create event loop for the wrapper
    let asyncio = py.import("asyncio")?;

    #[cfg(target_os = "windows")]
    let new_loop = {
        py.run(
            &std::ffi::CString::new("import asyncio; selector_loop = asyncio.SelectorEventLoop()")
                .unwrap(),
            None,
            None,
        )?;
        let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
        locals.get_item("selector_loop")?
    };

    #[cfg(not(target_os = "windows"))]
    let new_loop = asyncio.call_method0("new_event_loop")?;

    asyncio.call_method1("set_event_loop", (&new_loop,))?;

    // Run the wrapper coroutine
    new_loop.call_method1("run_until_complete", (wrapped_coro,))?;

    tracing::info!("Async initialize completed successfully");

    // Return the event loop for reuse
    Ok(new_loop.unbind())
}
