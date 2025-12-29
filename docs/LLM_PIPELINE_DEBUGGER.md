# LLM Pipeline Debugger Architecture

## Overview

The **LLM Pipeline Debugger** (also called **Cognitive Pipeline Controller**) is an autonomous control plane component that uses LLM reasoning to:

1. **Auto-heal** broken pipelines by detecting failures and applying fixes
2. **Optimize** graph structure based on runtime latency/throughput metrics
3. **Adapt** to changing conditions (load, resource availability, error patterns)

## Core Concepts

### OODA Control Loop

The system implements an **Observe-Orient-Decide-Act** loop:

```
┌─────────────────────────────────────────────────────────────┐
│                    OODA Control Loop                         │
│                                                              │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐ │
│  │ OBSERVE  │──▶│  ORIENT  │──▶│  DECIDE  │──▶│   ACT    │ │
│  │          │   │          │   │          │   │          │ │
│  │ Collect  │   │ Build    │   │ LLM      │   │ Apply    │ │
│  │ Metrics  │   │ Context  │   │ Reason   │   │ Changes  │ │
│  └──────────┘   └──────────┘   └──────────┘   └──────────┘ │
│       ▲                                            │        │
│       └────────────────────────────────────────────┘        │
│                      Continuous Loop                         │
└─────────────────────────────────────────────────────────────┘
```

### Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Control Plane (LLM)                       │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │ Observation │  │  Reasoning  │  │   Action Executor   │ │
│  │   Buffer    │  │   Engine    │  │                     │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                 Observation Layer (Metrics)                  │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌─────────────┐ │
│  │  Latency  │ │   Error   │ │  Health   │ │   Queue     │ │
│  │  Metrics  │ │   Rates   │ │  Status   │ │   Depths    │ │
│  └───────────┘ └───────────┘ └───────────┘ └─────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                    Data Plane (Pipeline)                     │
├─────────────────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────────────────┐  │
│  │                   Session Router                       │  │
│  │  ┌──────┐   ┌──────┐   ┌──────┐   ┌──────┐          │  │
│  │  │Node 1│──▶│Node 2│──▶│Node 3│──▶│Node 4│          │  │
│  │  └──────┘   └──────┘   └──────┘   └──────┘          │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Data Structures

### Observation Event

```rust
/// A snapshot of pipeline state at a point in time
#[derive(Debug, Clone, Serialize)]
pub struct PipelineObservation {
    pub timestamp: Instant,
    pub session_id: String,

    /// Per-node metrics
    pub node_metrics: HashMap<NodeId, NodeMetrics>,

    /// Graph-level metrics
    pub graph_metrics: GraphMetrics,

    /// Recent errors (sliding window)
    pub recent_errors: Vec<PipelineError>,

    /// Current graph topology
    pub topology: GraphTopology,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeMetrics {
    pub node_id: NodeId,
    pub node_type: String,
    pub executor: ExecutorType,  // Native, Multiprocess, WASM

    // Latency (exponential moving average)
    pub latency_p50_ms: f64,
    pub latency_p99_ms: f64,
    pub latency_trend: Trend,  // Rising, Falling, Stable

    // Throughput
    pub items_per_second: f64,
    pub queue_depth: usize,
    pub queue_capacity: usize,

    // Health
    pub error_rate: f64,  // errors per second
    pub last_error: Option<String>,
    pub status: NodeStatus,  // Healthy, Degraded, Failed

    // Resources (for multiprocess nodes)
    pub memory_mb: Option<f64>,
    pub cpu_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphMetrics {
    pub end_to_end_latency_ms: f64,
    pub total_throughput: f64,
    pub bottleneck_node: Option<NodeId>,
    pub critical_path: Vec<NodeId>,
}
```

### LLM Context

