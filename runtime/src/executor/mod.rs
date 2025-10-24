//! Pipeline execution engine
//!
//! This module implements the core pipeline executor that:
//! - Builds pipeline graphs from manifests
//! - Performs topological sorting for execution order
//! - Manages async execution with tokio
//! - Handles node lifecycle (init, process, cleanup)
//! - Runtime selection for Python nodes (Phase 1.10)

pub mod runtime_selector;

use crate::{Error, Result};
use crate::manifest::Manifest;
use crate::nodes::{NodeContext, NodeExecutor, NodeRegistry};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use serde_json::Value;
use pyo3::prelude::*;
use pyo3::types::PyAny;

pub use runtime_selector::{RuntimeSelector, SelectedRuntime};

/// Cache for Python objects to avoid unnecessary serialization
/// when passing data between Python nodes
#[derive(Clone)]
pub struct PyObjectCache {
    objects: Arc<Mutex<HashMap<String, Py<PyAny>>>>,
    next_id: Arc<Mutex<u64>>,
}

impl PyObjectCache {
    pub fn new() -> Self {
        Self {
            objects: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Store a Python object and return its cache ID
    pub fn store(&self, obj: Py<PyAny>) -> String {
        let mut next_id = self.next_id.lock().unwrap();
        let id = format!("pyobj_{}", *next_id);
        *next_id += 1;

        let mut objects = self.objects.lock().unwrap();
        objects.insert(id.clone(), obj);

        tracing::info!("Stored Python object in cache with ID: {}", id);
        id
    }

    /// Retrieve a Python object by ID
    pub fn get(&self, id: &str) -> Option<Py<PyAny>> {
        let objects = self.objects.lock().unwrap();
        Python::with_gil(|py| {
            objects.get(id).map(|obj| obj.clone_ref(py))
        })
    }

    /// Check if an ID exists in the cache
    pub fn contains(&self, id: &str) -> bool {
        let objects = self.objects.lock().unwrap();
        objects.contains_key(id)
    }

    /// Clear all cached objects
    pub fn clear(&self) {
        let mut objects = self.objects.lock().unwrap();
        objects.clear();
        tracing::info!("Cleared Python object cache");
    }
}

/// Represents a node in the pipeline graph
#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Node ID (unique identifier)
    pub id: String,

    /// Node type (class name)
    pub node_type: String,

    /// Node parameters
    pub params: Value,

    /// Whether this is a streaming node (async generator)
    pub is_streaming: bool,

    /// Optional capability requirements
    pub capabilities: Option<crate::manifest::CapabilityRequirements>,

    /// Optional remote host
    pub host: Option<String>,

    /// Input connections (node IDs that feed into this node)
    pub inputs: Vec<String>,

    /// Output connections (node IDs this node feeds into)
    pub outputs: Vec<String>,
}

/// Pipeline execution graph
#[derive(Debug)]
pub struct PipelineGraph {
    /// All nodes indexed by ID
    pub nodes: HashMap<String, GraphNode>,

    /// Execution order (topologically sorted node IDs)
    pub execution_order: Vec<String>,

    /// Source nodes (nodes with no inputs)
    pub sources: Vec<String>,

    /// Sink nodes (nodes with no outputs)
    pub sinks: Vec<String>,
}

impl PipelineGraph {
    /// Build a pipeline graph from a manifest
    pub fn from_manifest(manifest: &Manifest) -> Result<Self> {
        let mut nodes = HashMap::new();

        // First pass: Create all nodes
        for node_manifest in &manifest.nodes {
            let graph_node = GraphNode {
                id: node_manifest.id.clone(),
                node_type: node_manifest.node_type.clone(),
                params: node_manifest.params.clone(),
                is_streaming: node_manifest.is_streaming,
                capabilities: node_manifest.capabilities.clone(),
                host: node_manifest.host.clone(),
                inputs: Vec::new(),
                outputs: Vec::new(),
            };
            nodes.insert(node_manifest.id.clone(), graph_node);
        }

        // Second pass: Build connections
        for connection in &manifest.connections {
            // Add output connection to source node
            if let Some(from_node) = nodes.get_mut(&connection.from) {
                from_node.outputs.push(connection.to.clone());
            } else {
                return Err(Error::Manifest(format!(
                    "Connection references unknown source node: {}",
                    connection.from
                )));
            }

            // Add input connection to target node
            if let Some(to_node) = nodes.get_mut(&connection.to) {
                to_node.inputs.push(connection.from.clone());
            } else {
                return Err(Error::Manifest(format!(
                    "Connection references unknown target node: {}",
                    connection.to
                )));
            }
        }

        // Identify sources and sinks
        let mut sources = Vec::new();
        let mut sinks = Vec::new();

        for (id, node) in &nodes {
            if node.inputs.is_empty() {
                sources.push(id.clone());
            }
            if node.outputs.is_empty() {
                sinks.push(id.clone());
            }
        }

        // Perform topological sort to get execution order
        let execution_order = Self::topological_sort(&nodes)?;

        Ok(Self {
            nodes,
            execution_order,
            sources,
            sinks,
        })
    }

