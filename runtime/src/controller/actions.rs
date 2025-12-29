//! Controller actions that can be applied to pipelines

use serde::{Deserialize, Serialize};

use crate::Error;

/// Actions the LLM controller can take to modify the pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action_type")]
pub enum ControllerAction {
    /// No action needed - pipeline is healthy
    NoOp {
        reason: String,
    },

    /// Replace a node with an alternative implementation
    ReplaceNode {
        node_id: String,
        replacement_type: String,
        reason: String,
    },

    /// Insert a new node into the graph
    InsertNode {
        after_node: String,
        new_node_type: String,
        config: serde_json::Value,
        reason: String,
    },

    /// Remove a node from the graph
    RemoveNode {
        node_id: String,
        reason: String,
    },

    /// Modify node configuration
    UpdateNodeConfig {
        node_id: String,
        config_updates: serde_json::Value,
        reason: String,
    },

    /// Bypass a node (route around it)
    BypassNode {
        node_id: String,
        reason: String,
    },

    /// Un-bypass a previously bypassed node
    RestoreNode {
        node_id: String,
        reason: String,
    },

    /// Switch executor type (e.g., Python -> Rust native)
    SwitchExecutor {
        node_id: String,
        new_executor: String,
        reason: String,
    },

    /// Restart a failed process
    RestartProcess {
        node_id: String,
        reason: String,
    },

    /// Scale a node (add parallel instances)
    ScaleNode {
        node_id: String,
        replicas: usize,
        reason: String,
    },

    /// Adjust buffer/queue sizes
    AdjustBuffering {
        node_id: String,
        queue_size: usize,
        reason: String,
    },
}

impl ControllerAction {
    /// Get the target node of this action, if any
    pub fn target_node(&self) -> Option<&str> {
        match self {
            ControllerAction::NoOp { .. } => None,
            ControllerAction::ReplaceNode { node_id, .. } => Some(node_id),
            ControllerAction::InsertNode { after_node, .. } => Some(after_node),
            ControllerAction::RemoveNode { node_id, .. } => Some(node_id),
            ControllerAction::UpdateNodeConfig { node_id, .. } => Some(node_id),
            ControllerAction::BypassNode { node_id, .. } => Some(node_id),
            ControllerAction::RestoreNode { node_id, .. } => Some(node_id),
            ControllerAction::SwitchExecutor { node_id, .. } => Some(node_id),
            ControllerAction::RestartProcess { node_id, .. } => Some(node_id),
            ControllerAction::ScaleNode { node_id, .. } => Some(node_id),
            ControllerAction::AdjustBuffering { node_id, .. } => Some(node_id),
        }
    }

    /// Validate this action against policy and protected nodes
    pub fn validate(
        &self,
        policy: &MutationPolicy,
        protected_nodes: &[String],
    ) -> Result<(), Error> {
        // Check if action is allowed by policy
        let required_level = self.required_mutation_level();
        if !policy.allows(required_level) {
            return Err(Error::Execution(format!(
                "Action {:?} requires {:?} policy, but current policy is {:?}",
                self.action_type_name(),
                required_level,
                policy
            )));
        }

        // Check if target node is protected
        if let Some(node_id) = self.target_node() {
            if protected_nodes.iter().any(|p| p == node_id) {
                return Err(Error::Execution(format!(
                    "Cannot modify protected node: {}",
                    node_id
                )));
            }
        }

        Ok(())
    }

    /// Get the minimum mutation policy required for this action
    fn required_mutation_level(&self) -> MutationPolicy {
        match self {
            ControllerAction::NoOp { .. } => MutationPolicy::ReadOnly,
            ControllerAction::RestartProcess { .. } => MutationPolicy::Heal,
            ControllerAction::RestoreNode { .. } => MutationPolicy::Heal,
            ControllerAction::SwitchExecutor { .. } => MutationPolicy::Optimize,
            ControllerAction::AdjustBuffering { .. } => MutationPolicy::Optimize,
            ControllerAction::UpdateNodeConfig { .. } => MutationPolicy::Optimize,
            ControllerAction::BypassNode { .. } => MutationPolicy::Optimize,
            ControllerAction::ReplaceNode { .. } => MutationPolicy::Full,
            ControllerAction::InsertNode { .. } => MutationPolicy::Full,
            ControllerAction::RemoveNode { .. } => MutationPolicy::Full,
            ControllerAction::ScaleNode { .. } => MutationPolicy::Full,
        }
    }

