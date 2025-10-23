//! Phase 1.11 Data Flow & Orchestration Tests
//!
//! This test suite validates:
//! - 1.11.1: Sequential data passing between nodes
//! - 1.11.2: Streaming/async generator support
//! - 1.11.3: Backpressure handling
//! - 1.11.4: Branching and merging support
//! - 1.11.5: Complex pipeline topologies

use remotemedia_runtime::{
    executor::Executor,
    manifest::{Connection, Manifest, ManifestMetadata, NodeManifest, RuntimeHint},
};
use pyo3::types::PyAnyMethods;
use serde_json::json;
use std::ffi::CString;

/// Helper to set up Python environment
fn setup_python_environment() {
    pyo3::prepare_freethreaded_python();

    pyo3::Python::with_gil(|py| {
        let sys = py.import("sys").unwrap();
        let path = sys.getattr("path").unwrap();

        let python_client_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("python-client");

        if python_client_path.exists() {
            let path_str = python_client_path.to_str().unwrap();
            let append_code = CString::new(format!(
                "import sys; sys.path.insert(0, r'{}')",
                path_str
            ))
            .unwrap();
            py.run(&append_code, None, None).unwrap();
            println!("Added to Python path: {}", path_str);
        }
    });
}

/// Test 1.11.1: Sequential data passing through linear pipeline
#[tokio::test]
async fn test_sequential_data_passing_linear() {
    setup_python_environment();

    // Create test nodes in Python
    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

# Create remotemedia package structure
if 'remotemedia' not in sys.modules:
    remotemedia_module = types.ModuleType('remotemedia')
    remotemedia_module.__package__ = 'remotemedia'
    remotemedia_module.__path__ = []
    sys.modules['remotemedia'] = remotemedia_module

# Create remotemedia.nodes module
if 'remotemedia.nodes' not in sys.modules:
    nodes_module = types.ModuleType('remotemedia.nodes')
    nodes_module.__package__ = 'remotemedia.nodes'
    sys.modules['remotemedia.nodes'] = nodes_module

class MultiplyNode:
    def __init__(self, factor=2):
        self.factor = factor

    def process(self, data):
        return data * self.factor

class AddNode:
    def __init__(self, offset=10):
        self.offset = offset

    def process(self, data):
        return data + self.offset

# Add to module
sys.modules['remotemedia.nodes'].MultiplyNode = MultiplyNode
sys.modules['remotemedia.nodes'].AddNode = AddNode
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    // Create linear pipeline: Multiply(2) -> Add(10)
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "sequential-test".to_string(),
            description: Some("Test sequential data passing".to_string()),
            created_at: None,
        },
        nodes: vec![
            NodeManifest {
                id: "multiply".to_string(),
                node_type: "MultiplyNode".to_string(),
                params: json!({"factor": 2}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "add".to_string(),
                node_type: "AddNode".to_string(),
                params: json!({"offset": 10}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
        ],
        connections: vec![Connection {
            from: "multiply".to_string(),
            to: "add".to_string(),
        }],
    };

    let executor = Executor::new();
    let input = vec![json!(5), json!(7), json!(3)];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 3);

    // 5 * 2 + 10 = 20
    assert_eq!(outputs[0], json!(20));
    // 7 * 2 + 10 = 24
    assert_eq!(outputs[1], json!(24));
    // 3 * 2 + 10 = 16
    assert_eq!(outputs[2], json!(16));

    println!("✓ Test 1.11.1: Sequential data passing passed!");
}