    /// Perform topological sort using Kahn's algorithm
    fn topological_sort(nodes: &HashMap<String, GraphNode>) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut result = Vec::new();

        // Calculate in-degree for each node
        for (id, _) in nodes {
            in_degree.insert(id.clone(), 0);
        }

        for (_, node) in nodes {
            for output_id in &node.outputs {
                *in_degree.get_mut(output_id).unwrap() += 1;
            }
        }

        // Queue all nodes with in-degree 0
        let mut queue: VecDeque<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(id, _)| id.clone())
            .collect();

        // Process nodes in topological order
        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());

            // Get the node
            if let Some(node) = nodes.get(&node_id) {
                // Reduce in-degree of output nodes
                for output_id in &node.outputs {
                    if let Some(degree) = in_degree.get_mut(output_id) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(output_id.clone());
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != nodes.len() {
            return Err(Error::Manifest(
                "Pipeline contains a cycle - cannot execute".to_string(),
            ));
        }

        Ok(result)
    }

    /// Get a node by ID
    pub fn get_node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    /// Get the number of nodes in the graph
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

/// Pipeline executor
pub struct Executor {
    /// Execution configuration
    config: ExecutorConfig,

    /// Node registry
    registry: NodeRegistry,

    /// Runtime selector for Python nodes (Phase 1.10.6)
    runtime_selector: RuntimeSelector,

    /// Cache for Python objects to avoid serialization between Python nodes
    py_cache: PyObjectCache,
}

/// Executor configuration
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum concurrent node executions
    pub max_concurrency: usize,

    /// Enable debug logging
    pub debug: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 10,
            debug: false,
        }
    }
}

impl Executor {
    /// Create a new executor with default configuration
    pub fn new() -> Self {
        Self::with_config(ExecutorConfig::default())
    }

    /// Create a new executor with custom configuration
    pub fn with_config(config: ExecutorConfig) -> Self {
        Self {
            config,
            registry: NodeRegistry::default(),
            runtime_selector: RuntimeSelector::new(),
            py_cache: PyObjectCache::new(),
        }
    }

    /// Create executor with custom registry
    pub fn with_registry(config: ExecutorConfig, registry: NodeRegistry) -> Self {
        Self {
            config,
            registry,
            runtime_selector: RuntimeSelector::new(),
            py_cache: PyObjectCache::new(),
        }
    }

    /// Get a reference to the Python object cache
    pub fn py_cache(&self) -> &PyObjectCache {
        &self.py_cache
    }

    /// Execute a pipeline from a manifest
    pub async fn execute(&self, manifest: &Manifest) -> Result<ExecutionResult> {
        tracing::info!("Executing pipeline: {}", manifest.metadata.name);

        // Step 1: Build pipeline graph
        let graph = PipelineGraph::from_manifest(manifest)?;
        tracing::info!(
            "Built pipeline graph with {} nodes, execution order: {:?}",
            graph.node_count(),
            graph.execution_order
        );

        // Step 2: Validate manifest
        crate::manifest::validate(manifest)?;

        // Step 3: Execute nodes in topological order
        tracing::info!("Pipeline graph built successfully");
        tracing::info!("Sources: {:?}", graph.sources);
        tracing::info!("Sinks: {:?}", graph.sinks);

        // Create node instances
        let mut node_instances: HashMap<String, Box<dyn NodeExecutor>> = HashMap::new();
        for node_manifest in &manifest.nodes {
            let node = self.create_node_with_runtime(node_manifest)?;
            node_instances.insert(node_manifest.id.clone(), node);
        }

        // Initialize all nodes
        for node_manifest in &manifest.nodes {
            let context = NodeContext {
                node_id: node_manifest.id.clone(),
                node_type: node_manifest.node_type.clone(),
                params: node_manifest.params.clone(),
                session_id: None,
                metadata: HashMap::new(),
            };

            if let Some(node) = node_instances.get_mut(&node_manifest.id) {
                tracing::info!("Initializing node: {}", node_manifest.id);
                node.initialize(&context).await?;
            }
        }

        // Execute pipeline: For source-based pipelines, start with Null input to source nodes
        tracing::info!("Nodes initialized successfully");

        // Check if this is a linear pipeline
        let is_linear = self.is_linear_pipeline(&graph);

        // Check if pipeline has streaming nodes - if so, use concurrent execution
        let has_streaming_nodes = node_instances.values().any(|n| n.is_streaming());

        let outputs = if has_streaming_nodes {
            tracing::info!("Pipeline has streaming nodes, using concurrent channel-based execution");
            // Concurrent executor takes ownership and handles cleanup internally
            return self.execute_concurrent_pipeline(&graph, node_instances).await;
        } else if is_linear {
            // Use linear execution (simpler, optimized)
            // Start with source nodes by giving them Null input
            tracing::info!("Using linear execution for source-based pipeline");
            self.execute_linear_source_pipeline(&graph, &mut node_instances).await?
        } else {
            // Use DAG execution for complex topologies
            tracing::info!("Using DAG execution for source-based pipeline");
            self.execute_dag_source_pipeline(&graph, &mut node_instances).await?
        };

        tracing::info!("Pipeline execution completed");

        // Cleanup all nodes (only for non-concurrent execution)
        for (id, node) in node_instances.iter_mut() {
            tracing::info!("Cleaning up node: {}", id);
            node.cleanup().await?;
        }

        Ok(ExecutionResult {
            status: "success".to_string(),
            outputs,
            graph_info: Some(GraphInfo {
                node_count: graph.node_count(),
                source_count: graph.sources.len(),
                sink_count: graph.sinks.len(),
                execution_order: graph.execution_order.clone(),
            }),
        })
    }

