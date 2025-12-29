//! Pipeline Capability Resolver (spec 023)
//!
//! This module provides the `CapabilityResolver` which automatically resolves,
//! propagates, and validates media capabilities during pipeline construction.
//!
//! # Resolution Algorithm
//!
//! The resolver uses a two-pass algorithm:
//!
//! 1. **Forward Pass** (topological order):
//!    - Static nodes: Use `media_capabilities()` directly
//!    - Configured nodes: Use factory's `media_capabilities(params)`
//!    - Passthrough nodes: Inherit output from upstream
//!    - RuntimeDiscovered nodes: Use `potential_capabilities()` (provisional)
//!    - Adaptive nodes: Mark as `NeedsReverse`
//!
//! 2. **Reverse Pass** (reverse topological order):
//!    - Adaptive nodes: Set output to match downstream input requirements
//!
//! # Two-Phase Resolution (RuntimeDiscovered)
//!
//! For nodes with `RuntimeDiscovered` behavior:
//! - **Phase 1**: Use `potential_capabilities()` for early validation
//! - **Phase 2**: After `node.initialize()`, call `revalidate()` with actual capabilities
//!
//! # Example
//!
//! ```ignore
//! let resolver = CapabilityResolver::new(&registry);
//! let ctx = resolver.resolve(&graph, &params)?;
//!
//! // Check for errors
//! if ctx.has_errors() {
//!     for error in &ctx.errors {
//!         eprintln!("Mismatch: {}", error);
//!     }
//! }
//!
//! // Query resolved capabilities
//! if let Some(caps) = ctx.resolved.get("whisper") {
//!     println!("Whisper input: {:?}", caps.capabilities.default_input());
//! }
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::constraints::MediaCapabilities;
use super::dynamic::{CapabilityBehavior, ResolutionContext, ResolutionState, ResolvedCapabilities};
use crate::nodes::streaming_node::StreamingNodeRegistry;
use crate::Error;

// =============================================================================
// CapabilityHints (spec 023 - US6)
// =============================================================================

/// Hints for guiding capability resolution when multiple valid values exist.
///
/// Users can provide hints in the manifest to resolve ambiguity during
/// capability negotiation. For example, when an Adaptive node could output
/// multiple sample rates that all satisfy the downstream requirements, hints
/// can specify a preferred value.
///
/// # Example (in manifest YAML)
///
/// ```yaml
/// capability_hints:
///   resample:
///     preferred_sample_rate: 16000
///     preferred_channels: 1
///   mic:
///     prefer_exact: true  # Prefer exact constraints over ranges
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityHints {
    /// Per-node hints (node_id -> hint configuration)
    #[serde(default)]
    pub nodes: HashMap<String, NodeHints>,

    /// Global preference: prefer lower sample rates when multiple are valid
    #[serde(default)]
    pub prefer_lower_sample_rate: bool,

    /// Global preference: prefer lower channel counts when multiple are valid
    #[serde(default)]
    pub prefer_lower_channels: bool,

    /// Global preference: prefer exact constraints over ranges
    #[serde(default)]
    pub prefer_exact: bool,
}

/// Hints for a specific node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeHints {
    /// Preferred sample rate when multiple are valid
    #[serde(default)]
    pub preferred_sample_rate: Option<u32>,

    /// Preferred channel count when multiple are valid
    #[serde(default)]
    pub preferred_channels: Option<u32>,

    /// Preferred audio format (e.g., "f32", "i16")
    #[serde(default)]
    pub preferred_format: Option<String>,

    /// Whether to prefer exact constraints for this node
    #[serde(default)]
    pub prefer_exact: Option<bool>,
}

impl CapabilityHints {
    /// Create empty hints (no preferences).
    pub fn new() -> Self {
        Self::default()
    }

    /// Get hints for a specific node.
    pub fn get_node_hints(&self, node_id: &str) -> Option<&NodeHints> {
        self.nodes.get(node_id)
    }