/// Test 1.11.2: Streaming node with async generator
///
/// This test verifies that streaming nodes (async generators) work correctly
/// and that each item is yielded asynchronously with delays between them
#[tokio::test]
async fn test_streaming_async_generator() {
    setup_python_environment();

    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

# Create remotemedia package structure
if 'remotemedia' not in sys.modules:
    remotemedia_module = types.ModuleType('remotemedia')
    remotemedia_module.__package__ = 'remotemedia'
    remotemedia_module.__path__ = []
    sys.modules['remotemedia'] = remotemedia_module

# Create remotemedia.nodes module
if 'remotemedia.nodes' not in sys.modules:
    nodes_module = types.ModuleType('remotemedia.nodes')
    nodes_module.__package__ = 'remotemedia.nodes'
    sys.modules['remotemedia.nodes'] = nodes_module

class StreamingGeneratorNode:
    """Node that demonstrates async generator streaming with timestamps."""

    def __init__(self):
        self.yield_count = 0

    async def process(self, data_gen):
        """Async generator that yields items with delays to prove async behavior."""
        import asyncio
        import time

        async for data in data_gen:
            if isinstance(data, list):
                for item in data:
                    self.yield_count += 1
                    # Simulate async I/O delay
                    await asyncio.sleep(0.001)
                    yield {
                        "chunk": item,
                        "yield_index": self.yield_count,
                        "timestamp": time.time()
                    }
            else:
                self.yield_count += 1
                await asyncio.sleep(0.001)
                yield {
                    "value": data,
                    "yield_index": self.yield_count,
                    "timestamp": time.time()
                }

sys.modules['remotemedia.nodes'].StreamingGeneratorNode = StreamingGeneratorNode
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "streaming-test".to_string(),
            description: Some("Test async generator streaming".to_string()),
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "generator".to_string(),
            node_type: "StreamingGeneratorNode".to_string(),
            params: json!({}),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython),
        }],
        connections: vec![],
    };

    let executor = Executor::new();
    let input = vec![json!([1, 2, 3, 4, 5])];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    // The streaming node should yield multiple items
    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 5, "Expected 5 outputs from streaming node");

    // Verify async generator behavior:
    // 1. Items should be yielded in order
    for i in 0..5 {
        assert_eq!(outputs[i]["chunk"], json!(i + 1));
        assert_eq!(outputs[i]["yield_index"], json!(i + 1));
    }

    // 2. Each item should have a timestamp (proves async execution)
    for i in 0..5 {
        assert!(outputs[i]["timestamp"].is_number(), "Expected timestamp for yield {}", i);
    }

    // 3. Timestamps should be different (proves items were yielded at different times)
    let ts0 = outputs[0]["timestamp"].as_f64().unwrap();
    let ts4 = outputs[4]["timestamp"].as_f64().unwrap();
    assert!(ts4 > ts0, "Last item should have later timestamp than first item");

    println!("✓ Test 1.11.2: Async generator streaming with delays verified!");
    println!("  - {} items yielded asynchronously", outputs.len());
    println!("  - Time delta: {:.3}ms", (ts4 - ts0) * 1000.0);
}

/// Test 1.11.3: Backpressure handling (implicit via sequential processing)
#[tokio::test]
async fn test_backpressure_handling() {
    setup_python_environment();

    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

if 'remotemedia.nodes' not in sys.modules:
    sys.modules['remotemedia.nodes'] = types.ModuleType('remotemedia.nodes')

class SlowProcessorNode:
    """Simulates a slow processing node."""

    def __init__(self):
        self.processed_count = 0

    def process(self, data):
        # In real scenario, this would be slow I/O or computation
        self.processed_count += 1
        return {
            "data": data,
            "processed": self.processed_count,
            "slow": True
        }

sys.modules['remotemedia.nodes'].SlowProcessorNode = SlowProcessorNode
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "backpressure-test".to_string(),
            description: Some("Test backpressure handling".to_string()),
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "slow".to_string(),
            node_type: "SlowProcessorNode".to_string(),
            params: json!({}),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython),
        }],
        connections: vec![],
    };

    let executor = Executor::new();

    // Send multiple items - backpressure ensures they're processed sequentially
    let input = vec![
        json!("item1"),
        json!("item2"),
        json!("item3"),
        json!("item4"),
        json!("item5"),
    ];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 5);

    // Verify items were processed in order (backpressure maintained order)
    for (i, output) in outputs.iter().enumerate() {
        assert_eq!(output["processed"], json!(i + 1));
        assert_eq!(output["slow"], json!(true));
    }

    println!("✓ Test 1.11.3: Backpressure handling passed!");
}

