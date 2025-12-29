//! LLM Pipeline Controller (Cognitive Pipeline Debugger)
//!
//! This module implements an autonomous control plane that uses LLM reasoning
//! to auto-heal and optimize pipelines at runtime.
//!
//! # Architecture
//!
//! The controller implements an OODA (Observe-Orient-Decide-Act) loop:
//!
//! ```text
//! ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐
//! │ OBSERVE  │──▶│  ORIENT  │──▶│  DECIDE  │──▶│   ACT    │
//! │ Metrics  │   │ Context  │   │   LLM    │   │ Mutate   │
//! └──────────┘   └──────────┘   └──────────┘   └──────────┘
//!      ▲                                            │
//!      └────────────────────────────────────────────┘
//! ```
//!
//! # Key Concepts
//!
//! - **Observation**: Metrics collected from nodes (latency, errors, queue depth)
//! - **Context**: Historical data + node catalog + constraints
//! - **Action**: Graph mutations (replace, bypass, restart, scale, etc.)
//! - **Guardrails**: Circuit breakers, rollback, protected nodes

mod actions;
mod context;
mod llm_client;
mod metrics;
mod observation;

pub use actions::{ControllerAction, ActionOutcome, MutationPolicy};
pub use context::{ControllerContext, SessionConstraints};
pub use llm_client::{LlmClient, LlmRequest, LlmResponse};
pub use metrics::ControllerMetrics;
pub use observation::{PipelineObservation, NodeMetrics, GraphMetrics, NodeStatus, Trend};

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use crate::Error;

/// Configuration for the LLM Pipeline Controller
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    /// How often to collect observations
    pub observation_interval: Duration,

    /// Minimum time between actions (prevents oscillation)
    pub action_cooldown: Duration,

    /// What mutations are allowed
    pub mutation_policy: MutationPolicy,

    /// Nodes that should never be modified
    pub protected_nodes: Vec<String>,

    /// Session constraints (latency SLOs, etc.)
    pub constraints: SessionConstraints,

    /// Maximum actions per time window (circuit breaker)
    pub max_actions_per_minute: u32,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        Self {
            observation_interval: Duration::from_millis(1000),
            action_cooldown: Duration::from_millis(5000),
            mutation_policy: MutationPolicy::Optimize,
            protected_nodes: vec!["input_source".into(), "output_sink".into()],
            constraints: SessionConstraints::default(),
            max_actions_per_minute: 10,
        }
    }
}

/// The main LLM Pipeline Controller
///
/// Monitors pipeline health and uses LLM reasoning to decide on
/// corrective actions when issues are detected.
pub struct LlmPipelineController<L: LlmClient, R: PipelineRouter> {
    /// LLM client for reasoning
    llm_client: L,

    /// Configuration
    config: ControllerConfig,

    /// Controller-specific metrics
    metrics: ControllerMetrics,

    /// History of observations
    observation_history: Vec<PipelineObservation>,

    /// History of actions taken
    action_history: Vec<ActionRecord>,

    /// Last time an action was taken
    last_action_time: Option<Instant>,

    /// Actions taken in current window (circuit breaker)
    actions_this_minute: u32,
    window_start: Instant,

    /// The router we're controlling
    router: Arc<RwLock<R>>,
}

/// Record of an action taken
#[derive(Debug, Clone)]
pub struct ActionRecord {
    pub timestamp: Instant,
    pub action: ControllerAction,
    pub outcome: ActionOutcome,
    pub llm_reasoning: String,
}

/// Trait for pipeline routers that can be controlled
///
/// This abstracts over SessionRouter to allow testing
pub trait PipelineRouter: Send + Sync {
    /// Get current observation of pipeline state
    fn get_observation(&self) -> PipelineObservation;

    /// Apply a controller action
    fn apply_action(&mut self, action: ControllerAction) -> impl std::future::Future<Output = Result<ActionOutcome, Error>> + Send;

    /// Get the node catalog (available node types)
    fn get_node_catalog(&self) -> NodeCatalog;
}

/// Catalog of available node types
#[derive(Debug, Clone, Default)]
pub struct NodeCatalog {
    pub nodes: Vec<NodeCatalogEntry>,
}

/// Entry in the node catalog
#[derive(Debug, Clone)]
pub struct NodeCatalogEntry {
    pub node_type: String,
    pub description: String,
    pub available_executors: Vec<String>,
    pub capabilities: Vec<String>,
}

impl<L: LlmClient, R: PipelineRouter> LlmPipelineController<L, R> {
    /// Create a new controller
    pub fn new(
        llm_client: L,
        router: Arc<RwLock<R>>,
        config: ControllerConfig,
    ) -> Self {
        Self {
            llm_client,
            config,
            metrics: ControllerMetrics::new(),
            observation_history: Vec::with_capacity(60), // ~1 minute of history
            action_history: Vec::new(),
            last_action_time: None,
            actions_this_minute: 0,
            window_start: Instant::now(),
            router,
        }
    }

