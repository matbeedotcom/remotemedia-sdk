//! Pipeline observation types for the LLM controller

use std::collections::HashMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// A snapshot of pipeline state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineObservation {
    /// When this observation was taken
    #[serde(skip)]
    pub timestamp: Option<Instant>,

    /// Timestamp as milliseconds since session start (for serialization)
    pub timestamp_ms: u64,

    /// Session identifier
    pub session_id: String,

    /// Per-node metrics
    pub node_metrics: HashMap<String, NodeMetrics>,

    /// Graph-level aggregated metrics
    pub graph_metrics: GraphMetrics,

    /// Recent errors (sliding window)
    pub recent_errors: Vec<PipelineError>,

    /// Current graph topology
    pub topology: GraphTopology,
}

/// Metrics for a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    /// Node identifier
    pub node_id: String,

    /// Node type (e.g., "AudioResampler", "WhisperTranscriber")
    pub node_type: String,

    /// Executor type
    pub executor: ExecutorType,

    /// Median latency (exponential moving average)
    pub latency_p50_ms: f64,

    /// 99th percentile latency
    pub latency_p99_ms: f64,

    /// Latency trend over recent observations
    pub latency_trend: Trend,

    /// Items processed per second
    pub items_per_second: f64,

    /// Current queue depth
    pub queue_depth: usize,

    /// Queue capacity
    pub queue_capacity: usize,

    /// Errors per second (recent)
    pub error_rate: f64,

    /// Last error message, if any
    pub last_error: Option<String>,

    /// Node health status
    pub status: NodeStatus,

    /// Memory usage in MB (for multiprocess nodes)
    pub memory_mb: Option<f64>,

    /// CPU usage percentage (for multiprocess nodes)
    pub cpu_percent: Option<f64>,

    /// Whether this node is currently bypassed
    pub is_bypassed: bool,

    /// Whether this node is on the critical path
    pub is_critical_path: bool,
}

impl NodeMetrics {
    /// Calculate queue utilization as a percentage
    pub fn queue_utilization(&self) -> f64 {
        if self.queue_capacity == 0 {
            0.0
        } else {
            self.queue_depth as f64 / self.queue_capacity as f64
        }
    }

    /// Check if this node is experiencing issues
    pub fn has_issues(&self) -> bool {
        matches!(self.status, NodeStatus::Failed | NodeStatus::Degraded)
            || self.error_rate > 0.01
            || matches!(self.latency_trend, Trend::Rising)
            || self.queue_utilization() > 0.8
    }
}

/// Executor type for a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutorType {
    /// Native Rust execution (fastest)
    Native,
    /// Python multiprocess (isolated)
    Multiprocess,
    /// WebAssembly (browser)
    Wasm,
    /// In-process Python (deprecated)
    #[serde(rename = "cpython")]
    CPython,
}

impl std::fmt::Display for ExecutorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutorType::Native => write!(f, "Native"),
            ExecutorType::Multiprocess => write!(f, "Multiprocess"),
            ExecutorType::Wasm => write!(f, "Wasm"),
            ExecutorType::CPython => write!(f, "CPython"),
        }
    }
}

/// Trend direction for a metric
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trend {
    /// Metric is increasing
    Rising,
    /// Metric is decreasing
    Falling,
    /// Metric is stable
    Stable,
}

impl std::fmt::Display for Trend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trend::Rising => write!(f, "↑"),
            Trend::Falling => write!(f, "↓"),
            Trend::Stable => write!(f, "→"),
        }
    }
}

/// Health status of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Node is operating normally
    Healthy,
    /// Node is operational but experiencing issues
    Degraded,
    /// Node has failed
    Failed,
    /// Node is starting up
    Starting,
    /// Node status is unknown
    Unknown,
}

impl std::fmt::Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeStatus::Healthy => write!(f, "✓ Healthy"),
            NodeStatus::Degraded => write!(f, "⚠ Degraded"),
            NodeStatus::Failed => write!(f, "✗ Failed"),
            NodeStatus::Starting => write!(f, "◐ Starting"),
            NodeStatus::Unknown => write!(f, "? Unknown"),
        }
    }
}

/// Graph-level aggregated metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphMetrics {
    /// End-to-end latency through the pipeline
    pub end_to_end_latency_ms: f64,

    /// Total throughput (items per second at output)
    pub total_throughput: f64,

    /// Node that is the current bottleneck
    pub bottleneck_node: Option<String>,

    /// Nodes on the critical path (longest latency path)
    pub critical_path: Vec<String>,

    /// Total number of nodes
    pub node_count: usize,

    /// Number of healthy nodes
    pub healthy_node_count: usize,

    /// Number of failed nodes
    pub failed_node_count: usize,

    /// Number of bypassed nodes
    pub bypassed_node_count: usize,
}

/// Error event in the pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineError {
    /// When the error occurred (ms since session start)
    pub timestamp_ms: u64,

    /// Node that produced the error
    pub node_id: String,

    /// Error message
    pub message: String,

    /// Error category
    pub category: ErrorCategory,

    /// Whether this error was recovered from
    pub recovered: bool,
}

/// Category of pipeline error
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Process crashed or exited
    ProcessCrash,
    /// Out of memory
    OutOfMemory,
    /// Timeout
    Timeout,
    /// Invalid input data
    InvalidInput,
    /// External service failure (API, network)
    ExternalService,
    /// Internal logic error
    InternalError,
    /// Unknown error type
    Unknown,
}

/// Current graph topology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphTopology {
    /// Node IDs in topological order
    pub nodes: Vec<String>,

    /// Edges as (from, to) pairs
    pub edges: Vec<(String, String)>,

    /// Input nodes (sources)
    pub input_nodes: Vec<String>,

    /// Output nodes (sinks)
    pub output_nodes: Vec<String>,
}