```rust
/// Context provided to the LLM for reasoning
#[derive(Debug, Serialize)]
pub struct ControllerContext {
    /// Current observation
    pub observation: PipelineObservation,

    /// Historical observations (last N minutes)
    pub history: Vec<PipelineObservation>,

    /// Available actions the LLM can take
    pub available_actions: Vec<ActionSchema>,

    /// Node catalog (what nodes exist and their capabilities)
    pub node_catalog: NodeCatalog,

    /// Previous actions and their outcomes
    pub action_history: Vec<ActionOutcome>,

    /// Session constraints (latency SLOs, etc.)
    pub constraints: SessionConstraints,
}

#[derive(Debug, Serialize)]
pub struct SessionConstraints {
    pub max_latency_ms: Option<f64>,
    pub min_throughput: Option<f64>,
    pub allowed_mutations: MutationPolicy,  // ReadOnly, Optimize, Heal, Full
}
```

### Actions

```rust
/// Actions the LLM can request
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action_type")]
pub enum ControllerAction {
    /// No action needed
    NoOp { reason: String },

    /// Replace a node with an alternative implementation
    ReplaceNode {
        node_id: NodeId,
        replacement_type: String,
        reason: String,
    },

    /// Insert a new node into the graph
    InsertNode {
        after_node: NodeId,
        new_node_type: String,
        config: serde_json::Value,
        reason: String,
    },

    /// Remove a node from the graph
    RemoveNode {
        node_id: NodeId,
        reason: String,
    },

    /// Modify node configuration
    UpdateNodeConfig {
        node_id: NodeId,
        config_updates: serde_json::Value,
        reason: String,
    },

    /// Reroute data flow (bypass a node)
    BypassNode {
        node_id: NodeId,
        reason: String,
    },

    /// Switch executor type (e.g., Python -> Rust)
    SwitchExecutor {
        node_id: NodeId,
        new_executor: ExecutorType,
        reason: String,
    },

    /// Restart a failed process
    RestartProcess {
        node_id: NodeId,
        reason: String,
    },

    /// Add parallel instances for throughput
    ScaleNode {
        node_id: NodeId,
        replicas: usize,
        reason: String,
    },

    /// Adjust buffer/queue sizes
    AdjustBuffering {
        node_id: NodeId,
        queue_size: usize,
        reason: String,
    },
}
```

## LLM Prompt Structure

### System Prompt

```
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
  "analysis": "Brief analysis of current state",
  "issues": ["List of identified issues"],
  "action": { <ControllerAction object> }
}
```

### User Prompt Template

```
## Current Pipeline State

Session: {{session_id}}
Uptime: {{uptime}}
Constraints: max_latency={{max_latency_ms}}ms, mutation_policy={{mutation_policy}}

## Node Metrics

| Node | Type | Executor | P50 Latency | P99 Latency | Trend | Queue | Errors/s | Status |
|------|------|----------|-------------|-------------|-------|-------|----------|--------|
{{#each node_metrics}}
| {{node_id}} | {{node_type}} | {{executor}} | {{latency_p50_ms}}ms | {{latency_p99_ms}}ms | {{latency_trend}} | {{queue_depth}}/{{queue_capacity}} | {{error_rate}} | {{status}} |
{{/each}}

## Graph Metrics

- End-to-end latency: {{end_to_end_latency_ms}}ms
- Throughput: {{total_throughput}} items/sec
- Bottleneck: {{bottleneck_node}}
- Critical path: {{critical_path}}

## Recent Errors (last 60s)

{{#each recent_errors}}
- [{{timestamp}}] {{node_id}}: {{error_message}}
{{/each}}

## Recent Actions (last 5 minutes)

{{#each action_history}}
- [{{timestamp}}] {{action_type}}: {{reason}} → {{outcome}}
{{/each}}

## Available Actions

{{#each available_actions}}
- {{name}}: {{description}}
{{/each}}

## Node Catalog (available replacements)

{{#each node_catalog}}
- {{node_type}}: {{capabilities}} (executors: {{available_executors}})
{{/each}}

---

Analyze the pipeline state and decide on an action.
```

## Integration Points

### 1. Session Router Integration

The controller hooks into `SessionRouter` to observe and mutate:

