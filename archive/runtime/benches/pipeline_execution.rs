// Benchmark for pipeline execution performance
// Comparing Rust runtime with Python baseline

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use remotemedia_runtime::executor::Executor;
use remotemedia_runtime::manifest::{Connection, Manifest, ManifestMetadata, NodeManifest};
use serde_json::Value;
use tokio::runtime::Runtime;

/// Create a simple 3-node pipeline
fn create_simple_pipeline() -> Manifest {
    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "simple-bench".to_string(),
            description: Some("Simple 3-node pipeline".to_string()),
            created_at: None,
        },
        nodes: vec![
            NodeManifest {
                id: "pass_0".to_string(),
                node_type: "PassThrough".to_string(),
                params: serde_json::json!({}),
                capabilities: None,
                host: None,
            },
            NodeManifest {
                id: "echo_1".to_string(),
                node_type: "Echo".to_string(),
                params: serde_json::json!({}),
                capabilities: None,
                host: None,
            },
            NodeManifest {
                id: "pass_2".to_string(),
                node_type: "PassThrough".to_string(),
                params: serde_json::json!({}),
                capabilities: None,
                host: None,
            },
        ],
        connections: vec![
            Connection {
                from: "pass_0".to_string(),
                to: "echo_1".to_string(),
            },
            Connection {
                from: "echo_1".to_string(),
                to: "pass_2".to_string(),
            },
        ],
    }
}

/// Create a complex 10-node pipeline
fn create_complex_pipeline() -> Manifest {
    let mut nodes = vec![];
    let mut connections = vec![];

    // Create 10 nodes alternating between PassThrough and Echo
    for i in 0..10 {
        let node_type = if i % 2 == 0 { "PassThrough" } else { "Echo" };
        nodes.push(NodeManifest {
            id: format!("node_{}", i),
            node_type: node_type.to_string(),
            params: serde_json::json!({}),
            capabilities: None,
            host: None,
        });

        if i > 0 {
            connections.push(Connection {
                from: format!("node_{}", i - 1),
                to: format!("node_{}", i),
            });
        }
    }

    Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "complex-bench".to_string(),
            description: Some("Complex 10-node pipeline".to_string()),
            created_at: None,
        },
        nodes,
        connections,
    }
}

fn bench_simple_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let manifest = create_simple_pipeline();
    let executor = Executor::new();

    c.bench_function("simple_pipeline_100_items", |b| {
        b.iter(|| {
            let input_data: Vec<Value> = (0..100).map(|i| serde_json::json!(i)).collect();

            rt.block_on(async {
                black_box(
                    executor
                        .execute_with_input(&manifest, input_data)
                        .await
                        .unwrap(),
                )
            })
        });
    });
}

fn bench_complex_pipeline(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let manifest = create_complex_pipeline();
    let executor = Executor::new();

    c.bench_function("complex_pipeline_100_items", |b| {
        b.iter(|| {
            let input_data: Vec<Value> = (0..100).map(|i| serde_json::json!(i)).collect();

            rt.block_on(async {
                black_box(
                    executor
                        .execute_with_input(&manifest, input_data)
                        .await
                        .unwrap(),
                )
            })
        });
    });
}

fn bench_pipeline_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let manifest = create_simple_pipeline();
    let executor = Executor::new();

    let mut group = c.benchmark_group("pipeline_scaling");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let input_data: Vec<Value> = (0..size).map(|i| serde_json::json!(i)).collect();

                rt.block_on(async {
                    black_box(
                        executor
                            .execute_with_input(&manifest, input_data)
                            .await
                            .unwrap(),
                    )
                })
            });
        });
    }

    group.finish();
}

fn bench_graph_building(c: &mut Criterion) {
    let manifest = create_complex_pipeline();

    c.bench_function("graph_construction", |b| {
        b.iter(|| {
            black_box(
                remotemedia_runtime::executor::PipelineGraph::from_manifest(&manifest).unwrap(),
            )
        });
    });
}

criterion_group!(
    benches,
    bench_simple_pipeline,
    bench_complex_pipeline,
    bench_pipeline_sizes,
    bench_graph_building
);
criterion_main!(benches);
