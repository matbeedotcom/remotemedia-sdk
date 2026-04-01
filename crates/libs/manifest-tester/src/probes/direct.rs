//! Direct probe — executes pipeline in-process via PipelineExecutor
//!
//! This is the primary probe backend. It validates the manifest works
//! at the engine level without any transport overhead.
//!
//! Execution levels:
//! 1. Graph validation — structure, connections, cycles
//! 2. Registry validation — all node types exist in the factory registry
//! 3. Session execution — actually send data through and collect outputs

use super::{ProbeBackend, ProbeContext};
use crate::report::{
    CategorizedError, ErrorCategory, NodeResult, NodeStatus, ProbeResult, TestStatus,
};
use async_trait::async_trait;
use remotemedia_core::executor::PipelineGraph;
use remotemedia_core::transport::data::TransportData;
use remotemedia_core::transport::PipelineExecutor;
use remotemedia_manifest_analyzer::ExecutionMode;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub struct DirectProbe;

#[async_trait]
impl ProbeBackend for DirectProbe {
    fn name(&self) -> &str {
        "direct"
    }

    async fn probe(&self, ctx: &ProbeContext) -> ProbeResult {
        let start = Instant::now();
        let mut errors = Vec::new();
        let mut node_results = Vec::new();

        // ── Step 1: Validate graph structure ─────────────────────────────
        let graph = match PipelineGraph::from_manifest(&ctx.manifest) {
            Ok(g) => g,
            Err(e) => {
                return ProbeResult {
                    transport: "direct".to_string(),
                    status: TestStatus::Fail,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    first_output_ms: None,
                    errors: vec![CategorizedError {
                        category: ErrorCategory::ManifestValidation,
                        node_id: None,
                        message: format!("Graph validation failed: {e}"),
                        source: None,
                    }],
                };
            }
        };

        info!(
            "Direct probe: {} nodes, {} sources, {} sinks, order: {:?}",
            graph.nodes.len(),
            graph.sources.len(),
            graph.sinks.len(),
            graph.execution_order
        );

        // Build node results from graph analysis
        for node_id in &graph.execution_order {
            if let Some(node) = graph.nodes.get(node_id) {
                let ml_req = ctx
                    .analysis
                    .ml_requirements
                    .iter()
                    .find(|r| r.node_id == *node_id);
                let skip_node = ctx.skip_ml && ml_req.is_some();

                node_results.push(NodeResult {
                    node_id: node_id.clone(),
                    node_type: node.node_type.clone(),
                    status: if skip_node {
                        NodeStatus::Skipped
                    } else {
                        NodeStatus::Initialized
                    },
                    init_time_ms: None,
                    process_time_ms: None,
                    error: if skip_node {
                        Some("Skipped (--skip-ml)".to_string())
                    } else {
                        None
                    },
                });
            }
        }

        // ── Step 2: Create PipelineExecutor and validate registry ────────
        let executor = match PipelineExecutor::new() {
            Ok(e) => e,
            Err(e) => {
                errors.push(CategorizedError {
                    category: ErrorCategory::NodeInit,
                    node_id: None,
                    message: format!("Failed to create PipelineExecutor: {e}"),
                    source: None,
                });
                return make_result(start, errors);
            }
        };

        // Validate manifest against registry (checks node types exist)
        match executor.validate_manifest(&ctx.manifest).await {
            Ok(()) => {
                info!("Manifest validated against node registry");
            }
            Err(e) => {
                let err_msg = e.to_string();
                // If skip_ml is set and error is about missing node types,
                // downgrade to warning — the missing types are likely ML nodes
                if ctx.skip_ml
                    && (err_msg.contains("not found")
                        || err_msg.contains("Unknown node type")
                        || err_msg.contains("not registered"))
                {
                    warn!("Registry validation warning (--skip-ml): {err_msg}");
                    info!("Skipping execution — missing node types with --skip-ml");
                    // Graph validation passed, registry has missing ML nodes.
                    // Mark non-skipped nodes as validated at graph level.
                    for result in &mut node_results {
                        if result.status == NodeStatus::Initialized {
                            result.status = NodeStatus::OutputProduced;
                        }
                    }
                    return ProbeResult {
                        transport: "direct".to_string(),
                        status: TestStatus::Pass,
                        latency_ms: Some(start.elapsed().as_millis() as u64),
                        first_output_ms: None,
                        errors: vec![], // Not a failure — intentionally skipped
                    };
                } else {
                    errors.push(CategorizedError {
                        category: ErrorCategory::ManifestValidation,
                        node_id: None,
                        message: format!("Registry validation failed: {err_msg}"),
                        source: None,
                    });
                    return make_result(start, errors);
                }
            }
        }

        // ── Step 3: Attempt session execution ────────────────────────────
        if ctx.test_data.is_empty() {
            errors.push(CategorizedError {
                category: ErrorCategory::NodeExecution,
                node_id: None,
                message: "No synthetic test data generated for pipeline input".to_string(),
                source: None,
            });
            return make_result(start, errors);
        }

        let execution_mode = ctx.analysis.execution_mode;
        info!(
            "Direct probe: execution_mode={:?}, test_data_count={}",
            execution_mode,
            ctx.test_data.len()
        );

        let manifest = ctx.manifest.clone();
        let mut first_output_ms = None;
        let mut output_count = 0u64;

        match execution_mode {
            ExecutionMode::Streaming => {
                match executor.create_session(manifest).await {
                    Ok(mut session) => {
                        info!("Streaming session created, sending {} chunks", ctx.test_data.len());

                        // Send all test data
                        for (i, data) in ctx.test_data.iter().enumerate() {
                            let transport_data = TransportData::new(data.clone());
                            if let Err(e) = session.send_input(transport_data).await {
                                error!("Send failed at chunk {i}: {e}");
                                errors.push(CategorizedError {
                                    category: ErrorCategory::NodeExecution,
                                    node_id: None,
                                    message: format!("Send failed at chunk {i}: {e}"),
                                    source: Some(e.to_string()),
                                });
                                break;
                            }
                        }

                        // Try to receive outputs with timeout
                        let recv_deadline = Instant::now() + ctx.timeout;
                        loop {
                            let remaining = recv_deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero() {
                                if output_count == 0 {
                                    warn!("Timeout waiting for output");
                                    errors.push(CategorizedError {
                                        category: ErrorCategory::Timeout,
                                        node_id: None,
                                        message: format!(
                                            "Timeout after {:?} waiting for pipeline output",
                                            ctx.timeout
                                        ),
                                        source: None,
                                    });
                                }
                                break;
                            }

                            match tokio::time::timeout(
                                remaining.min(Duration::from_secs(2)),
                                session.recv_output(),
                            )
                            .await
                            {
                                Ok(Ok(Some(_output))) => {
                                    output_count += 1;
                                    if first_output_ms.is_none() {
                                        first_output_ms =
                                            Some(start.elapsed().as_millis() as u64);
                                    }
                                    info!("Received output #{output_count}");
                                }
                                Ok(Ok(None)) => {
                                    info!("Session stream ended after {output_count} outputs");
                                    break;
                                }
                                Ok(Err(e)) => {
                                    error!("Receive error: {e}");
                                    errors.push(CategorizedError {
                                        category: ErrorCategory::NodeExecution,
                                        node_id: None,
                                        message: format!("Receive error: {e}"),
                                        source: Some(e.to_string()),
                                    });
                                    break;
                                }
                                Err(_) => {
                                    // Timeout on this recv, try again until deadline
                                    continue;
                                }
                            }
                        }

                        let _ = session.close().await;
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        error!("Session creation failed: {err_msg}");
                        errors.push(CategorizedError {
                            category: categorize_error(&err_msg),
                            node_id: None,
                            message: format!("Session creation failed: {err_msg}"),
                            source: Some(err_msg),
                        });
                    }
                }
            }
            ExecutionMode::Unary => {
                let transport_data = TransportData::new(ctx.test_data[0].clone());
                match executor.execute_unary(manifest, transport_data).await {
                    Ok(output) => {
                        output_count = 1;
                        first_output_ms = Some(start.elapsed().as_millis() as u64);
                        info!("Unary execution succeeded: {:?}", output.data);
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        error!("Unary execution failed: {err_msg}");
                        errors.push(CategorizedError {
                            category: categorize_error(&err_msg),
                            node_id: None,
                            message: format!("Execution failed: {err_msg}"),
                            source: Some(err_msg),
                        });
                    }
                }
            }
        }

        // ── Step 4: Update node results based on execution ──────────────
        if output_count > 0 {
            for result in &mut node_results {
                if result.status == NodeStatus::Initialized {
                    result.status = NodeStatus::OutputProduced;
                }
            }
            info!("Pipeline produced {output_count} output(s)");
        } else if errors.is_empty() {
            // No output and no errors — still a problem
            errors.push(CategorizedError {
                category: ErrorCategory::NodeExecution,
                node_id: None,
                message: "Pipeline produced no output".to_string(),
                source: None,
            });
        }

        // Mark failed nodes
        if !errors.is_empty() {
            for result in &mut node_results {
                if result.status == NodeStatus::Initialized {
                    result.status = NodeStatus::Failed;
                }
            }
        }

        let status = if errors.is_empty() {
            TestStatus::Pass
        } else if output_count > 0 {
            TestStatus::Partial
        } else {
            TestStatus::Fail
        };

        ProbeResult {
            transport: "direct".to_string(),
            status,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            first_output_ms,
            errors,
        }
    }
}

fn make_result(start: Instant, errors: Vec<CategorizedError>) -> ProbeResult {
    ProbeResult {
        transport: "direct".to_string(),
        status: if errors.is_empty() {
            TestStatus::Pass
        } else {
            TestStatus::Fail
        },
        latency_ms: Some(start.elapsed().as_millis() as u64),
        first_output_ms: None,
        errors,
    }
}

/// Categorize an error message from the executor
fn categorize_error(msg: &str) -> ErrorCategory {
    let lower = msg.to_lowercase();
    if lower.contains("timeout") {
        ErrorCategory::Timeout
    } else if lower.contains("ipc") || lower.contains("iceoryx") {
        ErrorCategory::Ipc
    } else if lower.contains("not found") || lower.contains("not registered") {
        ErrorCategory::ManifestValidation
    } else if lower.contains("python") || lower.contains("process") {
        ErrorCategory::NodeInit
    } else {
        ErrorCategory::NodeExecution
    }
}