    /// Main control loop - runs until cancelled
    pub async fn run(&mut self) -> Result<(), Error> {
        loop {
            // Reset circuit breaker window
            if self.window_start.elapsed() > Duration::from_secs(60) {
                self.actions_this_minute = 0;
                self.window_start = Instant::now();
            }

            // 1. OBSERVE: Collect metrics
            let observation = {
                let router = self.router.read().await;
                router.get_observation()
            };

            self.metrics.record_observation();
            self.observation_history.push(observation.clone());

            // Keep history bounded
            if self.observation_history.len() > 300 {
                self.observation_history.remove(0);
            }

            // 2. Quick heuristics: Does this need LLM attention?
            if !self.needs_attention(&observation) {
                tokio::time::sleep(self.config.observation_interval).await;
                continue;
            }

            // 3. Check cooldown
            if let Some(last) = self.last_action_time {
                if last.elapsed() < self.config.action_cooldown {
                    tokio::time::sleep(self.config.observation_interval).await;
                    continue;
                }
            }

            // 4. Check circuit breaker
            if self.actions_this_minute >= self.config.max_actions_per_minute {
                tracing::warn!(
                    "Controller circuit breaker: {} actions in last minute, skipping",
                    self.actions_this_minute
                );
                tokio::time::sleep(self.config.observation_interval).await;
                continue;
            }

            // 5. ORIENT + DECIDE: Ask LLM
            let context = self.build_context(&observation).await;
            let decision = match self.reason_about_action(&context).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("LLM reasoning failed: {}", e);
                    tokio::time::sleep(self.config.observation_interval).await;
                    continue;
                }
            };

            // 6. ACT: Apply the action (if not NoOp)
            if !matches!(decision.action, ControllerAction::NoOp { .. }) {
                // Validate action against policy
                if let Err(e) = decision.action.validate(
                    &self.config.mutation_policy,
                    &self.config.protected_nodes,
                ) {
                    tracing::warn!("Action rejected by policy: {}", e);
                    continue;
                }

                // Apply the action
                let outcome = {
                    let mut router = self.router.write().await;
                    router.apply_action(decision.action.clone()).await
                };

                let outcome = match outcome {
                    Ok(o) => o,
                    Err(e) => ActionOutcome::Failed {
                        error: e.to_string(),
                    },
                };

                self.action_history.push(ActionRecord {
                    timestamp: Instant::now(),
                    action: decision.action,
                    outcome: outcome.clone(),
                    llm_reasoning: decision.reasoning,
                });

                self.last_action_time = Some(Instant::now());
                self.actions_this_minute += 1;
                self.metrics.record_action(&outcome);
            }

            tokio::time::sleep(self.config.observation_interval).await;
        }
    }

    /// Quick heuristics to determine if LLM attention is needed
    fn needs_attention(&self, obs: &PipelineObservation) -> bool {
        // Check for obvious issues that need attention
        obs.node_metrics.values().any(|m| {
            // Node failures or degradation
            matches!(m.status, NodeStatus::Failed | NodeStatus::Degraded)
            // High error rate
            || m.error_rate > 0.01
            // Latency trending up
            || matches!(m.latency_trend, Trend::Rising)
            // Queue pressure
            || (m.queue_capacity > 0 && m.queue_depth as f64 / m.queue_capacity as f64 > 0.8)
        })
        // Or constraint violations
        || self.config.constraints.max_latency_ms
            .map(|max| obs.graph_metrics.end_to_end_latency_ms > max)
            .unwrap_or(false)
        || self.config.constraints.min_throughput
            .map(|min| obs.graph_metrics.total_throughput < min)
            .unwrap_or(false)
    }

    /// Build context for LLM
    async fn build_context(&self, observation: &PipelineObservation) -> ControllerContext {
        let router = self.router.read().await;
        let node_catalog = router.get_node_catalog();

        ControllerContext {
            observation: observation.clone(),
            history: self.observation_history.clone(),
            action_history: self.action_history.clone(),
            node_catalog,
            constraints: self.config.constraints.clone(),
            available_actions: ControllerAction::available_actions(),
        }
    }

    /// Ask LLM to reason about what action to take
    async fn reason_about_action(
        &self,
        context: &ControllerContext,
    ) -> Result<ControllerDecision, Error> {
        let prompt = context.render_prompt();

        let start = Instant::now();
        let response = self.llm_client
            .complete(LlmRequest {
                system: CONTROLLER_SYSTEM_PROMPT.to_string(),
                user: prompt,
                temperature: 0.1,
                max_tokens: 1000,
            })
            .await?;

        self.metrics.record_llm_call(start.elapsed());

        // Parse JSON response
        let decision: ControllerDecision = serde_json::from_str(&response.text)
            .map_err(|e| Error::Execution(format!("Failed to parse LLM response: {}", e)))?;

        tracing::info!(
            reasoning = %decision.reasoning,
            action = ?decision.action,
            "Controller decision"
        );

        Ok(decision)
    }
}

/// Decision from LLM
#[derive(Debug, serde::Deserialize)]
pub struct ControllerDecision {
    pub reasoning: String,
    pub issues: Vec<String>,
    pub action: ControllerAction,
}

const CONTROLLER_SYSTEM_PROMPT: &str = r#"
You are a Pipeline Controller for a real-time media processing system.

Your role is to monitor pipeline health and performance, then decide on
corrective actions when needed. You should:

1. OBSERVE: Analyze the provided metrics and identify issues
2. REASON: Determine root causes and potential fixes
3. DECIDE: Choose the minimal action to resolve the issue
4. EXPLAIN: Provide clear reasoning for your decision

Guidelines:
- Prefer minimal changes (NoOp if pipeline is healthy)
- Consider latency impact of any changes
- Avoid oscillation (don't undo recent actions)
- Prioritize stability over optimization
- Only take drastic actions (RemoveNode, ReplaceNode) for clear failures

You must respond with a JSON object containing:
{
  "reasoning": "Brief analysis of current state and decision rationale",
  "issues": ["List of identified issues"],
  "action": { <ControllerAction object> }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Add tests with mock LLM client and router
}