    /// Create a node instance based on runtime selection (Phase 1.10.7)
    fn create_node_with_runtime(
        &self,
        node_manifest: &crate::manifest::NodeManifest,
    ) -> Result<Box<dyn NodeExecutor>> {
        // First try to create from registry (for Rust-native nodes)
        if self.registry.has_node_type(&node_manifest.node_type) {
            tracing::info!(
                "Creating node {} from registry (Rust-native)",
                node_manifest.id
            );
            return self.registry.create(&node_manifest.node_type);
        }

        // Not in registry, assume it's a Python node - select runtime
        let runtime = self.runtime_selector.select_runtime(node_manifest);
        tracing::info!(
            "Node {} (type: {}) will execute on: {:?}",
            node_manifest.id,
            node_manifest.node_type,
            runtime
        );

        match runtime {
            SelectedRuntime::RustPython => {
                // TODO: Create RustPython executor when available
                // For now, fall back to CPython
                tracing::warn!(
                    "RustPython executor selected but not yet integrated, using CPython for node {}",
                    node_manifest.id
                );
                let mut executor = crate::python::CPythonNodeExecutor::new_with_cache(
                    &node_manifest.node_type,
                    self.py_cache.clone(),
                );
                executor.set_is_streaming_node(node_manifest.is_streaming);
                Ok(Box::new(executor))
            }
            SelectedRuntime::CPython => {
                tracing::info!(
                    "Creating CPython executor for node {} (type: {})",
                    node_manifest.id,
                    node_manifest.node_type
                );
                let mut executor = crate::python::CPythonNodeExecutor::new_with_cache(
                    &node_manifest.node_type,
                    self.py_cache.clone(),
                );
                executor.set_is_streaming_node(node_manifest.is_streaming);
                Ok(Box::new(executor))
            }
            SelectedRuntime::CPythonWasm => {
                // Phase 3 - not implemented yet
                Err(Error::Execution(format!(
                    "CPython WASM runtime not yet implemented (Phase 3) for node {}",
                    node_manifest.id
                )))
            }
        }
    }

    /// Execute pipeline with input data
    ///
    /// Phase 1.11: Enhanced data flow orchestration with:
    /// - Sequential data passing between connected nodes (1.11.1)
    /// - Support for streaming/async generators (1.11.2)
    /// - Backpressure handling (1.11.3)
    /// - Branching and merging support (1.11.4)
    pub async fn execute_with_input(
        &self,
        manifest: &Manifest,
        input_data: Vec<Value>,
    ) -> Result<ExecutionResult> {
        tracing::info!("Executing pipeline: {} with {} inputs", manifest.metadata.name, input_data.len());

        // Build graph
        let graph = PipelineGraph::from_manifest(manifest)?;
        crate::manifest::validate(manifest)?;

        // Phase 1.11.1: Check if this is a simple linear pipeline or complex DAG
        let is_linear = self.is_linear_pipeline(&graph);

        if is_linear {
            // Simple linear execution path (optimized)
            tracing::info!("Using linear execution strategy");
            self.execute_linear_pipeline(&graph, manifest, input_data).await
        } else {
            // Complex DAG with branching/merging (Phase 1.11.4)
            tracing::info!("Using DAG execution strategy with branching/merging");
            self.execute_dag_pipeline(&graph, manifest, input_data).await
        }
    }