```rust
// runtime/src/grpc_service/session_router.rs

impl SessionRouter {
    /// Called by controller to get current metrics
    pub fn get_observation(&self) -> PipelineObservation {
        PipelineObservation {
            timestamp: Instant::now(),
            session_id: self.session_id.clone(),
            node_metrics: self.collect_node_metrics(),
            graph_metrics: self.compute_graph_metrics(),
            recent_errors: self.error_buffer.clone(),
            topology: self.get_topology(),
        }
    }

    /// Called by controller to apply a mutation
    pub async fn apply_action(&mut self, action: ControllerAction) -> Result<ActionOutcome> {
        match action {
            ControllerAction::ReplaceNode { node_id, replacement_type, .. } => {
                self.hot_swap_node(node_id, replacement_type).await
            }
            ControllerAction::BypassNode { node_id, .. } => {
                self.bypass_node(node_id).await
            }
            ControllerAction::RestartProcess { node_id, .. } => {
                self.restart_node_process(node_id).await
            }
            // ... other actions
        }
    }

    /// Hot-swap a node without stopping the pipeline
    async fn hot_swap_node(&mut self, node_id: NodeId, new_type: String) -> Result<ActionOutcome> {
        // 1. Create new node instance
        let new_node = self.registry.create_node(&new_type, config)?;

        // 2. Drain in-flight data from old node
        self.drain_node(&node_id).await?;

        // 3. Atomically swap node references
        self.nodes.insert(node_id.clone(), new_node);

        // 4. Resume data flow
        self.resume_node(&node_id).await?;

        Ok(ActionOutcome::Success {
            latency_impact_ms: measured_impact
        })
    }
}
```

### 2. Metrics Collection

Extend existing metrics infrastructure:

```rust
// runtime/src/executor/metrics.rs

/// Extended metrics for controller
pub struct ControllerMetrics {
    /// Per-node latency histograms
    node_latencies: HashMap<NodeId, Histogram>,

    /// Per-node error counts
    node_errors: HashMap<NodeId, Counter>,

    /// Queue depth gauges
    queue_depths: HashMap<NodeId, Gauge>,

    /// Sliding window of recent observations
    observation_buffer: CircularBuffer<PipelineObservation>,
}

impl ControllerMetrics {
    /// Record a node processing event
    pub fn record_node_processing(&self, node_id: &NodeId, duration: Duration, success: bool) {
        self.node_latencies.get(node_id).map(|h| h.record(duration));
        if !success {
            self.node_errors.get(node_id).map(|c| c.increment());
        }
    }

    /// Get current observation snapshot
    pub fn snapshot(&self) -> PipelineObservation {
        // Compute percentiles, trends, etc.
    }
}
```

### 3. Health Monitor Integration

Leverage existing health monitoring:

```rust
// runtime/src/python/multiprocess/health_monitor.rs

impl HealthMonitor {
    /// Export health data for controller
    pub fn get_node_health(&self, node_id: &NodeId) -> NodeHealthStatus {
        NodeHealthStatus {
            status: self.get_status(node_id),
            memory_mb: self.get_memory_usage(node_id),
            cpu_percent: self.get_cpu_usage(node_id),
            last_heartbeat: self.get_last_heartbeat(node_id),
            restart_count: self.get_restart_count(node_id),
        }
    }
}
```

## Controller Implementation