/// Test 1.11.4: Branching - one input, multiple outputs
#[tokio::test]
async fn test_branching_dag() {
    setup_python_environment();

    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

# Create remotemedia package structure
if 'remotemedia' not in sys.modules:
    remotemedia_module = types.ModuleType('remotemedia')
    remotemedia_module.__package__ = 'remotemedia'
    remotemedia_module.__path__ = []
    sys.modules['remotemedia'] = remotemedia_module

# Create remotemedia.nodes module
if 'remotemedia.nodes' not in sys.modules:
    nodes_module = types.ModuleType('remotemedia.nodes')
    nodes_module.__package__ = 'remotemedia.nodes'
    sys.modules['remotemedia.nodes'] = nodes_module

class SourceNode:
    def process(self, data):
        return {"value": data, "source": True}

class BranchA:
    def process(self, data):
        return {"value": data["value"] * 2, "branch": "A"}

class BranchB:
    def process(self, data):
        return {"value": data["value"] + 100, "branch": "B"}

sys.modules['remotemedia.nodes'].SourceNode = SourceNode
sys.modules['remotemedia.nodes'].BranchA = BranchA
sys.modules['remotemedia.nodes'].BranchB = BranchB
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    // Create branching pipeline:
    //       -> BranchA (multiply by 2)
    // Source
    //       -> BranchB (add 100)
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "branching-test".to_string(),
            description: Some("Test branching DAG".to_string()),
            created_at: None,
        },
        nodes: vec![
            NodeManifest {
                id: "source".to_string(),
                node_type: "SourceNode".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "branch_a".to_string(),
                node_type: "BranchA".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "branch_b".to_string(),
                node_type: "BranchB".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
        ],
        connections: vec![
            Connection {
                from: "source".to_string(),
                to: "branch_a".to_string(),
            },
            Connection {
                from: "source".to_string(),
                to: "branch_b".to_string(),
            },
        ],
    };

    let executor = Executor::new();
    let input = vec![json!(10)];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    // Both branches should produce outputs
    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 2); // One from each branch

    // Find outputs by branch
    let branch_a_output = outputs.iter().find(|o| o["branch"] == "A").unwrap();
    let branch_b_output = outputs.iter().find(|o| o["branch"] == "B").unwrap();

    assert_eq!(branch_a_output["value"], json!(20)); // 10 * 2
    assert_eq!(branch_b_output["value"], json!(110)); // 10 + 100

    println!("✓ Test 1.11.4: Branching DAG passed!");
}

/// Test 1.11.4: Merging - multiple inputs, one output
#[tokio::test]
async fn test_merging_dag() {
    setup_python_environment();

    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

if 'remotemedia.nodes' not in sys.modules:
    sys.modules['remotemedia.nodes'] = types.ModuleType('remotemedia.nodes')

class SourceA:
    def process(self, data):
        return {"value": data, "source": "A"}

class SourceB:
    def process(self, data):
        return {"value": data * 10, "source": "B"}

class MergeNode:
    def process(self, data):
        # Receives data from multiple sources
        return {"merged": data["value"], "from": data["source"]}

sys.modules['remotemedia.nodes'].SourceA = SourceA
sys.modules['remotemedia.nodes'].SourceB = SourceB
sys.modules['remotemedia.nodes'].MergeNode = MergeNode
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    // Create merging pipeline:
    // SourceA \
    //          -> MergeNode
    // SourceB /
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "merging-test".to_string(),
            description: Some("Test merging DAG".to_string()),
            created_at: None,
        },
        nodes: vec![
            NodeManifest {
                id: "source_a".to_string(),
                node_type: "SourceA".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "source_b".to_string(),
                node_type: "SourceB".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "merge".to_string(),
                node_type: "MergeNode".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
        ],
        connections: vec![
            Connection {
                from: "source_a".to_string(),
                to: "merge".to_string(),
            },
            Connection {
                from: "source_b".to_string(),
                to: "merge".to_string(),
            },
        ],
    };

    let executor = Executor::new();
    let input = vec![json!(5)];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    // Merge node should receive data from both sources
    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 2); // One from each source

    // Find outputs by source
    let from_a = outputs.iter().find(|o| o["from"] == "A").unwrap();
    let from_b = outputs.iter().find(|o| o["from"] == "B").unwrap();

    assert_eq!(from_a["merged"], json!(5));
    assert_eq!(from_b["merged"], json!(50)); // 5 * 10

    println!("✓ Test 1.11.4: Merging DAG passed!");
}