    /// Execute a linear source-based pipeline (source nodes generate data)
    /// TRUE STREAMING: Pulls items one at a time from source and flows through pipeline
    async fn execute_linear_source_pipeline(
        &self,
        graph: &PipelineGraph,
        node_instances: &mut HashMap<String, Box<dyn NodeExecutor>>,
    ) -> Result<Value> {
        if graph.execution_order.is_empty() {
            return Ok(Value::Null);
        }

        let mut final_results = Vec::new();

        // Initialize the source node (first node)
        let first_node_id = &graph.execution_order[0];

        loop {
            // Get ONE item from the source node
            let first_node = node_instances.get_mut(first_node_id).unwrap();
            let source_items = first_node.process(Value::Null).await?;

            if source_items.is_empty() {
                // Source exhausted
                tracing::info!("Source node {} exhausted", first_node_id);
                break;
            }

            // Flow this ONE item through all remaining nodes
            let mut current_items = source_items;

            for node_id in &graph.execution_order[1..] {
                let node = node_instances.get_mut(node_id).unwrap();
                let mut next_items = Vec::new();

                // Check if this is a streaming node
                if node.is_streaming() {
                    // For streaming nodes: just feed the inputs, collect any outputs that are ready
                    // DON'T call finish_streaming() - that happens after the source exhausts
                    for item in current_items {
                        let results = node.process(item).await?;
                        next_items.extend(results);
                    }
                } else {
                    // For non-streaming nodes: process items one at a time
                    for item in current_items {
                        let results = node.process(item).await?;
                        next_items.extend(results);
                    }
                }

                current_items = next_items;
            }

            // Add final results from this pipeline iteration
            final_results.extend(current_items);
        }

        // After source is exhausted, flush any streaming nodes
        for node_id in &graph.execution_order[1..] {
            let node = node_instances.get_mut(node_id).unwrap();
            if node.is_streaming() {
                tracing::info!("Flushing streaming node {}", node_id);
                let flushed_items = node.finish_streaming().await?;

                // Flow flushed items through remaining nodes in the pipeline
                let mut current_items = flushed_items;

                for next_node_id in &graph.execution_order[(graph.execution_order.iter().position(|id| id == node_id).unwrap() + 1)..] {
                    let next_node = node_instances.get_mut(next_node_id).unwrap();
                    let mut next_items = Vec::new();

                    for item in current_items {
                        let results = next_node.process(item).await?;
                        next_items.extend(results);
                    }

                    current_items = next_items;
                }

                final_results.extend(current_items);
            }
        }

        tracing::info!("Pipeline produced {} total results", final_results.len());

        // Return final results
        if final_results.len() == 1 {
            Ok(final_results.into_iter().next().unwrap())
        } else {
            Ok(Value::Array(final_results))
        }
    }

    /// Execute a DAG source-based pipeline
    async fn execute_dag_source_pipeline(
        &self,
        graph: &PipelineGraph,
        node_instances: &mut HashMap<String, Box<dyn NodeExecutor>>,
    ) -> Result<Value> {
        // For now, just use linear execution
        // TODO: Implement proper DAG execution for source pipelines
        self.execute_linear_source_pipeline(graph, node_instances).await
    }

