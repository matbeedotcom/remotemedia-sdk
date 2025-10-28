//! RustPython Node Execution Benchmarks (Phase 1.6)
//!
//! This benchmark suite measures the performance of Python nodes executed
//! in the embedded RustPython VM compared to native Rust nodes.
//!
//! Key Metrics:
//! - Single node execution time
//! - State preservation overhead
//! - Data marshaling overhead
//! - Pipeline execution with mixed Rust/RustPython nodes
//!
//! Expected Results (Phase 1.6):
//! - RustPython: 1.5-2x slower than native Rust
//! - State preservation: negligible overhead (globals reuse)
//! - Marshaling: <10% overhead for simple types

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use remotemedia_runtime::python::PythonNodeInstance;
use serde_json::json;

/// Benchmark 1: Simple node execution
fn bench_simple_node_rustpython(c: &mut Criterion) {
    let source_code = r#"
class CalculatorNode:
    def __init__(self, operation="multiply", operand=3):
        self.operation = operation
        self.operand = operand

    def process(self, data):
        if self.operation == "multiply":
            return data * self.operand
        elif self.operation == "add":
            return data + self.operand
        return data
"#;

    let params = json!({
        "operation": "multiply",
        "operand": 3
    });

    c.bench_function("rustpython_simple_node", |b| {
        b.iter(|| {
            let mut node =
                PythonNodeInstance::from_source(source_code, "CalculatorNode", params.clone())
                    .expect("Failed to create node");

            black_box(node.process(json!(42)).expect("Failed to process"))
        });
    });
}

/// Benchmark 2: Stateful node with state preservation
fn bench_stateful_node_rustpython(c: &mut Criterion) {
    let source_code = r#"
class CounterNode:
    def __init__(self):
        self.count = 0

    def process(self, data):
        self.count += 1
        return {"value": data, "count": self.count}
"#;

    c.bench_function("rustpython_stateful_node", |b| {
        // Create node once, reuse across iterations to test state preservation
        let mut node = PythonNodeInstance::from_source(source_code, "CounterNode", json!(null))
            .expect("Failed to create node");

        b.iter(|| black_box(node.process(json!(42)).expect("Failed to process")));
    });
}

/// Benchmark 3: Data marshaling overhead
fn bench_marshaling_overhead(c: &mut Criterion) {
    let source_code = r#"
class PassThroughNode:
    def __init__(self):
        pass

    def process(self, data):
        return data
"#;

    let mut group = c.benchmark_group("marshaling_overhead");

    // Test different data types
    let test_cases = vec![
        ("int", json!(42)),
        ("string", json!("hello world")),
        ("list_small", json!([1, 2, 3, 4, 5])),
        ("list_large", json!((0..100).collect::<Vec<i32>>())),
        ("dict_simple", json!({"key": "value", "number": 42})),
        (
            "dict_nested",
            json!({
                "data": {"nested": {"value": 42}},
                "array": [1, 2, 3],
                "string": "test"
            }),
        ),
    ];

    for (name, data) in test_cases {
        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            let mut node =
                PythonNodeInstance::from_source(source_code, "PassThroughNode", json!(null))
                    .expect("Failed to create node");

            b.iter(|| black_box(node.process(data.clone()).expect("Failed to process")));
        });
    }

    group.finish();
}

/// Benchmark 4: Node initialization overhead
fn bench_node_initialization(c: &mut Criterion) {
    let source_code = r#"
class SimpleNode:
    def __init__(self, value=42):
        self.value = value
        self.initialized = False

    def initialize(self):
        self.initialized = True

    def process(self, data):
        if not self.initialized:
            return {"error": "not initialized"}
        return data + self.value
"#;

    c.bench_function("rustpython_node_initialization", |b| {
        b.iter(|| {
            let mut node =
                PythonNodeInstance::from_source(source_code, "SimpleNode", json!({"value": 10}))
                    .expect("Failed to create node");

            node.initialize().expect("Failed to initialize");
            black_box(node.process(json!(32)).expect("Failed to process"))
        });
    });
}