    /// Get preferred sample rate for a node (node-specific or global).
    pub fn preferred_sample_rate(&self, node_id: &str) -> Option<u32> {
        self.nodes
            .get(node_id)
            .and_then(|h| h.preferred_sample_rate)
    }

    /// Get preferred channel count for a node (node-specific or global).
    pub fn preferred_channels(&self, node_id: &str) -> Option<u32> {
        self.nodes.get(node_id).and_then(|h| h.preferred_channels)
    }

    /// Check if exact constraints are preferred for a node.
    pub fn prefers_exact(&self, node_id: &str) -> bool {
        self.nodes
            .get(node_id)
            .and_then(|h| h.prefer_exact)
            .unwrap_or(self.prefer_exact)
    }
}

// =============================================================================
// CapabilityResolver
// =============================================================================

/// Resolves media capabilities for pipeline nodes.
///
/// The resolver queries the `StreamingNodeRegistry` to get capability information
/// from node factories, then applies the resolution algorithm to determine
/// final capabilities for each node in the pipeline.
pub struct CapabilityResolver<'a> {
    /// Reference to the streaming node registry
    registry: &'a StreamingNodeRegistry,
}

impl<'a> CapabilityResolver<'a> {
    /// Create a new resolver using the node registry.
    ///
    /// The resolver will query factory capabilities from the registry
    /// during resolution.
    pub fn new(registry: &'a StreamingNodeRegistry) -> Self {
        Self { registry }
    }

    /// Resolve capabilities for a pipeline (Phase 1).
    ///
    /// This method performs the full resolution algorithm:
    /// 1. Build connection graph from node definitions
    /// 2. Forward pass to resolve Static, Configured, Passthrough, RuntimeDiscovered
    /// 3. Reverse pass to resolve Adaptive nodes
    /// 4. Validation pass to check all connections
    ///
    /// # Arguments
    /// * `nodes` - Node definitions (node_id, node_type)
    /// * `connections` - Edge definitions (from_node_id, to_node_id)
    /// * `params` - Node parameters (node_id -> params)
    ///
    /// # Returns
    /// * `Ok(ResolutionContext)` - Resolution context with all resolved capabilities
    /// * `Err(Error)` - If resolution fails (missing node types, cycles, etc.)
    pub fn resolve(
        &self,
        nodes: &[(String, String)], // (node_id, node_type)
        connections: &[(String, String)], // (from_node_id, to_node_id)
        params: &HashMap<String, Value>,
    ) -> Result<ResolutionContext, Error> {
        self.resolve_with_hints(nodes, connections, params, &CapabilityHints::new())
    }

    /// Resolve capabilities for a pipeline with user-provided hints.
    ///
    /// This is the same as `resolve()` but accepts `CapabilityHints` to guide
    /// resolution when multiple valid values exist (e.g., Adaptive nodes).
    ///
    /// # Arguments
    /// * `nodes` - Node definitions (node_id, node_type)
    /// * `connections` - Edge definitions (from_node_id, to_node_id)
    /// * `params` - Node parameters (node_id -> params)
    /// * `hints` - User-provided hints for ambiguous resolution
    ///
    /// # Returns
    /// * `Ok(ResolutionContext)` - Resolution context with all resolved capabilities
    /// * `Err(Error)` - If resolution fails (missing node types, cycles, etc.)
    pub fn resolve_with_hints(
        &self,
        nodes: &[(String, String)], // (node_id, node_type)
        connections: &[(String, String)], // (from_node_id, to_node_id)
        params: &HashMap<String, Value>,
        hints: &CapabilityHints,
    ) -> Result<ResolutionContext, Error> {
        let mut ctx = ResolutionContext::new();

        // Step 1: Build connection graph and set behaviors
        self.build_graph(&mut ctx, nodes, connections, params)?;

        // Step 2: Forward pass (topological order)
        let topo_order = self.topological_sort(&ctx, nodes)?;
        self.forward_pass(&mut ctx, &topo_order)?;

        // Step 3: Reverse pass for Adaptive nodes (with hints)
        self.reverse_pass_with_hints(&mut ctx, &topo_order, hints)?;

        // Step 4: Validation pass
        self.validation_pass(&mut ctx, connections)?;

        Ok(ctx)
    }

