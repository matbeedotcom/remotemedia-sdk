//! Context building and prompt rendering for LLM controller

use serde::{Deserialize, Serialize};

use super::{
    actions::ActionSchema,
    observation::{NodeMetrics, NodeStatus, PipelineObservation, Trend},
    ActionRecord, NodeCatalog,
};

/// Full context provided to the LLM for reasoning
#[derive(Debug, Clone, Serialize)]
pub struct ControllerContext {
    /// Current observation of pipeline state
    pub observation: PipelineObservation,

    /// Historical observations (for trend analysis)
    pub history: Vec<PipelineObservation>,

    /// Previous actions and their outcomes
    pub action_history: Vec<ActionRecord>,

    /// Available node types for replacements
    pub node_catalog: NodeCatalog,

    /// Session constraints (SLOs)
    pub constraints: SessionConstraints,

    /// Available actions the LLM can take
    pub available_actions: Vec<ActionSchema>,
}

impl ControllerContext {
    /// Render this context as a prompt for the LLM
    pub fn render_prompt(&self) -> String {
        let mut prompt = String::new();

        // Header
        prompt.push_str("## Current Pipeline State\n\n");
        prompt.push_str(&format!(
            "Session: {}\n",
            self.observation.session_id
        ));
        prompt.push_str(&format!(
            "Uptime: {}ms\n",
            self.observation.timestamp_ms
        ));
        prompt.push_str(&format!(
            "Constraints: max_latency={}ms, min_throughput={}\n\n",
            self.constraints.max_latency_ms.map(|v| v.to_string()).unwrap_or_else(|| "none".to_string()),
            self.constraints.min_throughput.map(|v| v.to_string()).unwrap_or_else(|| "none".to_string()),
        ));

        // Node metrics table
        prompt.push_str("## Node Metrics\n\n");
        prompt.push_str("| Node | Type | Executor | P50 | P99 | Trend | Queue | Err/s | Status |\n");
        prompt.push_str("|------|------|----------|-----|-----|-------|-------|-------|--------|\n");

        for metrics in self.observation.node_metrics.values() {
            prompt.push_str(&self.render_node_row(metrics));
        }
        prompt.push('\n');

        // Graph metrics
        prompt.push_str("## Graph Metrics\n\n");
        let gm = &self.observation.graph_metrics;
        prompt.push_str(&format!("- End-to-end latency: {:.1}ms\n", gm.end_to_end_latency_ms));
        prompt.push_str(&format!("- Throughput: {:.1} items/sec\n", gm.total_throughput));
        prompt.push_str(&format!(
            "- Bottleneck: {}\n",
            gm.bottleneck_node.as_deref().unwrap_or("none")
        ));
        prompt.push_str(&format!("- Critical path: {}\n", gm.critical_path.join(" → ")));
        prompt.push_str(&format!(
            "- Nodes: {} total, {} healthy, {} failed, {} bypassed\n\n",
            gm.node_count, gm.healthy_node_count, gm.failed_node_count, gm.bypassed_node_count
        ));

        // Recent errors
        prompt.push_str("## Recent Errors (last 60s)\n\n");
        if self.observation.recent_errors.is_empty() {
            prompt.push_str("No recent errors.\n\n");
        } else {
            for error in &self.observation.recent_errors {
                prompt.push_str(&format!(
                    "- [{}ms] {}: {} ({})\n",
                    error.timestamp_ms,
                    error.node_id,
                    error.message,
                    if error.recovered { "recovered" } else { "unrecovered" }
                ));
            }
            prompt.push('\n');
        }

        // Recent actions
        prompt.push_str("## Recent Actions (last 5 minutes)\n\n");
        let recent_actions: Vec<_> = self.action_history.iter().rev().take(5).collect();
        if recent_actions.is_empty() {
            prompt.push_str("No recent actions.\n\n");
        } else {
            for action in recent_actions {
                let outcome = match &action.outcome {
                    super::actions::ActionOutcome::Success { .. } => "✓ Success",
                    super::actions::ActionOutcome::Failed { .. } => "✗ Failed",
                    super::actions::ActionOutcome::Rejected { .. } => "⊘ Rejected",
                };
                prompt.push_str(&format!(
                    "- {:?}: {} → {}\n",
                    action.action, action.llm_reasoning, outcome
                ));
            }
            prompt.push('\n');
        }

        // Available actions
        prompt.push_str("## Available Actions\n\n");
        for action in &self.available_actions {
            prompt.push_str(&format!("- **{}**: {}\n", action.name, action.description));
            prompt.push_str(&format!("  Parameters: {}\n", action.parameters.join(", ")));
        }
        prompt.push('\n');

        // Node catalog (for replacements)
        if !self.node_catalog.nodes.is_empty() {
            prompt.push_str("## Node Catalog (available replacements)\n\n");
            for node in &self.node_catalog.nodes {
                prompt.push_str(&format!(
                    "- **{}**: {} (executors: {})\n",
                    node.node_type,
                    node.description,
                    node.available_executors.join(", ")
                ));
            }
            prompt.push('\n');
        }

        // Latency trend analysis
        if self.history.len() >= 5 {
            prompt.push_str("## Latency Trend (last 5 observations)\n\n");
            for obs in self.history.iter().rev().take(5) {
                prompt.push_str(&format!(
                    "- [{}ms] e2e: {:.1}ms, throughput: {:.1}/s\n",
                    obs.timestamp_ms,
                    obs.graph_metrics.end_to_end_latency_ms,
                    obs.graph_metrics.total_throughput
                ));
            }
            prompt.push('\n');
        }

        prompt.push_str("---\n\n");
        prompt.push_str("Analyze the pipeline state and decide on an action. ");
        prompt.push_str("Respond with JSON containing: reasoning, issues, and action.\n");

        prompt
    }

