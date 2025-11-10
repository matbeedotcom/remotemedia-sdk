//! Streaming queue management for CPython nodes
//!
//! Provides a persistent queue for feeding data into streaming Python nodes.

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Streaming queue for feeding data to Python async generators
pub struct StreamingQueue {
    queue: Py<PyAny>,
    finished: bool,
}

impl StreamingQueue {
    /// Create a new streaming queue
    pub fn new(py: Python) -> PyResult<Self> {
        let code = std::ffi::CString::new(
            r#"
import asyncio

class StreamingQueue:
    """Queue for feeding data into streaming nodes."""
    def __init__(self):
        self.items = []
        self.index = 0
        self.finished = False

    def put(self, item):
        """Add an item to the queue."""
        self.items.append(item)

    def finish(self):
        """Signal that no more items will be added."""
        self.finished = True

    async def stream(self):
        """Async generator that yields items from the queue."""
        while not self.finished or self.index < len(self.items):
            while self.index < len(self.items):
                item = self.items[self.index]
                self.index += 1
                yield item

            if not self.finished:
                await asyncio.sleep(0)

_StreamingQueue = StreamingQueue
"#,
        )
        .unwrap();

        py.run(&code, None, None)?;

        let locals = py.eval(&std::ffi::CString::new("locals()").unwrap(), None, None)?;
        let queue_class = locals.get_item("_StreamingQueue")?;

        let queue_instance = queue_class.call0()?;

        Ok(Self {
            queue: queue_instance.unbind(),
            finished: false,
        })
    }

    /// Get the queue's stream() async generator
    pub fn get_stream<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let queue = self.queue.bind(py);
        queue.call_method0("stream")
    }

    /// Add an item to the queue
    pub fn put(&self, py: Python, item: &Bound<'_, PyAny>) -> PyResult<()> {
        let queue = self.queue.bind(py);
        let locals = PyDict::new(py);
        locals.set_item("queue_ref", queue)?;
        locals.set_item("item_ref", item)?;

        let code = std::ffi::CString::new("queue_ref.put(item_ref)").unwrap();
        py.run(&code, None, Some(&locals))?;

        Ok(())
    }

    /// Signal that no more items will be added
    pub fn finish(&mut self, py: Python) -> PyResult<()> {
        if self.finished {
            return Ok(());
        }

        let queue = self.queue.bind(py);
        queue.call_method0("finish")?;
        self.finished = true;

        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }
}