impl GraphTopology {
    /// Get downstream nodes from a given node
    pub fn downstream(&self, node_id: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|(from, _)| from == node_id)
            .map(|(_, to)| to.as_str())
            .collect()
    }

    /// Get upstream nodes to a given node
    pub fn upstream(&self, node_id: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|(_, to)| to == node_id)
            .map(|(from, _)| from.as_str())
            .collect()
    }
}

/// Builder for constructing observations from runtime data
pub struct ObservationBuilder {
    session_id: String,
    session_start: Instant,
    node_metrics: HashMap<String, NodeMetrics>,
    recent_errors: Vec<PipelineError>,
    topology: GraphTopology,
}

impl ObservationBuilder {
    pub fn new(session_id: String, session_start: Instant) -> Self {
        Self {
            session_id,
            session_start,
            node_metrics: HashMap::new(),
            recent_errors: Vec::new(),
            topology: GraphTopology {
                nodes: Vec::new(),
                edges: Vec::new(),
                input_nodes: Vec::new(),
                output_nodes: Vec::new(),
            },
        }
    }

    pub fn add_node_metrics(&mut self, metrics: NodeMetrics) {
        self.node_metrics.insert(metrics.node_id.clone(), metrics);
    }

    pub fn add_error(&mut self, error: PipelineError) {
        self.recent_errors.push(error);
    }

    pub fn set_topology(&mut self, topology: GraphTopology) {
        self.topology = topology;
    }

    pub fn build(self) -> PipelineObservation {
        let now = Instant::now();
        let timestamp_ms = now.duration_since(self.session_start).as_millis() as u64;

        // Compute graph metrics
        let graph_metrics = self.compute_graph_metrics();

        PipelineObservation {
            timestamp: Some(now),
            timestamp_ms,
            session_id: self.session_id,
            node_metrics: self.node_metrics,
            graph_metrics,
            recent_errors: self.recent_errors,
            topology: self.topology,
        }
    }

    fn compute_graph_metrics(&self) -> GraphMetrics {
        let node_count = self.node_metrics.len();
        let healthy_node_count = self
            .node_metrics
            .values()
            .filter(|m| matches!(m.status, NodeStatus::Healthy))
            .count();
        let failed_node_count = self
            .node_metrics
            .values()
            .filter(|m| matches!(m.status, NodeStatus::Failed))
            .count();
        let bypassed_node_count = self.node_metrics.values().filter(|m| m.is_bypassed).count();

        // Find bottleneck (node with highest latency on critical path)
        let bottleneck_node = self
            .node_metrics
            .values()
            .filter(|m| m.is_critical_path)
            .max_by(|a, b| a.latency_p50_ms.partial_cmp(&b.latency_p50_ms).unwrap())
            .map(|m| m.node_id.clone());

        // Sum latencies on critical path for end-to-end
        let end_to_end_latency_ms: f64 = self
            .node_metrics
            .values()
            .filter(|m| m.is_critical_path)
            .map(|m| m.latency_p50_ms)
            .sum();

        // Throughput is limited by the slowest node
        let total_throughput = self
            .node_metrics
            .values()
            .map(|m| m.items_per_second)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // Critical path nodes
        let critical_path: Vec<String> = self
            .node_metrics
            .values()
            .filter(|m| m.is_critical_path)
            .map(|m| m.node_id.clone())
            .collect();

        GraphMetrics {
            end_to_end_latency_ms,
            total_throughput,
            bottleneck_node,
            critical_path,
            node_count,
            healthy_node_count,
            failed_node_count,
            bypassed_node_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_metrics_has_issues() {
        let healthy = NodeMetrics {
            node_id: "test".to_string(),
            node_type: "Test".to_string(),
            executor: ExecutorType::Native,
            latency_p50_ms: 10.0,
            latency_p99_ms: 20.0,
            latency_trend: Trend::Stable,
            items_per_second: 100.0,
            queue_depth: 5,
            queue_capacity: 50,
            error_rate: 0.0,
            last_error: None,
            status: NodeStatus::Healthy,
            memory_mb: None,
            cpu_percent: None,
            is_bypassed: false,
            is_critical_path: true,
        };

        assert!(!healthy.has_issues());

        let failing = NodeMetrics {
            status: NodeStatus::Failed,
            ..healthy.clone()
        };
        assert!(failing.has_issues());

        let high_errors = NodeMetrics {
            error_rate: 0.05,
            ..healthy.clone()
        };
        assert!(high_errors.has_issues());

        let rising_latency = NodeMetrics {
            latency_trend: Trend::Rising,
            ..healthy.clone()
        };
        assert!(rising_latency.has_issues());

        let queue_pressure = NodeMetrics {
            queue_depth: 45,
            queue_capacity: 50,
            ..healthy
        };
        assert!(queue_pressure.has_issues());
    }

    #[test]
    fn test_topology_upstream_downstream() {
        let topology = GraphTopology {
            nodes: vec!["a".into(), "b".into(), "c".into()],
            edges: vec![("a".into(), "b".into()), ("b".into(), "c".into())],
            input_nodes: vec!["a".into()],
            output_nodes: vec!["c".into()],
        };

        assert_eq!(topology.downstream("a"), vec!["b"]);
        assert_eq!(topology.downstream("b"), vec!["c"]);
        assert!(topology.downstream("c").is_empty());

        assert!(topology.upstream("a").is_empty());
        assert_eq!(topology.upstream("b"), vec!["a"]);
        assert_eq!(topology.upstream("c"), vec!["b"]);
    }
}
