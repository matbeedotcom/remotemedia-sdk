//! Integration tests for pipeline execution orchestration (Phase 3)
//!
//! Tests verify pipeline graph construction and execution ordering with various topologies:
//! - T042: Linear pipelines (A → B → C)
//! - T043: Branching topologies (A → B, A → C → D)  
//! - T044: Converging topologies (A → C, B → C)
//! - T045: Cycle detection (A → B → C → A)

use remotemedia_runtime::executor::PipelineGraph;
use remotemedia_runtime::manifest::{parse, Manifest};

fn create_manifest_json(name: &str, nodes: &str, connections: &str) -> String {
    format!(r#"{{
        "version": "v1",
        "metadata": {{
            "name": "{}",
            "description": "Test manifest"
        }},
        "nodes": {},
        "connections": {}
    }}"#, name, nodes, connections)
}

#[test]
fn test_linear_pipeline() {
    // T042: Linear pipeline A → B → C
    let manifest_json = create_manifest_json(
        "linear_pipeline",
        r#"[
            {"id": "node_a", "node_type": "SourceNode", "params": {}},
            {"id": "node_b", "node_type": "ProcessorNode", "params": {}},
            {"id": "node_c", "node_type": "SinkNode", "params": {}}
        ]"#,
        r#"[
            {"from": "node_a", "to": "node_b"},
            {"from": "node_b", "to": "node_c"}
        ]"#
    );

    let manifest = parse(&manifest_json).expect("Failed to parse manifest");
    let graph = PipelineGraph::from_manifest(&manifest)
        .expect("Failed to build linear pipeline graph");

    // Verify execution order: A must come before B, B before C
    let order = &graph.execution_order;
    let a_idx = order.iter().position(|n| n == "node_a").unwrap();
    let b_idx = order.iter().position(|n| n == "node_b").unwrap();
    let c_idx = order.iter().position(|n| n == "node_c").unwrap();

    assert!(a_idx < b_idx, "node_a should execute before node_b");
    assert!(b_idx < c_idx, "node_b should execute before node_c");

    // Verify sources and sinks
    assert_eq!(graph.sources, vec!["node_a"]);
    assert_eq!(graph.sinks, vec!["node_c"]);

    println!("✓ T042: Linear pipeline test passed");
}

#[test]
fn test_branching_topology() {
    // T043: Branching topology A → B, A → C, B → D, C → D
    let manifest_json = create_manifest_json(
        "branching_pipeline",
        r#"[
            {"id": "node_a", "node_type": "SourceNode", "params": {}},
            {"id": "node_b", "node_type": "ProcessorNode", "params": {}},
            {"id": "node_c", "node_type": "ProcessorNode", "params": {}},
            {"id": "node_d", "node_type": "SinkNode", "params": {}}
        ]"#,
        r#"[
            {"from": "node_a", "to": "node_b"},
            {"from": "node_a", "to": "node_c"},
            {"from": "node_b", "to": "node_d"},
            {"from": "node_c", "to": "node_d"}
        ]"#
    );

    let manifest = parse(&manifest_json).expect("Failed to parse manifest");
    let graph = PipelineGraph::from_manifest(&manifest)
        .expect("Failed to build branching pipeline graph");

    let order = &graph.execution_order;
    let a_idx = order.iter().position(|n| n == "node_a").unwrap();
    let b_idx = order.iter().position(|n| n == "node_b").unwrap();
    let c_idx = order.iter().position(|n| n == "node_c").unwrap();
    let d_idx = order.iter().position(|n| n == "node_d").unwrap();

    // A must execute before B and C
    assert!(a_idx < b_idx, "node_a should execute before node_b");
    assert!(a_idx < c_idx, "node_a should execute before node_c");

    // B and C can execute in parallel, but both must execute before D
    assert!(b_idx < d_idx, "node_b should execute before node_d");
    assert!(c_idx < d_idx, "node_c should execute before node_d");

    // Single source, single sink
    assert_eq!(graph.sources, vec!["node_a"]);
    assert_eq!(graph.sinks, vec!["node_d"]);

    println!("✓ T043: Branching topology test passed");
}

#[test]
fn test_converging_topology() {
    // T044: Converging topology A → C, B → C
    let manifest_json = create_manifest_json(
        "converging_pipeline",
        r#"[
            {"id": "node_a", "node_type": "SourceNode", "params": {}},
            {"id": "node_b", "node_type": "SourceNode", "params": {}},
            {"id": "node_c", "node_type": "ProcessorNode", "params": {}}
        ]"#,
        r#"[
            {"from": "node_a", "to": "node_c"},
            {"from": "node_b", "to": "node_c"}
        ]"#
    );

    let manifest = parse(&manifest_json).expect("Failed to parse manifest");
    let graph = PipelineGraph::from_manifest(&manifest)
        .expect("Failed to build converging pipeline graph");

    let order = &graph.execution_order;
    let a_idx = order.iter().position(|n| n == "node_a").unwrap();
    let b_idx = order.iter().position(|n| n == "node_b").unwrap();
    let c_idx = order.iter().position(|n| n == "node_c").unwrap();

    // Both A and B must execute before C
    assert!(a_idx < c_idx, "node_a should execute before node_c");
    assert!(b_idx < c_idx, "node_b should execute before node_c");

    // A and B are both sources (can execute in parallel)
    let mut sources = graph.sources.clone();
    sources.sort();
    assert_eq!(sources, vec!["node_a", "node_b"]);

    // C is the only sink
    assert_eq!(graph.sinks, vec!["node_c"]);

    println!("✓ T044: Converging topology test passed");
}

#[test]
fn test_cycle_detection() {
    // T045: Cyclic pipeline A → B → C → A
    let manifest_json = create_manifest_json(
        "cyclic_pipeline",
        r#"[
            {"id": "node_a", "node_type": "ProcessorNode", "params": {}},
            {"id": "node_b", "node_type": "ProcessorNode", "params": {}},
            {"id": "node_c", "node_type": "ProcessorNode", "params": {}}
        ]"#,
        r#"[
            {"from": "node_a", "to": "node_b"},
            {"from": "node_b", "to": "node_c"},
            {"from": "node_c", "to": "node_a"}
        ]"#
    );

    let manifest = parse(&manifest_json).expect("Failed to parse manifest");
    
    // Graph construction should fail due to cycle
    let result = PipelineGraph::from_manifest(&manifest);

    assert!(
        result.is_err(),
        "Pipeline with cycle should fail to build"
    );

    if let Err(err) = result {
        let err_str = err.to_string().to_lowercase();
        assert!(
            err_str.contains("cycle") || err_str.contains("circular"),
            "Error should mention cycle, got: {}",
            err
        );
    }

    println!("✓ T045: Cycle detection test passed");
}