    /// Re-validate after RuntimeDiscovered nodes report actual capabilities (Phase 2).
    ///
    /// Called after `node.initialize()` completes for RuntimeDiscovered nodes.
    /// Updates the node's capabilities with actual values and re-validates
    /// affected connections.
    ///
    /// # Arguments
    /// * `ctx` - Resolution context to update
    /// * `node_id` - Node that was initialized
    /// * `actual` - Actual capabilities discovered during initialization
    pub fn revalidate(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
        actual: MediaCapabilities,
    ) -> Result<(), Error> {
        // Update the resolved capabilities
        if let Some(resolved) = ctx.resolved.get_mut(node_id) {
            resolved.confirm_actual(actual);
        } else {
            return Err(Error::Execution(format!(
                "Cannot revalidate unknown node: {}",
                node_id
            )));
        }

        // Re-validate connections from this node
        let downstream: Vec<String> = ctx.downstream_nodes(node_id)
            .iter()
            .map(|s| s.to_string())
            .collect();

        for to_node in downstream {
            self.validate_connection(ctx, node_id, &to_node)?;
        }

        // Re-validate connections to this node
        let upstream: Vec<String> = ctx.upstream_nodes(node_id)
            .iter()
            .map(|s| s.to_string())
            .collect();

        for from_node in upstream {
            self.validate_connection(ctx, &from_node, node_id)?;
        }

        Ok(())
    }