    fn render_node_row(&self, m: &NodeMetrics) -> String {
        let status_icon = match m.status {
            NodeStatus::Healthy => "✓",
            NodeStatus::Degraded => "⚠",
            NodeStatus::Failed => "✗",
            NodeStatus::Starting => "◐",
            NodeStatus::Unknown => "?",
        };

        let trend_icon = match m.latency_trend {
            Trend::Rising => "↑",
            Trend::Falling => "↓",
            Trend::Stable => "→",
        };

        let bypassed = if m.is_bypassed { " (bypassed)" } else { "" };

        format!(
            "| {}{} | {} | {} | {:.1}ms | {:.1}ms | {} | {}/{} | {:.3} | {} |\n",
            m.node_id,
            bypassed,
            m.node_type,
            m.executor,
            m.latency_p50_ms,
            m.latency_p99_ms,
            trend_icon,
            m.queue_depth,
            m.queue_capacity,
            m.error_rate,
            status_icon
        )
    }

    /// Get a summary of issues for quick analysis
    pub fn summarize_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Check each node for issues
        for metrics in self.observation.node_metrics.values() {
            if matches!(metrics.status, NodeStatus::Failed) {
                issues.push(format!("Node '{}' has failed", metrics.node_id));
            }
            if matches!(metrics.status, NodeStatus::Degraded) {
                issues.push(format!("Node '{}' is degraded", metrics.node_id));
            }
            if metrics.error_rate > 0.01 {
                issues.push(format!(
                    "Node '{}' has high error rate: {:.1}%",
                    metrics.node_id,
                    metrics.error_rate * 100.0
                ));
            }
            if matches!(metrics.latency_trend, Trend::Rising) {
                issues.push(format!(
                    "Node '{}' latency is rising",
                    metrics.node_id
                ));
            }
            if metrics.queue_utilization() > 0.8 {
                issues.push(format!(
                    "Node '{}' queue is {:.0}% full",
                    metrics.node_id,
                    metrics.queue_utilization() * 100.0
                ));
            }
        }

        // Check constraint violations
        if let Some(max_latency) = self.constraints.max_latency_ms {
            if self.observation.graph_metrics.end_to_end_latency_ms > max_latency {
                issues.push(format!(
                    "Latency SLO violation: {:.1}ms > {:.1}ms",
                    self.observation.graph_metrics.end_to_end_latency_ms,
                    max_latency
                ));
            }
        }

        if let Some(min_throughput) = self.constraints.min_throughput {
            if self.observation.graph_metrics.total_throughput < min_throughput {
                issues.push(format!(
                    "Throughput SLO violation: {:.1}/s < {:.1}/s",
                    self.observation.graph_metrics.total_throughput,
                    min_throughput
                ));
            }
        }

