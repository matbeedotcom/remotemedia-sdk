//! T059: Performance benchmark for node parameter validation
//!
//! Success criteria: validation overhead < 50ms for 100 nodes
//!
//! This benchmark measures the time to validate a manifest with varying numbers
//! of nodes to ensure validation doesn't become a bottleneck.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use remotemedia_core::manifest::{Manifest, ManifestMetadata, NodeManifest};
use remotemedia_core::nodes::schema::create_builtin_schema_registry;
use remotemedia_core::validation::{validate_manifest, SchemaValidator};
use serde_json::json;
use std::time::Duration;

/// Create a test manifest with N nodes
fn create_test_manifest(node_count: usize) -> Manifest {
    let nodes: Vec<NodeManifest> = (0..node_count)
        .map(|i| {
            // Alternate between different node types to simulate real pipelines
            let (node_type, params) = match i % 4 {
                0 => (
                    "SileroVAD",
                    json!({
                        "threshold": 0.5,
                        "minSpeechDuration": 0.25,
                        "minSilenceDuration": 0.1
                    }),
                ),
                1 => (
                    "KokoroTTSNode",
                    json!({
                        "voice": "af_heart",
                        "speed": 1.0
                    }),
                ),
                2 => (
                    "AudioResample",
                    json!({
                        "targetSampleRate": 16000,
                        "targetChannels": 1
                    }),
                ),
                _ => (
                    "UnknownNode", // Will produce warning, not error
                    json!({
                        "anyParam": "value"
                    }),
                ),
            };

            NodeManifest {
                id: format!("node_{}", i),
                node_type: node_type.to_string(),
                params,
                ..Default::default()
            }
        })
        .collect();

    Manifest {
        version: "1.0".to_string(),
        metadata: ManifestMetadata {
            name: "benchmark_pipeline".to_string(),
            description: Some("Performance test manifest".to_string()),
            ..Default::default()
        },
        nodes,
        connections: vec![],
    }
}

/// Benchmark validation with varying node counts
fn bench_validation(c: &mut Criterion) {
    // Create validator once (this is done at startup in real usage)
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let mut group = c.benchmark_group("validation");

    // Configure for faster iteration during development
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(100);

    for node_count in [10, 25, 50, 100, 200].iter() {
        let manifest = create_test_manifest(*node_count);

        group.bench_with_input(
            BenchmarkId::new("nodes", node_count),
            &manifest,
            |b, manifest| {
                b.iter(|| {
                    let result = validate_manifest(black_box(manifest), black_box(&validator));
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark validator creation (one-time startup cost)
fn bench_validator_creation(c: &mut Criterion) {
    let registry = create_builtin_schema_registry();

    c.bench_function("validator_creation", |b| {
        b.iter(|| {
            let validator = SchemaValidator::from_registry(black_box(&registry)).unwrap();
            black_box(validator)
        });
    });
}

/// Benchmark schema introspection
fn bench_schema_introspection(c: &mut Criterion) {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();

    let mut group = c.benchmark_group("introspection");

    group.bench_function("get_all_schemas", |b| {
        b.iter(|| {
            let schemas = validator.get_all_schemas();
            black_box(schemas)
        });
    });

    group.bench_function("get_single_schema", |b| {
        b.iter(|| {
            let schema = validator.get_schema(black_box("SileroVAD"));
            black_box(schema)
        });
    });

    group.bench_function("has_schema", |b| {
        b.iter(|| {
            let has = validator.has_schema(black_box("SileroVAD"));
            black_box(has)
        });
    });

    group.finish();
}

/// Quick sanity check that 100 nodes validates in < 50ms
fn bench_100_nodes_target(c: &mut Criterion) {
    let registry = create_builtin_schema_registry();
    let validator = SchemaValidator::from_registry(&registry).unwrap();
    let manifest = create_test_manifest(100);

    c.bench_function("100_nodes_target_50ms", |b| {
        b.iter(|| {
            let result = validate_manifest(black_box(&manifest), black_box(&validator));
            black_box(result)
        });
    });
}

criterion_group!(
    benches,
    bench_validation,
    bench_validator_creation,
    bench_schema_introspection,
    bench_100_nodes_target
);

criterion_main!(benches);