    /// Execute pipeline using concurrent channels (for streaming nodes)
    /// Each node runs in its own async task with input/output channels
    async fn execute_concurrent_pipeline(
        &self,
        graph: &PipelineGraph,
        mut node_instances: HashMap<String, Box<dyn NodeExecutor>>,
    ) -> Result<ExecutionResult> {
        use tokio::sync::mpsc;
        use tokio::task::JoinHandle;

        // Create channels for each node FIRST
        let mut input_channels: HashMap<String, mpsc::UnboundedSender<Value>> = HashMap::new();
        let mut output_receivers: HashMap<String, mpsc::UnboundedReceiver<Value>> = HashMap::new();

        // Create all channels upfront
        for node_id in &graph.execution_order {
            let (tx, rx) = mpsc::unbounded_channel::<Value>();
            input_channels.insert(node_id.clone(), tx);
            output_receivers.insert(node_id.clone(), rx);
        }

        let mut tasks: Vec<JoinHandle<Result<()>>> = Vec::new();

        // Create output channel for final results
        let (final_tx, mut final_rx) = mpsc::unbounded_channel::<Value>();

        // Spawn a task for each node
        for node_id in &graph.execution_order {
            let mut node = node_instances.remove(node_id).unwrap();
            let graph_node = graph.get_node(node_id).unwrap();

            // Get this node's input receiver
            let mut rx = output_receivers.remove(node_id).unwrap();

            // Get output channels for this node's outputs
            let output_channels: Vec<mpsc::UnboundedSender<Value>> = graph_node.outputs.iter()
                .filter_map(|out_id| input_channels.get(out_id).cloned())
                .collect();

            // If this is a sink node (no outputs), send to final channel
            let is_sink = output_channels.is_empty();
            let final_sender = if is_sink { Some(final_tx.clone()) } else { None };

            let node_id_clone = node_id.clone();
            let is_source = graph_node.inputs.is_empty();

            tracing::info!("Spawning task for node: {} (source: {}, sink: {}, streaming: {}, outputs: {:?})",
                node_id, is_source, is_sink, node.is_streaming(), graph_node.outputs);

            // Spawn async task for this node
            let task = tokio::spawn(async move {
                if is_source {
                    // Source node - call process() once, which returns all generated items
                    // For async generator source nodes, the CPythonExecutor will iterate them completely
                    tracing::info!("Source node {} starting", node_id_clone);
                    let results = node.process(Value::Null).await?;

                    tracing::info!("Source node {} produced {} items", node_id_clone, results.len());

                    tracing::info!("Source node {} sending {} items to {} output channels", node_id_clone, results.len(), output_channels.len());
                    for (idx, result) in results.iter().enumerate() {
                        for out_ch in &output_channels {
                            if out_ch.send(result.clone()).is_err() {
                                tracing::warn!("Output channel closed for node {} at item {}", node_id_clone, idx + 1);
                                return Ok(());
                            }
                        }
                    }
                    tracing::info!("Source node {} finished sending all items", node_id_clone);

                    tracing::info!("Source node {} finished", node_id_clone);
                } else {
                    // Non-source node - process inputs
                    tracing::info!("Processing node {} waiting for inputs", node_id_clone);

                    let mut input_count = 0;
                    let is_streaming_node = node.is_streaming();

                    while let Some(input) = rx.recv().await {
                        input_count += 1;
                        if input_count == 1 || input_count % 50 == 0 {
                            tracing::info!("Node {} received input #{}", node_id_clone, input_count);
                        }
                        let results = node.process(input).await?;
                        if input_count == 1 || input_count % 50 == 0 {
                            tracing::info!("Node {} produced {} results from input #{}", node_id_clone, results.len(), input_count);
                        }

                        for result in results {
                            if is_sink {
                                if let Some(ref final_ch) = final_sender {
                                    if final_ch.send(result.clone()).is_err() {
                                        tracing::warn!("Final output channel closed");
                                        return Ok(());
                                    }
                                }
                            } else {
                                for out_ch in &output_channels {
                                    if out_ch.send(result.clone()).is_err() {
                                        tracing::warn!("Output channel closed for node {}", node_id_clone);
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }

                    // Input channel closed - flush streaming node if applicable
                    tracing::info!("Input channel closed for node {}, received {} total inputs", node_id_clone, input_count);
                    if is_streaming_node {
                        tracing::info!("Flushing streaming node {}", node_id_clone);
                        let flushed = node.finish_streaming().await?;

                        for result in flushed {
                            if is_sink {
                                if let Some(ref final_ch) = final_sender {
                                    if final_ch.send(result.clone()).is_err() {
                                        tracing::warn!("Final output channel closed");
                                        return Ok(());
                                    }
                                }
                            } else {
                                for out_ch in &output_channels {
                                    if out_ch.send(result.clone()).is_err() {
                                        tracing::warn!("Output channel closed for node {}", node_id_clone);
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }

                    tracing::info!("Node {} completed", node_id_clone);
                }

                node.cleanup().await?;
                Ok(())
            });

            tasks.push(task);
        }

        // IMPORTANT: Drop input_channels to close all unused input senders
        // This allows downstream nodes to detect when their input channel closes
        drop(input_channels);

        // Drop the final_tx so final_rx will close when all tasks complete
        drop(final_tx);

        // Collect all final results
        let mut final_results = Vec::new();
        while let Some(result) = final_rx.recv().await {
            final_results.push(result);
        }

        // Wait for all tasks to complete
        for task in tasks {
            task.await.map_err(|e| Error::Execution(format!("Task join error: {}", e)))??;
        }

        tracing::info!("Concurrent pipeline produced {} results", final_results.len());

        // Return final results
        let outputs = if final_results.len() == 1 {
            final_results.into_iter().next().unwrap()
        } else {
            Value::Array(final_results)
        };

        Ok(ExecutionResult {
            status: "success".to_string(),
            outputs,
            graph_info: Some(GraphInfo {
                node_count: graph.node_count(),
                source_count: graph.sources.len(),
                sink_count: graph.sinks.len(),
                execution_order: graph.execution_order.clone(),
            }),
        })
    }

    /// Check if the pipeline is a simple linear chain (no branching or merging)
    fn is_linear_pipeline(&self, graph: &PipelineGraph) -> bool {
        // Linear if every node has at most one input and one output
        for node in graph.nodes.values() {
            if node.inputs.len() > 1 || node.outputs.len() > 1 {
                return false;
            }
        }
        true
    }

    /// Execute a linear pipeline (optimized path for simple chains)
    ///
    /// Phase 1.11.1: Sequential data passing between nodes
    /// Phase 1.11.2: Support for streaming nodes
    async fn execute_linear_pipeline(
        &self,
        graph: &PipelineGraph,
        manifest: &Manifest,
        input_data: Vec<Value>,
    ) -> Result<ExecutionResult> {
        // Initialize all nodes
        let mut node_instances: HashMap<String, Box<dyn NodeExecutor>> = HashMap::new();

        for node_id in &graph.execution_order {
            let graph_node = graph.get_node(node_id).unwrap();

            // Get the corresponding node manifest
            let node_manifest = manifest
                .nodes
                .iter()
                .find(|n| n.id == *node_id)
                .ok_or_else(|| Error::Manifest(format!("Node {} not found in manifest", node_id)))?;

            // Create node instance with runtime selection (Phase 1.10.7)
            let mut node = self.create_node_with_runtime(node_manifest)?;

            // Initialize node
            let context = NodeContext {
                node_id: node_id.clone(),
                node_type: graph_node.node_type.clone(),
                params: graph_node.params.clone(),
                session_id: None,
                metadata: HashMap::new(),
            };

            node.initialize(&context).await?;
            node_instances.insert(node_id.clone(), node);
        }

        // Phase 1.11.1: Execute nodes sequentially, passing data through connections
        let mut current_data = input_data;

        for node_id in &graph.execution_order {
            let node = node_instances.get_mut(node_id).unwrap();
            let mut output_data = Vec::new();

            // Check if this is a streaming node
            if node.is_streaming() {
                // For streaming nodes: feed all inputs first, then collect all outputs
                tracing::info!("Streaming node {} - feeding {} inputs", node_id, current_data.len());

                // Feed all inputs
                for item in current_data {
                    let _ = node.process(item).await?; // Returns empty vec
                }

                // Signal completion and collect all outputs
                output_data = node.finish_streaming().await?;
                tracing::info!("Streaming node {} produced {} outputs", node_id, output_data.len());
            } else {
                // For non-streaming nodes: process items one at a time
                // Phase 1.11.2: Process each item (supports streaming)
                // Phase 1.11.3: Backpressure is implicit - we process one item at a time
                for item in current_data {
                    let results = node.process(item).await?;
                    output_data.extend(results);
                }
                tracing::info!("Node {} processed, {} items remaining", node_id, output_data.len());
            }

            current_data = output_data;
        }

        // Cleanup all nodes
        for (node_id, mut node) in node_instances {
            node.cleanup().await?;
            tracing::info!("Node {} cleaned up", node_id);
        }

        // Return final outputs
        let outputs = if current_data.len() == 1 {
            current_data.into_iter().next().unwrap()
        } else {
            Value::Array(current_data)
        };

        Ok(ExecutionResult {
            status: "success".to_string(),
            outputs,
            graph_info: Some(GraphInfo {
                node_count: graph.node_count(),
                source_count: graph.sources.len(),
                sink_count: graph.sinks.len(),
                execution_order: graph.execution_order.clone(),
            }),
        })
    }

    /// Execute a DAG pipeline with branching and merging
    ///
    /// Phase 1.11.4: Support for complex topologies with multiple paths
    async fn execute_dag_pipeline(
        &self,
        graph: &PipelineGraph,
        manifest: &Manifest,
        input_data: Vec<Value>,
    ) -> Result<ExecutionResult> {
        // Initialize all nodes
        let mut node_instances: HashMap<String, Box<dyn NodeExecutor>> = HashMap::new();

        for node_id in &graph.execution_order {
            let graph_node = graph.get_node(node_id).unwrap();

            let node_manifest = manifest
                .nodes
                .iter()
                .find(|n| n.id == *node_id)
                .ok_or_else(|| Error::Manifest(format!("Node {} not found in manifest", node_id)))?;

            let mut node = self.create_node_with_runtime(node_manifest)?;

            let context = NodeContext {
                node_id: node_id.clone(),
                node_type: graph_node.node_type.clone(),
                params: graph_node.params.clone(),
                session_id: None,
                metadata: HashMap::new(),
            };

            node.initialize(&context).await?;
            node_instances.insert(node_id.clone(), node);
        }

        // Phase 1.11.4: Track data buffers for each node
        let mut node_outputs: HashMap<String, Vec<Value>> = HashMap::new();

        // Initialize source nodes with input data
        for source_id in &graph.sources {
            node_outputs.insert(source_id.clone(), input_data.clone());
        }

        // Phase 1.11.4: Execute nodes in topological order
        for node_id in &graph.execution_order {
            let graph_node = graph.get_node(node_id).unwrap();
            let node = node_instances.get_mut(node_id).unwrap();

            // Collect inputs from all upstream nodes
            let inputs = if graph_node.inputs.is_empty() {
                // Source node - already has data
                node_outputs.get(node_id).cloned().unwrap_or_default()
            } else if graph_node.inputs.len() == 1 {
                // Single input - pass through
                let input_node_id = &graph_node.inputs[0];
                node_outputs.get(input_node_id).cloned().unwrap_or_default()
            } else {
                // Phase 1.11.4: Multiple inputs - merge them
                let mut merged_inputs = Vec::new();
                for input_node_id in &graph_node.inputs {
                    if let Some(input_data) = node_outputs.get(input_node_id) {
                        merged_inputs.extend(input_data.clone());
                    }
                }
                merged_inputs
            };

            // Process data through node
            let mut outputs = Vec::new();
            for item in inputs {
                let results = node.process(item).await?;
                outputs.extend(results);
            }

            tracing::info!("Node {} produced {} outputs", node_id, outputs.len());

            // Phase 1.11.4: Store outputs for downstream nodes or broadcast to multiple outputs
            if !graph_node.outputs.is_empty() {
                // Has downstream consumers - store for them
                node_outputs.insert(node_id.clone(), outputs.clone());
            } else {
                // Sink node - store final outputs
                node_outputs.insert(node_id.clone(), outputs);
            }
        }

        // Cleanup all nodes
        for (node_id, mut node) in node_instances {
            node.cleanup().await?;
            tracing::info!("Node {} cleaned up", node_id);
        }

        // Collect outputs from all sink nodes
        let mut final_outputs = Vec::new();
        for sink_id in &graph.sinks {
            if let Some(sink_data) = node_outputs.get(sink_id) {
                final_outputs.extend(sink_data.clone());
            }
        }

        // Return final outputs
        let outputs = if final_outputs.len() == 1 {
            final_outputs.into_iter().next().unwrap()
        } else {
            Value::Array(final_outputs)
        };

        Ok(ExecutionResult {
            status: "success".to_string(),
            outputs,
            graph_info: Some(GraphInfo {
                node_count: graph.node_count(),
                source_count: graph.sources.len(),
                sink_count: graph.sinks.len(),
                execution_order: graph.execution_order.clone(),
            }),
        })
    }

    /// Check if output is from a streaming node (async generator that returned multiple items)
    ///
    /// Phase 1.11.2: Heuristic to detect streaming output
    fn is_streaming_output(&self, value: &Value) -> bool {
        // If it's an array and seems like streamed chunks, flatten it
        // This is a heuristic - in the future we could use metadata
        if let Some(arr) = value.as_array() {
            // If array has uniform structure, it's likely streaming output
            // For now, we'll be conservative and not flatten
            false
        } else {
            false
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about the pipeline graph
#[derive(Debug, Clone)]
pub struct GraphInfo {
    /// Number of nodes in the graph
    pub node_count: usize,

    /// Number of source nodes
    pub source_count: usize,

    /// Number of sink nodes
    pub sink_count: usize,

    /// Execution order
    pub execution_order: Vec<String>,
}

/// Result of pipeline execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Execution status
    pub status: String,

    /// Output data
    pub outputs: serde_json::Value,

    /// Graph information
    pub graph_info: Option<GraphInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Connection, ManifestMetadata};

    #[test]
    fn test_executor_creation() {
        let executor = Executor::new();
        assert_eq!(executor.config.max_concurrency, 10);
    }

    #[test]
    fn test_graph_linear_pipeline() {
        // Create a simple linear pipeline: A -> B -> C
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "linear-test".to_string(),
                description: None,
                created_at: None,
            },
            nodes: vec![
                crate::manifest::NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
            ],
            connections: vec![
                Connection {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                Connection {
                    from: "B".to_string(),
                    to: "C".to_string(),
                },
            ],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // Check node count
        assert_eq!(graph.node_count(), 3);

        // Check sources and sinks
        assert_eq!(graph.sources.len(), 1);
        assert_eq!(graph.sources[0], "A");
        assert_eq!(graph.sinks.len(), 1);
        assert_eq!(graph.sinks[0], "C");

        // Check execution order (should be A, B, C)
        assert_eq!(graph.execution_order, vec!["A", "B", "C"]);

        // Check node connections
        let node_a = graph.get_node("A").unwrap();
        assert_eq!(node_a.inputs.len(), 0);
        assert_eq!(node_a.outputs, vec!["B"]);

        let node_b = graph.get_node("B").unwrap();
        assert_eq!(node_b.inputs, vec!["A"]);
        assert_eq!(node_b.outputs, vec!["C"]);

        let node_c = graph.get_node("C").unwrap();
        assert_eq!(node_c.inputs, vec!["B"]);
        assert_eq!(node_c.outputs.len(), 0);
    }

    #[test]
    fn test_graph_dag() {
        // Create a DAG:
        //     B
        //    / \
        //   A   D
        //    \ /
        //     C
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "dag-test".to_string(),
                description: None,
                created_at: None,
            },
            nodes: vec![
                crate::manifest::NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "D".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
            ],
            connections: vec![
                Connection { from: "A".to_string(), to: "B".to_string() },
                Connection { from: "A".to_string(), to: "C".to_string() },
                Connection { from: "B".to_string(), to: "D".to_string() },
                Connection { from: "C".to_string(), to: "D".to_string() },
            ],
        };

        let graph = PipelineGraph::from_manifest(&manifest).unwrap();

        // Check basics
        assert_eq!(graph.node_count(), 4);
        assert_eq!(graph.sources, vec!["A"]);
        assert_eq!(graph.sinks, vec!["D"]);

        // Verify execution order is valid
        // A must come before B and C
        // B and C must come before D
        let exec_order = &graph.execution_order;
        let a_idx = exec_order.iter().position(|x| x == "A").unwrap();
        let b_idx = exec_order.iter().position(|x| x == "B").unwrap();
        let c_idx = exec_order.iter().position(|x| x == "C").unwrap();
        let d_idx = exec_order.iter().position(|x| x == "D").unwrap();

        assert!(a_idx < b_idx);
        assert!(a_idx < c_idx);
        assert!(b_idx < d_idx);
        assert!(c_idx < d_idx);
    }

    #[test]
    fn test_graph_cycle_detection() {
        // Create a cycle: A -> B -> C -> A
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "cycle-test".to_string(),
                description: None,
                created_at: None,
            },
            nodes: vec![
                crate::manifest::NodeManifest {
                    id: "A".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "B".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "C".to_string(),
                    node_type: "TestNode".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
            ],
            connections: vec![
                Connection { from: "A".to_string(), to: "B".to_string() },
                Connection { from: "B".to_string(), to: "C".to_string() },
                Connection { from: "C".to_string(), to: "A".to_string() }, // Cycle!
            ],
        };

        let result = PipelineGraph::from_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[tokio::test]
    async fn test_executor_with_graph() {
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "exec-test".to_string(),
                description: Some("Test execution".to_string()),
                created_at: None,
            },
            nodes: vec![
                crate::manifest::NodeManifest {
                    id: "input_0".to_string(),
                    node_type: "DataSource".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "process_1".to_string(),
                    node_type: "Transform".to_string(),
                    params: serde_json::json!({"operation": "add"}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
            ],
            connections: vec![
                Connection {
                    from: "input_0".to_string(),
                    to: "process_1".to_string(),
                },
            ],
        };

        let executor = Executor::new();
        let result = executor.execute(&manifest).await.unwrap();

        assert_eq!(result.status, "success");
        assert!(result.graph_info.is_some());

        let graph_info = result.graph_info.unwrap();
        assert_eq!(graph_info.node_count, 2);
        assert_eq!(graph_info.source_count, 1);
        assert_eq!(graph_info.sink_count, 1);
        assert_eq!(graph_info.execution_order, vec!["input_0", "process_1"]);
    }

    #[tokio::test]
    async fn test_executor_with_actual_execution() {
        // Create a simple pipeline: PassThrough -> Echo
        let manifest = Manifest {
            version: "v1".to_string(),
            metadata: ManifestMetadata {
                name: "execution-test".to_string(),
                description: None,
                created_at: None,
            },
            nodes: vec![
                crate::manifest::NodeManifest {
                    id: "pass_0".to_string(),
                    node_type: "PassThrough".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
                crate::manifest::NodeManifest {
                    id: "echo_1".to_string(),
                    node_type: "Echo".to_string(),
                    params: serde_json::json!({}),
                    capabilities: None,
                    host: None,
                    runtime_hint: None,
                },
            ],
            connections: vec![Connection {
                from: "pass_0".to_string(),
                to: "echo_1".to_string(),
            }],
        };

        let executor = Executor::new();
        let input_data = vec![
            serde_json::json!("test1"),
            serde_json::json!("test2"),
            serde_json::json!("test3"),
        ];

        let result = executor.execute_with_input(&manifest, input_data).await.unwrap();

        assert_eq!(result.status, "success");

        // Should have 3 outputs (one for each input)
        let outputs = result.outputs.as_array().unwrap();
        assert_eq!(outputs.len(), 3);

        // Each output should be wrapped by Echo node
        assert_eq!(outputs[0]["input"], "test1");
        assert_eq!(outputs[0]["counter"], 1);
        assert_eq!(outputs[1]["counter"], 2);
        assert_eq!(outputs[2]["counter"], 3);
    }
}