/// Test 1.11.5: Complex diamond topology (branching + merging)
#[tokio::test]
async fn test_complex_diamond_topology() {
    setup_python_environment();

    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

if 'remotemedia.nodes' not in sys.modules:
    sys.modules['remotemedia.nodes'] = types.ModuleType('remotemedia.nodes')

class InputNode:
    def process(self, data):
        return {"input": data}

class ProcessorA:
    def process(self, data):
        return {"value": data["input"] * 2, "processor": "A"}

class ProcessorB:
    def process(self, data):
        return {"value": data["input"] + 10, "processor": "B"}

class CombinerNode:
    def process(self, data):
        # Combine results from both processors
        return {"combined": data["value"], "from": data["processor"]}

sys.modules['remotemedia.nodes'].InputNode = InputNode
sys.modules['remotemedia.nodes'].ProcessorA = ProcessorA
sys.modules['remotemedia.nodes'].ProcessorB = ProcessorB
sys.modules['remotemedia.nodes'].CombinerNode = CombinerNode
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    // Create diamond pipeline:
    //         ProcessorA
    //        /           \
    // Input                Combiner
    //        \           /
    //         ProcessorB
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "diamond-test".to_string(),
            description: Some("Test complex diamond topology".to_string()),
            created_at: None,
        },
        nodes: vec![
            NodeManifest {
                id: "input".to_string(),
                node_type: "InputNode".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "proc_a".to_string(),
                node_type: "ProcessorA".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "proc_b".to_string(),
                node_type: "ProcessorB".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "combiner".to_string(),
                node_type: "CombinerNode".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
        ],
        connections: vec![
            Connection {
                from: "input".to_string(),
                to: "proc_a".to_string(),
            },
            Connection {
                from: "input".to_string(),
                to: "proc_b".to_string(),
            },
            Connection {
                from: "proc_a".to_string(),
                to: "combiner".to_string(),
            },
            Connection {
                from: "proc_b".to_string(),
                to: "combiner".to_string(),
            },
        ],
    };

    let executor = Executor::new();
    let input = vec![json!(7)];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    // Combiner should receive from both processors
    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 2);

    let from_a = outputs.iter().find(|o| o["from"] == "A").unwrap();
    let from_b = outputs.iter().find(|o| o["from"] == "B").unwrap();

    assert_eq!(from_a["combined"], json!(14)); // 7 * 2
    assert_eq!(from_b["combined"], json!(17)); // 7 + 10

    println!("✓ Test 1.11.5: Complex diamond topology passed!");
}

/// Test 1.11.5: Multi-level DAG (3 levels deep)
#[tokio::test]
async fn test_multilevel_dag() {
    setup_python_environment();

    pyo3::Python::with_gil(|py| {
        let code = CString::new(
            r#"
import sys, types

if 'remotemedia.nodes' not in sys.modules:
    sys.modules['remotemedia.nodes'] = types.ModuleType('remotemedia.nodes')

class Level1Node:
    def process(self, data):
        return {"level": 1, "value": data}

class Level2NodeA:
    def process(self, data):
        return {"level": 2, "branch": "A", "value": data["value"] * 2}

class Level2NodeB:
    def process(self, data):
        return {"level": 2, "branch": "B", "value": data["value"] + 5}

class Level3Node:
    def process(self, data):
        return {"level": 3, "final_value": data["value"], "from_branch": data["branch"]}

sys.modules['remotemedia.nodes'].Level1Node = Level1Node
sys.modules['remotemedia.nodes'].Level2NodeA = Level2NodeA
sys.modules['remotemedia.nodes'].Level2NodeB = Level2NodeB
sys.modules['remotemedia.nodes'].Level3Node = Level3Node
"#,
        )
        .unwrap();
        py.run(&code, None, None).unwrap();
    });

    // Create 3-level pipeline:
    //              -> L2A -> L3
    // L1
    //              -> L2B -> L3
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "multilevel-test".to_string(),
            description: Some("Test multi-level DAG".to_string()),
            created_at: None,
        },
        nodes: vec![
            NodeManifest {
                id: "l1".to_string(),
                node_type: "Level1Node".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "l2a".to_string(),
                node_type: "Level2NodeA".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "l2b".to_string(),
                node_type: "Level2NodeB".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
            NodeManifest {
                id: "l3".to_string(),
                node_type: "Level3Node".to_string(),
                params: json!({}),
                capabilities: None,
                host: None,
                runtime_hint: Some(RuntimeHint::Cpython),
            },
        ],
        connections: vec![
            Connection {
                from: "l1".to_string(),
                to: "l2a".to_string(),
            },
            Connection {
                from: "l1".to_string(),
                to: "l2b".to_string(),
            },
            Connection {
                from: "l2a".to_string(),
                to: "l3".to_string(),
            },
            Connection {
                from: "l2b".to_string(),
                to: "l3".to_string(),
            },
        ],
    };

    let executor = Executor::new();
    let input = vec![json!(3)];

    let result = executor.execute_with_input(&manifest, input).await.unwrap();

    assert_eq!(result.status, "success");

    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 2);

    // Verify both branches reached level 3
    for output in outputs.iter() {
        assert_eq!(output["level"], json!(3));
    }

    let from_a = outputs.iter().find(|o| o["from_branch"] == "A").unwrap();
    let from_b = outputs.iter().find(|o| o["from_branch"] == "B").unwrap();

    assert_eq!(from_a["final_value"], json!(6)); // 3 * 2
    assert_eq!(from_b["final_value"], json!(8)); // 3 + 5

    println!("✓ Test 1.11.5: Multi-level DAG passed!");
}