    /// Get resolved capabilities for a node.
    pub fn get_resolved<'b>(
        &self,
        ctx: &'b ResolutionContext,
        node_id: &str,
    ) -> Option<&'b ResolvedCapabilities> {
        ctx.resolved.get(node_id)
    }

    // =========================================================================
    // Internal Methods
    // =========================================================================

    /// Build the connection graph and set behaviors.
    fn build_graph(
        &self,
        ctx: &mut ResolutionContext,
        nodes: &[(String, String)],
        connections: &[(String, String)],
        params: &HashMap<String, Value>,
    ) -> Result<(), Error> {
        // Add nodes with their behaviors and types
        for (node_id, node_type) in nodes {
            let behavior = self.registry.get_capability_behavior(node_type);
            ctx.set_behavior(node_id, behavior);
            ctx.set_state(node_id, ResolutionState::Pending);

            // Store node type
            ctx.node_types.insert(node_id.clone(), node_type.clone());

            // Store params
            if let Some(node_params) = params.get(node_id) {
                ctx.params.insert(node_id.clone(), node_params.clone());
            } else {
                ctx.params.insert(node_id.clone(), Value::Null);
            }
        }

        // Add connections
        for (from, to) in connections {
            ctx.add_connection(from, to);
        }

        Ok(())
    }

    /// Compute topological sort of nodes.
    fn topological_sort(
        &self,
        ctx: &ResolutionContext,
        nodes: &[(String, String)],
    ) -> Result<Vec<String>, Error> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut result = Vec::new();
        let mut queue = Vec::new();

        // Initialize in-degrees
        for (node_id, _) in nodes {
            in_degree.insert(node_id.clone(), 0);
        }

        // Count incoming edges
        for targets in ctx.connections.values() {
            for target in targets {
                *in_degree.entry(target.clone()).or_default() += 1;
            }
        }

        // Start with nodes that have no incoming edges
        for (node_id, degree) in &in_degree {
            if *degree == 0 {
                queue.push(node_id.clone());
            }
        }

        // Process queue
        while let Some(node_id) = queue.pop() {
            result.push(node_id.clone());

            if let Some(targets) = ctx.connections.get(&node_id) {
                for target in targets {
                    if let Some(degree) = in_degree.get_mut(target) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push(target.clone());
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != nodes.len() {
            return Err(Error::Execution(
                "Cycle detected in pipeline graph - capability resolution requires a DAG".to_string(),
            ));
        }

        Ok(result)
    }

    /// Forward pass: resolve Static, Configured, Passthrough, RuntimeDiscovered.
    fn forward_pass(
        &self,
        ctx: &mut ResolutionContext,
        topo_order: &[String],
    ) -> Result<(), Error> {
        for node_id in topo_order {
            let behavior = ctx.get_behavior(node_id);
            let params = ctx.params.get(node_id).cloned().unwrap_or(Value::Null);

            match behavior {
                CapabilityBehavior::Static | CapabilityBehavior::Configured => {
                    // Get capabilities from factory
                    self.resolve_static_or_configured(ctx, node_id, &params)?;
                }
                CapabilityBehavior::Passthrough => {
                    // Inherit from upstream
                    self.resolve_passthrough(ctx, node_id)?;
                }
                CapabilityBehavior::Adaptive => {
                    // Mark as needing reverse pass - resolve input only
                    self.mark_needs_reverse(ctx, node_id, &params)?;
                }
                CapabilityBehavior::RuntimeDiscovered => {
                    // Use potential capabilities (provisional)
                    self.resolve_runtime_discovered(ctx, node_id, &params)?;
                }
            }
        }

        Ok(())
    }

    /// Reverse pass: resolve Adaptive nodes (without hints).
    #[allow(dead_code)]
    fn reverse_pass(
        &self,
        ctx: &mut ResolutionContext,
        topo_order: &[String],
    ) -> Result<(), Error> {
        self.reverse_pass_with_hints(ctx, topo_order, &CapabilityHints::new())
    }

    /// Reverse pass: resolve Adaptive nodes with user-provided hints.
    fn reverse_pass_with_hints(
        &self,
        ctx: &mut ResolutionContext,
        topo_order: &[String],
        hints: &CapabilityHints,
    ) -> Result<(), Error> {
        // Process in reverse order
        for node_id in topo_order.iter().rev() {
            if ctx.get_state(node_id) == &ResolutionState::NeedsReverse {
                self.resolve_adaptive_with_hints(ctx, node_id, hints)?;
            }
        }

        Ok(())
    }

    /// Validation pass: check all connections.
    fn validation_pass(
        &self,
        ctx: &mut ResolutionContext,
        connections: &[(String, String)],
    ) -> Result<(), Error> {
        for (from, to) in connections {
            self.validate_connection(ctx, from, to)?;
        }
        Ok(())
    }

    /// Resolve Static or Configured node.
    fn resolve_static_or_configured(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
        params: &Value,
    ) -> Result<(), Error> {
        // Get node_type from behaviors (we need to look it up)
        // For now, we need to find the node_type from the params or stored info
        // Actually, we need to store node_type in the context

        // Try to get capabilities from registry using stored node type
        // We'll need to enhance the context to store node types

        // For now, check if we have capabilities in params (temporary workaround)
        // In a full implementation, we'd look up the node_type and query the registry

        let behavior = ctx.get_behavior(node_id);

        // Look up capabilities - for now we'll create empty if not found
        // The actual implementation would query registry.get_media_capabilities(node_type, params)
        if let Some(caps) = self.get_factory_capabilities(ctx, node_id, params) {
            let resolved = match behavior {
                CapabilityBehavior::Static => ResolvedCapabilities::from_static(node_id, caps),
                CapabilityBehavior::Configured => ResolvedCapabilities::from_configured(node_id, caps),
                _ => unreachable!(),
            };
            ctx.resolved.insert(node_id.to_string(), resolved);
            ctx.set_state(node_id, ResolutionState::Complete);
        } else {
            // No capabilities declared - treat as passthrough
            ctx.set_state(node_id, ResolutionState::Complete);
        }

        Ok(())
    }

    /// Resolve Passthrough node (inherit from upstream).
    fn resolve_passthrough(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
    ) -> Result<(), Error> {
        let upstream = ctx.upstream_nodes(node_id);

        if upstream.is_empty() {
            // Source node with no upstream - can't be passthrough
            // Treat as having no constraints
            ctx.set_state(node_id, ResolutionState::Complete);
            return Ok(());
        }

        // Get output from first upstream node
        if let Some(upstream_output) = ctx.get_output(upstream[0]) {
            let resolved = ResolvedCapabilities::passthrough(node_id, upstream_output);
            ctx.resolved.insert(node_id.to_string(), resolved);
        }

        ctx.set_state(node_id, ResolutionState::Complete);
        Ok(())
    }

    /// Mark Adaptive node as needing reverse pass.
    fn mark_needs_reverse(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
        params: &Value,
    ) -> Result<(), Error> {
        // Get input capabilities (if any) from factory
        if let Some(caps) = self.get_factory_capabilities(ctx, node_id, params) {
            let resolved = ResolvedCapabilities::needs_reverse(node_id, caps);
            ctx.resolved.insert(node_id.to_string(), resolved);
        }

        ctx.set_state(node_id, ResolutionState::NeedsReverse);
        Ok(())
    }

    /// Resolve RuntimeDiscovered node (use potential capabilities).
    fn resolve_runtime_discovered(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
        params: &Value,
    ) -> Result<(), Error> {
        // Use potential capabilities for Phase 1 validation
        // These are broad capabilities that will be refined after device initialization
        if let Some(caps) = self.get_potential_capabilities(ctx, node_id, params) {
            let resolved = ResolvedCapabilities::provisional(node_id, caps);
            ctx.resolved.insert(node_id.to_string(), resolved);
        }

        ctx.set_state(node_id, ResolutionState::ResolvedForward);
        Ok(())
    }

    /// Get potential capabilities from factory using node_type lookup.
    fn get_potential_capabilities(
        &self,
        ctx: &ResolutionContext,
        node_id: &str,
        params: &Value,
    ) -> Option<MediaCapabilities> {
        // Look up the node_type for this node_id
        let node_type = ctx.get_node_type(node_id)?;

        // Query the registry for potential capabilities
        self.registry.get_potential_capabilities(node_type, params)
    }

    /// Resolve Adaptive node during reverse pass (without hints).
    #[allow(dead_code)]
    fn resolve_adaptive(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
    ) -> Result<(), Error> {
        self.resolve_adaptive_with_hints(ctx, node_id, &CapabilityHints::new())
    }

    /// Resolve Adaptive node during reverse pass with user-provided hints.
    ///
    /// Hints are applied when the downstream node accepts a range of values.
    /// For example, if downstream accepts sample_rate 8000-48000 and hints
    /// specify preferred_sample_rate=16000, the output will use 16000.
    fn resolve_adaptive_with_hints(
        &self,
        ctx: &mut ResolutionContext,
        node_id: &str,
        hints: &CapabilityHints,
    ) -> Result<(), Error> {
        let downstream: Vec<String> = ctx.downstream_nodes(node_id)
            .iter()
            .map(|s| s.to_string())
            .collect();

        if downstream.is_empty() {
            // No downstream - can't adapt, mark as complete
            ctx.set_state(node_id, ResolutionState::Complete);
            return Ok(());
        }

        // Get input requirements from first downstream node (clone to avoid borrow issues)
        let downstream_input = ctx.get_input(&downstream[0]).cloned();

        if let Some(input_constraints) = downstream_input {
            // Apply hints to refine the constraints if possible
            let refined_constraints = self.apply_hints_to_constraints(
                node_id,
                input_constraints,
                hints,
            );

            // Update output to match downstream input (with hints applied)
            if let Some(resolved) = ctx.resolved.get_mut(node_id) {
                // Set output to match downstream input requirements
                resolved.capabilities.outputs.insert(
                    "default".to_string(),
                    refined_constraints,
                );
                resolved.sources.insert(
                    "default".to_string(),
                    super::dynamic::CapabilitySource::Negotiated,
                );
                resolved.mark_complete();
            }
        }

        ctx.set_state(node_id, ResolutionState::Complete);
        Ok(())
    }

    /// Apply hints to refine media constraints.
    ///
    /// When constraints contain ranges, hints can narrow them to specific values.
    fn apply_hints_to_constraints(
        &self,
        node_id: &str,
        constraints: super::constraints::MediaConstraints,
        hints: &CapabilityHints,
    ) -> super::constraints::MediaConstraints {
        use super::constraints::{ConstraintValue, MediaConstraints};

        match constraints {
            MediaConstraints::Audio(audio) => {
                let mut refined = audio.clone();

                // Apply preferred sample rate if hint exists and constraint allows it
                if let Some(preferred_rate) = hints.preferred_sample_rate(node_id) {
                    if self.constraint_allows_value(&audio.sample_rate, preferred_rate) {
                        refined.sample_rate = Some(ConstraintValue::Exact(preferred_rate));
                    }
                }

                // Apply preferred channels if hint exists and constraint allows it
                if let Some(preferred_ch) = hints.preferred_channels(node_id) {
                    if self.constraint_allows_value(&audio.channels, preferred_ch) {
                        refined.channels = Some(ConstraintValue::Exact(preferred_ch));
                    }
                }

                // Apply preferred format if hint exists
                if let Some(ref preferred_fmt) = hints.nodes.get(node_id).and_then(|h| h.preferred_format.as_ref()) {
                    if let Some(format) = self.parse_audio_format(preferred_fmt) {
                        refined.format = Some(ConstraintValue::Exact(format));
                    }
                }

                MediaConstraints::Audio(refined)
            }
            // For other constraint types, return as-is (hints only support audio for now)
            other => other,
        }
    }

    /// Check if a constraint allows a specific value.
    fn constraint_allows_value(
        &self,
        constraint: &Option<super::constraints::ConstraintValue<u32>>,
        value: u32,
    ) -> bool {
        use super::constraints::ConstraintValue;

        match constraint {
            None => true, // No constraint means any value is allowed
            Some(ConstraintValue::Exact(exact)) => *exact == value,
            Some(ConstraintValue::Range { min, max }) => value >= *min && value <= *max,
            Some(ConstraintValue::Set(values)) => values.contains(&value),
        }
    }

    /// Parse an audio format string into AudioSampleFormat.
    fn parse_audio_format(&self, format: &str) -> Option<super::constraints::AudioSampleFormat> {
        use super::constraints::AudioSampleFormat;

        match format.to_lowercase().as_str() {
            "f32" | "float32" => Some(AudioSampleFormat::F32),
            "i16" | "int16" | "s16" => Some(AudioSampleFormat::I16),
            "i32" | "int32" | "s32" => Some(AudioSampleFormat::I32),
            "u8" | "uint8" => Some(AudioSampleFormat::U8),
            _ => None,
        }
    }

    /// Validate a connection between two nodes.
    fn validate_connection(
        &self,
        ctx: &mut ResolutionContext,
        from_node: &str,
        to_node: &str,
    ) -> Result<(), Error> {
        // Get resolved capabilities for both nodes
        let from_caps = ctx.resolved.get(from_node);
        let to_caps = ctx.resolved.get(to_node);

        // If either node has no declared capabilities, skip validation
        let (from_resolved, to_resolved) = match (from_caps, to_caps) {
            (Some(from), Some(to)) => (from, to),
            _ => return Ok(()), // No validation needed
        };

        // Use the existing validation logic from the validation module
        use super::validation::validate_connection;

        let mismatches = validate_connection(
            from_node,
            &from_resolved.capabilities,
            to_node,
            &to_resolved.capabilities,
            "default", // source port
            "default", // target port
        );

        // Add any mismatches to context
        for mismatch in mismatches {
            ctx.add_error(mismatch);
        }

        Ok(())
    }

    /// Get capabilities from factory using node_type lookup.
    fn get_factory_capabilities(
        &self,
        ctx: &ResolutionContext,
        node_id: &str,
        params: &Value,
    ) -> Option<MediaCapabilities> {
        // Look up the node_type for this node_id
        let node_type = ctx.get_node_type(node_id)?;

        // Query the registry for capabilities
        self.registry.get_media_capabilities(node_type, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_creation() {
        let registry = StreamingNodeRegistry::new();
        let _resolver = CapabilityResolver::new(&registry);
    }

    #[test]
    fn test_empty_pipeline() {
        let registry = StreamingNodeRegistry::new();
        let resolver = CapabilityResolver::new(&registry);

        let nodes: Vec<(String, String)> = vec![];
        let connections: Vec<(String, String)> = vec![];
        let params = HashMap::new();

        let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();
        assert!(ctx.resolved.is_empty());
        assert!(!ctx.has_errors());
    }

    #[test]
    fn test_topological_sort_linear() {
        let registry = StreamingNodeRegistry::new();
        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("a".to_string(), "NodeA".to_string()),
            ("b".to_string(), "NodeB".to_string()),
            ("c".to_string(), "NodeC".to_string()),
        ];
        let connections = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "c".to_string()),
        ];
        let params = HashMap::new();

        let ctx = resolver.resolve(&nodes, &connections, &params).unwrap();

        // All nodes should have been processed
        assert_eq!(ctx.states.len(), 3);
    }

    #[test]
    fn test_cycle_detection() {
        let registry = StreamingNodeRegistry::new();
        let resolver = CapabilityResolver::new(&registry);

        let nodes = vec![
            ("a".to_string(), "NodeA".to_string()),
            ("b".to_string(), "NodeB".to_string()),
        ];
        // Create a cycle: a -> b -> a
        let connections = vec![
            ("a".to_string(), "b".to_string()),
            ("b".to_string(), "a".to_string()),
        ];
        let params = HashMap::new();

        let result = resolver.resolve(&nodes, &connections, &params);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cycle"));
    }

    #[test]
    fn test_resolution_performance_sc001() {
        // SC-001: Capability resolution should complete within 10ms
        use std::time::Instant;

        let registry = StreamingNodeRegistry::new();
        let resolver = CapabilityResolver::new(&registry);

        // Create a moderately complex pipeline with 10 nodes
        let nodes: Vec<_> = (0..10)
            .map(|i| (format!("node_{}", i), format!("NodeType{}", i)))
            .collect();

        // Create a linear chain of connections
        let connections: Vec<_> = (0..9)
            .map(|i| (format!("node_{}", i), format!("node_{}", i + 1)))
            .collect();

        let params = HashMap::new();

        // Measure resolution time
        let start = Instant::now();
        let _result = resolver.resolve(&nodes, &connections, &params);
        let elapsed = start.elapsed();

        // SC-001: Must be under 10ms
        assert!(
            elapsed.as_millis() < 10,
            "Resolution took {}ms, expected <10ms (SC-001)",
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_introspection_performance_sc005() {
        // SC-005: Introspection queries should complete within 1ms
        use std::time::Instant;
        use super::super::dynamic::ResolvedCapabilities;
        use super::super::constraints::MediaCapabilities;

        let mut ctx = ResolutionContext::new();

        // Add some resolved capabilities
        for i in 0..100 {
            let node_id = format!("node_{}", i);
            let caps = MediaCapabilities {
                inputs: HashMap::new(),
                outputs: HashMap::new(),
            };
            ctx.resolved.insert(
                node_id.clone(),
                ResolvedCapabilities::from_static(&node_id, caps),
            );
        }

        // Measure introspection time
        let start = Instant::now();
        for i in 0..100 {
            let _caps = ctx.resolved.get(&format!("node_{}", i));
        }
        let elapsed = start.elapsed();

        // SC-005: Must be under 1ms for 100 queries
        assert!(
            elapsed.as_micros() < 1000,
            "Introspection took {}μs for 100 queries, expected <1000μs (SC-005)",
            elapsed.as_micros()
        );
    }
}