```rust
// runtime/src/controller/llm_controller.rs

pub struct LlmPipelineController {
    /// LLM client (Claude, etc.)
    llm_client: Box<dyn LlmClient>,

    /// Observation collection interval
    observation_interval: Duration,

    /// Minimum time between actions (prevent oscillation)
    action_cooldown: Duration,

    /// History of observations and actions
    history: ControllerHistory,

    /// Node catalog for replacement options
    node_catalog: NodeCatalog,

    /// Session constraints
    constraints: SessionConstraints,
}

impl LlmPipelineController {
    /// Main control loop
    pub async fn run(&mut self, router: Arc<RwLock<SessionRouter>>) {
        let mut last_action_time = Instant::now();

        loop {
            // 1. OBSERVE: Collect metrics
            let observation = {
                let router = router.read().await;
                router.get_observation()
            };

            // 2. Check if action is needed (quick heuristics first)
            if !self.needs_attention(&observation) {
                self.history.record_observation(observation);
                tokio::time::sleep(self.observation_interval).await;
                continue;
            }

            // 3. Check cooldown
            if last_action_time.elapsed() < self.action_cooldown {
                tokio::time::sleep(self.observation_interval).await;
                continue;
            }

            // 4. ORIENT + DECIDE: Ask LLM
            let context = self.build_context(observation);
            let action = self.reason_about_action(context).await?;

            // 5. ACT: Apply the action
            if !matches!(action, ControllerAction::NoOp { .. }) {
                let outcome = {
                    let mut router = router.write().await;
                    router.apply_action(action.clone()).await
                };

                self.history.record_action(action, outcome);
                last_action_time = Instant::now();
            }

            tokio::time::sleep(self.observation_interval).await;
        }
    }

    /// Quick heuristics to avoid LLM call when everything is fine
    fn needs_attention(&self, obs: &PipelineObservation) -> bool {
        // Check for obvious issues
        obs.node_metrics.values().any(|m| {
            m.status == NodeStatus::Failed ||
            m.status == NodeStatus::Degraded ||
            m.error_rate > 0.01 ||
            m.latency_trend == Trend::Rising ||
            m.queue_depth as f64 / m.queue_capacity as f64 > 0.8
        }) ||
        // Check constraints
        self.constraints.max_latency_ms
            .map(|max| obs.graph_metrics.end_to_end_latency_ms > max)
            .unwrap_or(false)
    }

    /// Build context and call LLM
    async fn reason_about_action(&self, context: ControllerContext) -> Result<ControllerAction> {
        let prompt = self.render_prompt(&context);

        let response = self.llm_client
            .complete(LlmRequest {
                system: CONTROLLER_SYSTEM_PROMPT,
                user: prompt,
                temperature: 0.1,  // Low temperature for consistency
                max_tokens: 1000,
            })
            .await?;

        // Parse JSON response
        let decision: ControllerDecision = serde_json::from_str(&response.text)?;

        log::info!(
            "Controller decision: {} -> {:?}",
            decision.analysis,
            decision.action
        );

        Ok(decision.action)
    }
}
```

## Use Cases

### 1. Auto-Healing: Process Crash Recovery

```
Observation:
- Node "whisper_transcriber" status: Failed
- Error: "Process exited with code 137 (OOM)"
- Queue depth: 47/50 (backed up)

LLM Analysis:
"The whisper_transcriber node has crashed due to OOM. The queue is
backing up, which will cause latency issues. The process should be
restarted, and we should consider switching to a smaller model variant
if this recurs."

Action:
{
  "action_type": "RestartProcess",
  "node_id": "whisper_transcriber",
  "reason": "Process crashed with OOM, restarting to restore pipeline"
}
```

### 2. Latency Optimization: Executor Switch

```
Observation:
- Node "audio_resampler" latency: P50=45ms, P99=120ms (Rising trend)
- Node type: AudioResampler, Executor: Multiprocess (Python)
- Constraint: max_latency=100ms

LLM Analysis:
"The audio_resampler node is causing latency violations. This node has
a native Rust implementation available which is 10x faster. Since audio
quality is preserved, switching executors is safe."

Action:
{
  "action_type": "SwitchExecutor",
  "node_id": "audio_resampler",
  "new_executor": "Native",
  "reason": "Python resampler exceeding latency SLO, switching to native Rust (10x faster)"
}
```

### 3. Adaptive Buffering: Queue Pressure

```
Observation:
- Node "llm_processor" queue: 48/50 (96% full)
- Upstream node throughput: 100 items/sec
- LLM processor throughput: 80 items/sec
- No errors

LLM Analysis:
"The llm_processor is slightly slower than upstream, causing queue
buildup. This is not a failure, but will eventually cause drops.
Increasing buffer size provides headroom while maintaining latency."

Action:
{
  "action_type": "AdjustBuffering",
  "node_id": "llm_processor",
  "queue_size": 100,
  "reason": "Queue near capacity due to throughput mismatch, increasing buffer"
}
```

### 4. Graceful Degradation: Bypass Failing Node

