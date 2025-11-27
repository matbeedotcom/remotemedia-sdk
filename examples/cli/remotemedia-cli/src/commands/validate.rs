//! `remotemedia validate` command - Validate a pipeline manifest

use anyhow::{Context, Result};
use clap::Args;
use std::collections::HashSet;
use std::path::PathBuf;

use crate::output::{OutputFormat, Outputter};

/// Arguments for the validate command
#[derive(Args)]
pub struct ValidateArgs {
    /// Path to pipeline manifest (YAML or JSON)
    pub manifest: PathBuf,

    /// Verify that node types are registered
    #[arg(long)]
    pub check_nodes: bool,

    /// Check nodes against remote server
    #[arg(long)]
    pub server: Option<String>,
}

pub async fn execute(args: ValidateArgs, format: OutputFormat) -> Result<()> {
    let outputter = Outputter::new(format);

    // Read manifest file
    let manifest_content = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest: {:?}", args.manifest))?;

    // Parse as YAML
    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_content)
        .map_err(|e| anyhow::anyhow!("Invalid YAML: {}", e))?;

    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Check required version field
    if manifest.get("version").is_none() {
        errors.push("Missing required 'version' field".to_string());
    } else {
        let version = manifest["version"].as_str().unwrap_or("");
        if version != "v1" {
            warnings.push(format!("Unknown version '{}', expected 'v1'", version));
        }
    }

    // Validate nodes
    let nodes = manifest.get("nodes").and_then(|n| n.as_sequence());
    if let Some(nodes) = nodes {
        let mut seen_ids = HashSet::new();

        for (i, node) in nodes.iter().enumerate() {
            // Check node has id
            let node_id = node.get("id").and_then(|id| id.as_str());
            match node_id {
                Some(id) => {
                    if !seen_ids.insert(id.to_string()) {
                        errors.push(format!("Duplicate node id '{}' at index {}", id, i));
                    }
                }
                None => {
                    errors.push(format!("Node at index {} missing 'id' field", i));
                }
            }

            // Check node has type
            if node.get("node_type").is_none() {
                errors.push(format!(
                    "Node '{}' missing 'node_type' field",
                    node_id.unwrap_or("<unknown>")
                ));
            }

            // Check node type exists (if --check-nodes)
            if args.check_nodes {
                if let Some(node_type) = node.get("node_type").and_then(|t| t.as_str()) {
                    // TODO: Check against registered node types
                    let known_types = [
                        "SileroVADNode",
                        "WhisperNode",
                        "KokoroTTSNode",
                        "EchoNode",
                        "PassthroughNode",
                        "RemotePipelineNode",
                    ];
                    if !known_types.contains(&node_type) {
                        errors.push(format!("Node type '{}' not found", node_type));
                    }
                }
            }
        }
    } else if manifest.get("nodes").is_some() {
        errors.push("'nodes' field must be an array".to_string());
    }

    // Validate connections
    if let Some(connections) = manifest.get("connections").and_then(|c| c.as_sequence()) {
        let node_ids: HashSet<String> = nodes
            .map(|n| {
                n.iter()
                    .filter_map(|node| node.get("id").and_then(|id| id.as_str()))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        // Build connection graph for cycle detection
        let mut graph: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        for conn in connections {
            let from = conn.get("from").and_then(|f| f.as_str());
            let to = conn.get("to").and_then(|t| t.as_str());

            match (from, to) {
                (Some(from), Some(to)) => {
                    if !node_ids.contains(from) {
                        errors.push(format!("Connection references unknown node '{}'", from));
                    }
                    if !node_ids.contains(to) {
                        errors.push(format!("Connection references unknown node '{}'", to));
                    }
                    graph.entry(from.to_string()).or_default().push(to.to_string());
                }
                _ => {
                    errors.push("Connection missing 'from' or 'to' field".to_string());
                }
            }
        }

        // Check for cycles using DFS
        if let Some(cycle) = detect_cycle(&graph) {
            errors.push(format!("Circular dependency detected: {}", cycle.join(" -> ")));
        }
    }

    // Output result
    let is_valid = errors.is_empty();
    let result = serde_json::json!({
        "valid": is_valid,
        "manifest": args.manifest.display().to_string(),
        "errors": errors,
        "warnings": warnings,
    });

    if is_valid {
        outputter.output(&result)?;
        println!("✓ Manifest is valid");
        Ok(())
    } else {
        outputter.output(&result)?;
        for error in &errors {
            eprintln!("✗ {}", error);
        }
        std::process::exit(1);
    }
}

/// Detect cycles in a directed graph using DFS
fn detect_cycle(graph: &std::collections::HashMap<String, Vec<String>>) -> Option<Vec<String>> {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for node in graph.keys() {
        if let Some(cycle) = dfs_cycle(node, graph, &mut visited, &mut rec_stack, &mut path) {
            return Some(cycle);
        }
    }
    None
}

fn dfs_cycle(
    node: &str,
    graph: &std::collections::HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    rec_stack: &mut HashSet<String>,
    path: &mut Vec<String>,
) -> Option<Vec<String>> {
    if rec_stack.contains(node) {
        // Found cycle - extract the cycle from path
        let cycle_start = path.iter().position(|n| n == node).unwrap_or(0);
        let mut cycle: Vec<String> = path[cycle_start..].to_vec();
        cycle.push(node.to_string());
        return Some(cycle);
    }

    if visited.contains(node) {
        return None;
    }

    visited.insert(node.to_string());
    rec_stack.insert(node.to_string());
    path.push(node.to_string());

    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
            if let Some(cycle) = dfs_cycle(neighbor, graph, visited, rec_stack, path) {
                return Some(cycle);
            }
        }
    }

    rec_stack.remove(node);
    path.pop();
    None
}