    fn action_type_name(&self) -> &'static str {
        match self {
            ControllerAction::NoOp { .. } => "NoOp",
            ControllerAction::ReplaceNode { .. } => "ReplaceNode",
            ControllerAction::InsertNode { .. } => "InsertNode",
            ControllerAction::RemoveNode { .. } => "RemoveNode",
            ControllerAction::UpdateNodeConfig { .. } => "UpdateNodeConfig",
            ControllerAction::BypassNode { .. } => "BypassNode",
            ControllerAction::RestoreNode { .. } => "RestoreNode",
            ControllerAction::SwitchExecutor { .. } => "SwitchExecutor",
            ControllerAction::RestartProcess { .. } => "RestartProcess",
            ControllerAction::ScaleNode { .. } => "ScaleNode",
            ControllerAction::AdjustBuffering { .. } => "AdjustBuffering",
        }
    }

    /// List of available actions for LLM context
    pub fn available_actions() -> Vec<ActionSchema> {
        vec![
            ActionSchema {
                name: "NoOp".to_string(),
                description: "Take no action - pipeline is healthy".to_string(),
                parameters: vec!["reason: string".to_string()],
            },
            ActionSchema {
                name: "RestartProcess".to_string(),
                description: "Restart a failed Python process".to_string(),
                parameters: vec!["node_id: string".to_string(), "reason: string".to_string()],
            },
            ActionSchema {
                name: "BypassNode".to_string(),
                description: "Route data around a failing non-critical node".to_string(),
                parameters: vec!["node_id: string".to_string(), "reason: string".to_string()],
            },
            ActionSchema {
                name: "RestoreNode".to_string(),
                description: "Un-bypass a previously bypassed node".to_string(),
                parameters: vec!["node_id: string".to_string(), "reason: string".to_string()],
            },
            ActionSchema {
                name: "SwitchExecutor".to_string(),
                description: "Switch node executor (e.g., Python to Rust native)".to_string(),
                parameters: vec![
                    "node_id: string".to_string(),
                    "new_executor: Native | Multiprocess | Wasm".to_string(),
                    "reason: string".to_string(),
                ],
            },
            ActionSchema {
                name: "AdjustBuffering".to_string(),
                description: "Modify queue/buffer size for a node".to_string(),
                parameters: vec![
                    "node_id: string".to_string(),
                    "queue_size: number".to_string(),
                    "reason: string".to_string(),
                ],
            },
            ActionSchema {
                name: "UpdateNodeConfig".to_string(),
                description: "Update node configuration parameters".to_string(),
                parameters: vec![
                    "node_id: string".to_string(),
                    "config_updates: object".to_string(),
                    "reason: string".to_string(),
                ],
            },
            ActionSchema {
                name: "ReplaceNode".to_string(),
                description: "Replace a node with a different implementation".to_string(),
                parameters: vec![
                    "node_id: string".to_string(),
                    "replacement_type: string".to_string(),
                    "reason: string".to_string(),
                ],
            },
            ActionSchema {
                name: "ScaleNode".to_string(),
                description: "Add parallel instances of a node for throughput".to_string(),
                parameters: vec![
                    "node_id: string".to_string(),
                    "replicas: number".to_string(),
                    "reason: string".to_string(),
                ],
            },
        ]
    }
}

/// Schema description for an action (for LLM context)
#[derive(Debug, Clone, Serialize)]
pub struct ActionSchema {
    pub name: String,
    pub description: String,
    pub parameters: Vec<String>,
}

/// What mutations the controller is allowed to make
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MutationPolicy {
    /// Can only observe, no mutations
    ReadOnly,
    /// Can restart processes and restore bypassed nodes
    Heal,
    /// Can switch executors, adjust buffering, update config
    Optimize,
    /// Full control: can add/remove/replace nodes
    Full,
}

impl MutationPolicy {
    /// Check if this policy allows a given level
    pub fn allows(&self, required: MutationPolicy) -> bool {
        match (self, required) {
            (_, MutationPolicy::ReadOnly) => true,
            (MutationPolicy::ReadOnly, _) => false,
            (_, MutationPolicy::Heal) => true,
            (MutationPolicy::Heal, _) => false,
            (_, MutationPolicy::Optimize) => true,
            (MutationPolicy::Optimize, MutationPolicy::Full) => false,
            (MutationPolicy::Full, MutationPolicy::Full) => true,
        }
    }
}

/// Outcome of applying an action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionOutcome {
    /// Action succeeded
    Success {
        /// Measured latency impact in ms (positive = slower, negative = faster)
        latency_impact_ms: Option<f64>,
        /// Additional details
        details: Option<String>,
    },
    /// Action failed
    Failed {
        error: String,
    },
    /// Action was rejected (policy, protected node, etc.)
    Rejected {
        reason: String,
    },
}

impl ActionOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, ActionOutcome::Success { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mutation_policy_allows() {
        assert!(MutationPolicy::Full.allows(MutationPolicy::Full));
        assert!(MutationPolicy::Full.allows(MutationPolicy::Optimize));
        assert!(MutationPolicy::Full.allows(MutationPolicy::Heal));
        assert!(MutationPolicy::Full.allows(MutationPolicy::ReadOnly));

        assert!(!MutationPolicy::Optimize.allows(MutationPolicy::Full));
        assert!(MutationPolicy::Optimize.allows(MutationPolicy::Optimize));
        assert!(MutationPolicy::Optimize.allows(MutationPolicy::Heal));

        assert!(!MutationPolicy::Heal.allows(MutationPolicy::Optimize));
        assert!(MutationPolicy::Heal.allows(MutationPolicy::Heal));

        assert!(!MutationPolicy::ReadOnly.allows(MutationPolicy::Heal));
        assert!(MutationPolicy::ReadOnly.allows(MutationPolicy::ReadOnly));
    }

    #[test]
    fn test_action_validation_protected_node() {
        let action = ControllerAction::RestartProcess {
            node_id: "input_source".to_string(),
            reason: "test".to_string(),
        };

        let result = action.validate(
            &MutationPolicy::Full,
            &["input_source".to_string()],
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_action_validation_policy() {
        let action = ControllerAction::RemoveNode {
            node_id: "some_node".to_string(),
            reason: "test".to_string(),
        };

        // RemoveNode requires Full policy
        assert!(action.validate(&MutationPolicy::Full, &[]).is_ok());
        assert!(action.validate(&MutationPolicy::Optimize, &[]).is_err());
    }
}