        issues
    }
}

/// Constraints that the controller should enforce
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConstraints {
    /// Maximum end-to-end latency in milliseconds
    pub max_latency_ms: Option<f64>,

    /// Minimum throughput (items per second)
    pub min_throughput: Option<f64>,

    /// Maximum error rate (errors per second)
    pub max_error_rate: Option<f64>,

    /// Maximum queue utilization (0.0 - 1.0)
    pub max_queue_utilization: Option<f64>,

    /// Maximum memory usage per node in MB
    pub max_memory_mb: Option<f64>,
}

impl SessionConstraints {
    /// Create constraints for real-time audio (strict latency)
    pub fn realtime_audio() -> Self {
        Self {
            max_latency_ms: Some(100.0),
            min_throughput: Some(50.0),
            max_error_rate: Some(0.01),
            max_queue_utilization: Some(0.7),
            max_memory_mb: Some(1024.0),
        }
    }

    /// Create constraints for batch processing (throughput-focused)
    pub fn batch_processing() -> Self {
        Self {
            max_latency_ms: None,
            min_throughput: Some(100.0),
            max_error_rate: Some(0.001),
            max_queue_utilization: Some(0.9),
            max_memory_mb: Some(4096.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_render_prompt_basic() {
        let context = ControllerContext {
            observation: PipelineObservation {
                timestamp: None,
                timestamp_ms: 5000,
                session_id: "test-session".to_string(),
                node_metrics: HashMap::new(),
                graph_metrics: super::super::observation::GraphMetrics {
                    end_to_end_latency_ms: 50.0,
                    total_throughput: 100.0,
                    bottleneck_node: None,
                    critical_path: vec![],
                    node_count: 0,
                    healthy_node_count: 0,
                    failed_node_count: 0,
                    bypassed_node_count: 0,
                },
                recent_errors: vec![],
                topology: super::super::observation::GraphTopology {
                    nodes: vec![],
                    edges: vec![],
                    input_nodes: vec![],
                    output_nodes: vec![],
                },
            },
            history: vec![],
            action_history: vec![],
            node_catalog: NodeCatalog { nodes: vec![] },
            constraints: SessionConstraints::default(),
            available_actions: vec![],
        };

        let prompt = context.render_prompt();
        assert!(prompt.contains("test-session"));
        assert!(prompt.contains("End-to-end latency: 50.0ms"));
    }

    #[test]
    fn test_summarize_issues() {
        let mut node_metrics = HashMap::new();
        node_metrics.insert(
            "failing_node".to_string(),
            super::super::observation::NodeMetrics {
                node_id: "failing_node".to_string(),
                node_type: "Test".to_string(),
                executor: super::super::observation::ExecutorType::Native,
                latency_p50_ms: 10.0,
                latency_p99_ms: 20.0,
                latency_trend: Trend::Stable,
                items_per_second: 100.0,
                queue_depth: 5,
                queue_capacity: 50,
                error_rate: 0.0,
                last_error: None,
                status: NodeStatus::Failed,
                memory_mb: None,
                cpu_percent: None,
                is_bypassed: false,
                is_critical_path: true,
            },
        );

        let context = ControllerContext {
            observation: PipelineObservation {
                timestamp: None,
                timestamp_ms: 5000,
                session_id: "test-session".to_string(),
                node_metrics,
                graph_metrics: super::super::observation::GraphMetrics {
                    end_to_end_latency_ms: 50.0,
                    total_throughput: 100.0,
                    bottleneck_node: None,
                    critical_path: vec![],
                    node_count: 1,
                    healthy_node_count: 0,
                    failed_node_count: 1,
                    bypassed_node_count: 0,
                },
                recent_errors: vec![],
                topology: super::super::observation::GraphTopology {
                    nodes: vec![],
                    edges: vec![],
                    input_nodes: vec![],
                    output_nodes: vec![],
                },
            },
            history: vec![],
            action_history: vec![],
            node_catalog: NodeCatalog { nodes: vec![] },
            constraints: SessionConstraints::default(),
            available_actions: vec![],
        };

        let issues = context.summarize_issues();
        assert!(issues.iter().any(|i| i.contains("failing_node") && i.contains("failed")));
    }
}