/// Benchmark 5: Complex data transformations
fn bench_complex_transform(c: &mut Criterion) {
    let source_code = r#"
class TransformNode:
    def __init__(self, multiplier=2):
        self.multiplier = multiplier
        self.processed = 0

    def process(self, data):
        self.processed += 1

        if isinstance(data, dict):
            result = {}
            for key, value in data.items():
                if isinstance(value, (int, float)):
                    result[key] = value * self.multiplier
                else:
                    result[key] = value
            result["_metadata"] = {
                "processed_count": self.processed,
                "multiplier": self.multiplier
            }
            return result
        elif isinstance(data, list):
            return [x * self.multiplier if isinstance(x, (int, float)) else x for x in data]
        else:
            return data * self.multiplier if isinstance(data, (int, float)) else data
"#;

    let params = json!({"multiplier": 3});

    c.bench_function("rustpython_complex_transform", |b| {
        let mut node =
            PythonNodeInstance::from_source(source_code, "TransformNode", params.clone())
                .expect("Failed to create node");

        let test_data = json!({
            "value1": 10,
            "value2": 20,
            "name": "test",
            "nested": {"count": 5}
        });

        b.iter(|| black_box(node.process(test_data.clone()).expect("Failed to process")));
    });
}

/// Benchmark 6: Streaming/generator nodes
fn bench_streaming_node(c: &mut Criterion) {
    let source_code = r#"
class StreamingNode:
    def __init__(self, count=5):
        self.count = count

    def process(self, data):
        for i in range(self.count):
            yield {"index": i, "value": data * i}
"#;

    c.bench_function("rustpython_streaming_node", |b| {
        let mut node =
            PythonNodeInstance::from_source(source_code, "StreamingNode", json!({"count": 5}))
                .expect("Failed to create node");

        b.iter(|| {
            black_box(
                node.process_streaming(json!(10))
                    .expect("Failed to process"),
            )
        });
    });
}

/// Benchmark 7: Multiple nodes in sequence (mini-pipeline)
fn bench_node_chain(c: &mut Criterion) {
    c.bench_function("rustpython_node_chain", |b| {
        // Create a chain of 3 nodes
        let node1_src = r#"
class DoubleNode:
    def process(self, data):
        return data * 2
"#;

        let node2_src = r#"
class AddTenNode:
    def process(self, data):
        return data + 10
"#;

        let node3_src = r#"
class FormatNode:
    def process(self, data):
        return {"result": data, "formatted": f"Result: {data}"}
"#;

        let mut node1 = PythonNodeInstance::from_source(node1_src, "DoubleNode", json!(null))
            .expect("Failed to create node1");
        let mut node2 = PythonNodeInstance::from_source(node2_src, "AddTenNode", json!(null))
            .expect("Failed to create node2");
        let mut node3 = PythonNodeInstance::from_source(node3_src, "FormatNode", json!(null))
            .expect("Failed to create node3");

        b.iter(|| {
            let r1 = node1.process(json!(5)).expect("Failed at node1");
            let r2 = node2.process(r1).expect("Failed at node2");
            black_box(node3.process(r2).expect("Failed at node3"))
        });
    });
}

/// Benchmark 8: Comparison - VM reuse vs recreation
fn bench_vm_reuse_vs_recreation(c: &mut Criterion) {
    let source_code = r#"
class SimpleNode:
    def process(self, data):
        return data * 2
"#;

    let mut group = c.benchmark_group("vm_reuse");

    // Benchmark with VM reuse (normal operation)
    group.bench_function("with_reuse", |b| {
        let mut node = PythonNodeInstance::from_source(source_code, "SimpleNode", json!(null))
            .expect("Failed to create node");

        b.iter(|| black_box(node.process(json!(42)).expect("Failed to process")));
    });

    // Benchmark without VM reuse (recreate each time)
    group.bench_function("without_reuse", |b| {
        b.iter(|| {
            let mut node = PythonNodeInstance::from_source(source_code, "SimpleNode", json!(null))
                .expect("Failed to create node");

            black_box(node.process(json!(42)).expect("Failed to process"))
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_simple_node_rustpython,
    bench_stateful_node_rustpython,
    bench_marshaling_overhead,
    bench_node_initialization,
    bench_complex_transform,
    bench_streaming_node,
    bench_node_chain,
    bench_vm_reuse_vs_recreation,
);

criterion_main!(benches);