```
Observation:
- Node "sentiment_analyzer" error_rate: 0.85 (85% failing)
- Error: "API rate limit exceeded"
- Node is non-critical (enrichment only)

LLM Analysis:
"The sentiment_analyzer is failing due to rate limits. This node
provides optional enrichment and is not on the critical path.
Bypassing it will restore pipeline health while preserving core
functionality."

Action:
{
  "action_type": "BypassNode",
  "node_id": "sentiment_analyzer",
  "reason": "Non-critical node hitting rate limits, bypassing to restore pipeline"
}
```

## Configuration

```yaml
# pipeline_manifest.yaml
controller:
  enabled: true

  # How often to collect observations
  observation_interval_ms: 1000

  # Minimum time between actions (prevent oscillation)
  action_cooldown_ms: 5000

  # LLM configuration
  llm:
    provider: anthropic
    model: claude-sonnet-4-20250514
    temperature: 0.1
    max_tokens: 1000

  # What the controller is allowed to do
  mutation_policy: optimize  # readonly | heal | optimize | full

  # Constraints to enforce
  constraints:
    max_latency_ms: 100
    min_throughput: 50

  # Nodes that should never be modified
  protected_nodes:
    - input_source
    - output_sink
```

## Safety Guardrails

### 1. Action Validation

```rust
impl ControllerAction {
    /// Validate action before execution
    pub fn validate(&self, policy: &MutationPolicy, protected: &[NodeId]) -> Result<()> {
        // Check mutation policy allows this action type
        match (self, policy) {
            (ControllerAction::NoOp { .. }, _) => Ok(()),
            (ControllerAction::RestartProcess { .. }, MutationPolicy::Heal) => Ok(()),
            (ControllerAction::ReplaceNode { .. }, MutationPolicy::ReadOnly) => {
                Err(Error::PolicyViolation("ReplaceNode not allowed in readonly mode"))
            }
            // ...
        }?;

        // Check node is not protected
        if let Some(node_id) = self.target_node() {
            if protected.contains(node_id) {
                return Err(Error::ProtectedNode(node_id.clone()));
            }
        }

        Ok(())
    }
}
```

### 2. Rollback Support

```rust
/// Track actions for potential rollback
pub struct ActionJournal {
    actions: Vec<JournaledAction>,
}

pub struct JournaledAction {
    action: ControllerAction,
    timestamp: Instant,
    previous_state: NodeSnapshot,  // For rollback
    outcome: ActionOutcome,
}

impl ActionJournal {
    /// Rollback last N actions
    pub async fn rollback(&self, router: &mut SessionRouter, n: usize) -> Result<()> {
        for action in self.actions.iter().rev().take(n) {
            let reverse = action.compute_reverse();
            router.apply_action(reverse).await?;
        }
        Ok(())
    }
}
```

### 3. Circuit Breaker

```rust
/// Prevent runaway control loops
pub struct ControllerCircuitBreaker {
    action_count: AtomicU32,
    window_start: Instant,
    max_actions_per_window: u32,
    window_duration: Duration,
}

impl ControllerCircuitBreaker {
    pub fn allow_action(&self) -> bool {
        if self.window_start.elapsed() > self.window_duration {
            self.reset();
        }
        self.action_count.load(Ordering::Relaxed) < self.max_actions_per_window
    }
}
```

## Metrics & Observability

The controller itself emits metrics:

```rust
// Controller-specific metrics
controller_observations_total: Counter,       // Total observations collected
controller_llm_calls_total: Counter,          // LLM invocations
controller_llm_latency_ms: Histogram,         // LLM response time
controller_actions_total: Counter,            // Actions taken (by type)
controller_action_success_rate: Gauge,        // Success rate of actions
controller_action_latency_impact_ms: Histogram, // Measured latency impact
```

## Future Extensions

1. **Multi-Session Learning**: Share learnings across sessions
2. **Predictive Scaling**: Anticipate load based on patterns
3. **A/B Testing**: Test different node implementations
4. **Cost Optimization**: Consider resource costs in decisions
5. **Federated Control**: Coordinate across distributed pipelines
