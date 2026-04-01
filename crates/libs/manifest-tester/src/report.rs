//! Structured error reporting for manifest tests

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Overall test status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    Pass,
    Fail,
    Partial,
    Skipped,
}

/// Per-node execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    Initialized,
    Running,
    OutputProduced,
    Failed,
    Skipped,
}

/// Error category for structured reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    ManifestParse,
    ManifestValidation,
    Prerequisite,
    NodeInit,
    NodeExecution,
    Transport,
    Timeout,
    Ipc,
}

/// A categorized error with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorizedError {
    pub category: ErrorCategory,
    pub node_id: Option<String>,
    pub message: String,
    pub source: Option<String>,
}

/// Per-node test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: String,
    pub node_type: String,
    pub status: NodeStatus,
    pub init_time_ms: Option<u64>,
    pub process_time_ms: Option<u64>,
    pub error: Option<String>,
}

/// Per-probe (transport) test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub transport: String,
    pub status: TestStatus,
    pub latency_ms: Option<u64>,
    pub first_output_ms: Option<u64>,
    pub errors: Vec<CategorizedError>,
}

/// Latency metrics across probes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencyMetrics {
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub mean_ms: Option<f64>,
}

/// Full manifest test report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestTestReport {
    pub manifest_path: PathBuf,
    pub manifest_name: String,
    pub overall_status: TestStatus,
    pub node_results: Vec<NodeResult>,
    pub probe_results: Vec<ProbeResult>,
    pub latency: LatencyMetrics,
    pub errors: Vec<CategorizedError>,
    pub duration: Duration,
}

impl ManifestTestReport {
    /// Create a new empty report
    pub fn new(manifest_path: PathBuf, manifest_name: String) -> Self {
        Self {
            manifest_path,
            manifest_name,
            overall_status: TestStatus::Pass,
            node_results: Vec::new(),
            probe_results: Vec::new(),
            latency: LatencyMetrics::default(),
            errors: Vec::new(),
            duration: Duration::ZERO,
        }
    }

    /// Compute overall status from probe results
    pub fn finalize(&mut self) {
        if self.probe_results.is_empty() {
            self.overall_status = TestStatus::Skipped;
            return;
        }

        let all_pass = self.probe_results.iter().all(|p| p.status == TestStatus::Pass);
        let all_fail = self
            .probe_results
            .iter()
            .all(|p| p.status == TestStatus::Fail);
        let all_skipped = self
            .probe_results
            .iter()
            .all(|p| p.status == TestStatus::Skipped);

        self.overall_status = if all_pass {
            TestStatus::Pass
        } else if all_skipped {
            TestStatus::Skipped
        } else if all_fail {
            TestStatus::Fail
        } else {
            TestStatus::Partial
        };

        // Collect all errors
        self.errors = self
            .probe_results
            .iter()
            .flat_map(|p| p.errors.clone())
            .collect();

        // Compute latency metrics from probe results
        let latencies: Vec<f64> = self
            .probe_results
            .iter()
            .filter_map(|p| p.latency_ms.map(|l| l as f64))
            .collect();

        if !latencies.is_empty() {
            let mut sorted = latencies.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let len = sorted.len();
            self.latency.mean_ms = Some(sorted.iter().sum::<f64>() / len as f64);
            self.latency.p50_ms = Some(sorted[len / 2]);
            self.latency.p95_ms = Some(sorted[(len as f64 * 0.95) as usize]);
            self.latency.p99_ms = Some(sorted[(len as f64 * 0.99) as usize]);
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}" ))
    }
}

impl fmt::Display for ManifestTestReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Manifest Test Report: {}", self.manifest_name)?;
        writeln!(f, "  Path: {}", self.manifest_path.display())?;
        writeln!(f, "  Status: {:?}", self.overall_status)?;
        writeln!(f, "  Duration: {:?}", self.duration)?;
        writeln!(f)?;

        if !self.node_results.is_empty() {
            writeln!(f, "  Nodes:")?;
            for node in &self.node_results {
                let status_icon = match node.status {
                    NodeStatus::OutputProduced => "✓",
                    NodeStatus::Failed => "✗",
                    NodeStatus::Skipped => "⊘",
                    _ => "·",
                };
                write!(f, "    {status_icon} {} ({})", node.node_id, node.node_type)?;
                if let Some(ms) = node.process_time_ms {
                    write!(f, " [{ms}ms]")?;
                }
                if let Some(err) = &node.error {
                    write!(f, " — {err}")?;
                }
                writeln!(f)?;
            }
            writeln!(f)?;
        }

        if !self.probe_results.is_empty() {
            writeln!(f, "  Probes:")?;
            for probe in &self.probe_results {
                let status_icon = match probe.status {
                    TestStatus::Pass => "✓",
                    TestStatus::Fail => "✗",
                    TestStatus::Skipped => "⊘",
                    TestStatus::Partial => "~",
                };
                write!(f, "    {status_icon} {}", probe.transport)?;
                if let Some(ms) = probe.latency_ms {
                    write!(f, " [{ms}ms]")?;
                }
                writeln!(f)?;
                for err in &probe.errors {
                    writeln!(f, "      {:?}: {}", err.category, err.message)?;
                }
            }
            writeln!(f)?;
        }

        if !self.errors.is_empty() {
            writeln!(f, "  Errors ({}):", self.errors.len())?;
            for err in &self.errors {
                writeln!(
                    f,
                    "    [{:?}] {}{}",
                    err.category,
                    err.message,
                    err.node_id
                        .as_ref()
                        .map(|id| format!(" (node: {id})"))
                        .unwrap_or_default()
                )?;
            }
        }

        Ok(())
    }
}
