//! Integration tests for RemoteMedia SDK Python nodes in RustPython (Phase 1.6.8)
//!
//! This test suite validates that existing Python SDK nodes can run in RustPython.
//! Tests cover:
//! - Simple processing nodes
//! - Stateful nodes
//! - Nodes with initialization
//! - Nodes with cleanup
//! - Streaming/generator nodes
//! - Async nodes (if supported)

use remotemedia_runtime::python::PythonNodeInstance;
use serde_json::json;

/// Test 1: PassThroughNode - simplest node
#[test]
fn test_passthrough_node() {
    let source_code = r#"
class PassThroughNode:
    """Simple pass-through node"""

    def __init__(self, name=None):
        self.name = name or "PassThrough"

    def process(self, data):
        return data
"#;

    let params = json!({ "name": "TestPassThrough" });
    let mut node = PythonNodeInstance::from_source(source_code, "PassThroughNode", params)
        .expect("Failed to create PassThroughNode");

    // Test processing
    let input = json!({"test": "data", "value": 42});
    let output = node.process(input.clone()).expect("Failed to process");

    assert_eq!(output, input, "PassThroughNode should return input unchanged");

    node.cleanup().expect("Failed to cleanup");
}

/// Test 2: TransformNode - stateful node with parameters
#[test]
fn test_transform_node() {
    let source_code = r#"
import logging

class TransformNode:
    """Node that transforms data with state tracking"""

    def __init__(self, operation="identity", multiplier=1):
        self.operation = operation
        self.multiplier = multiplier
        self.processed_count = 0
        self.logger = logging.getLogger("TransformNode")

    def initialize(self):
        self.logger.info("TransformNode initialized with operation=%s", self.operation)

    def process(self, data):
        self.processed_count += 1
        self.logger.debug("Processing item %d", self.processed_count)

        if self.operation == "double":
            return data * 2
        elif self.operation == "square":
            return data ** 2
        elif self.operation == "multiply":
            return data * self.multiplier
        else:
            return data

    def cleanup(self):
        self.logger.info("Processed %d items", self.processed_count)
"#;

    let params = json!({
        "operation": "multiply",
        "multiplier": 3
    });

    let mut node = PythonNodeInstance::from_source(source_code, "TransformNode", params)
        .expect("Failed to create TransformNode");

    node.initialize().expect("Failed to initialize");

    // Process multiple items - state should be preserved
    let result1 = node.process(json!(5)).expect("Failed to process");
    assert_eq!(result1, json!(15));

    let result2 = node.process(json!(10)).expect("Failed to process");
    assert_eq!(result2, json!(30));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 3: CounterNode - simple state preservation
#[test]
fn test_counter_node() {
    let source_code = r#"
class CounterNode:
    """Node that counts processed items"""

    def __init__(self):
        self.count = 0

    def process(self, data):
        self.count += 1
        return {
            "input": data,
            "count": self.count,
            "is_even": self.count % 2 == 0
        }
"#;

    let mut node = PythonNodeInstance::from_source(source_code, "CounterNode", json!(null))
        .expect("Failed to create CounterNode");

    // Process several items
    let result1 = node.process(json!("first")).expect("Failed to process");
    assert_eq!(result1["count"], json!(1));
    assert_eq!(result1["is_even"], json!(false));

    let result2 = node.process(json!("second")).expect("Failed to process");
    assert_eq!(result2["count"], json!(2));
    assert_eq!(result2["is_even"], json!(true));

    let result3 = node.process(json!("third")).expect("Failed to process");
    assert_eq!(result3["count"], json!(3));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 4: FilterNode - conditional processing
#[test]
fn test_filter_node() {
    let source_code = r#"
class FilterNode:
    """Node that filters data based on condition"""

    def __init__(self, min_value=0, max_value=100):
        self.min_value = min_value
        self.max_value = max_value
        self.filtered_count = 0
        self.passed_count = 0

    def process(self, data):
        if isinstance(data, (int, float)):
            if self.min_value <= data <= self.max_value:
                self.passed_count += 1
                return {"value": data, "passed": True}
            else:
                self.filtered_count += 1
                return {"value": data, "passed": False}
        else:
            return {"value": data, "passed": True}
"#;

    let params = json!({
        "min_value": 10,
        "max_value": 50
    });

    let mut node = PythonNodeInstance::from_source(source_code, "FilterNode", params)
        .expect("Failed to create FilterNode");

    // Test filtering
    let result1 = node.process(json!(25)).expect("Failed to process");
    assert_eq!(result1["passed"], json!(true));

    let result2 = node.process(json!(5)).expect("Failed to process");
    assert_eq!(result2["passed"], json!(false));

    let result3 = node.process(json!(100)).expect("Failed to process");
    assert_eq!(result3["passed"], json!(false));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 5: AccumulatorNode - complex state management
#[test]
fn test_accumulator_node() {
    let source_code = r#"
class AccumulatorNode:
    """Node that accumulates values"""

    def __init__(self, window_size=5):
        self.window_size = window_size
        self.values = []
        self.total = 0

    def process(self, data):
        # Add new value
        self.values.append(data)
        self.total += data

        # Maintain window
        if len(self.values) > self.window_size:
            removed = self.values.pop(0)
            self.total -= removed

        return {
            "current": data,
            "sum": self.total,
            "average": self.total / len(self.values),
            "count": len(self.values)
        }
"#;

    let params = json!({ "window_size": 3 });

    let mut node = PythonNodeInstance::from_source(source_code, "AccumulatorNode", params)
        .expect("Failed to create AccumulatorNode");

    // Process values
    let result1 = node.process(json!(10)).expect("Failed to process");
    assert_eq!(result1["sum"], json!(10));
    assert_eq!(result1["count"], json!(1));

    let result2 = node.process(json!(20)).expect("Failed to process");
    assert_eq!(result2["sum"], json!(30));
    assert_eq!(result2["count"], json!(2));

    let result3 = node.process(json!(30)).expect("Failed to process");
    assert_eq!(result3["sum"], json!(60));
    assert_eq!(result3["count"], json!(3));

    // This should remove the first value (10)
    let result4 = node.process(json!(40)).expect("Failed to process");
    assert_eq!(result4["sum"], json!(90)); // 20 + 30 + 40
    assert_eq!(result4["count"], json!(3));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 6: DataSourceNode simulation - streaming node
#[test]
fn test_streaming_node() {
    let source_code = r#"
class StreamingNode:
    """Node that generates streaming output"""

    def __init__(self, count=5):
        self.count = count

    def process(self, data):
        """Generator function that yields multiple values"""
        for i in range(self.count):
            yield {
                "index": i,
                "input": data,
                "value": data * i
            }
"#;

    let params = json!({ "count": 3 });

    let mut node = PythonNodeInstance::from_source(source_code, "StreamingNode", params)
        .expect("Failed to create StreamingNode");

    // Test streaming
    let results = node.process_streaming(json!(10))
        .expect("Failed to process streaming");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0]["index"], json!(0));
    assert_eq!(results[1]["index"], json!(1));
    assert_eq!(results[2]["index"], json!(2));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 7: EchoNode - RemoteMedia SDK pattern
#[test]
fn test_echo_node() {
    let source_code = r#"
import logging

class EchoNode:
    """Echo node that wraps input in metadata"""

    def __init__(self, name="Echo"):
        self.name = name
        self.counter = 0
        self.logger = logging.getLogger("EchoNode")

    def initialize(self):
        self.counter = 0
        self.logger.info("EchoNode '%s' initialized", self.name)

    def process(self, data):
        self.counter += 1
        self.logger.debug("Processing item %d", self.counter)

        return {
            "input": data,
            "counter": self.counter,
            "node": self.name
        }

    def cleanup(self):
        self.logger.info("EchoNode processed %d items", self.counter)
"#;

    let params = json!({ "name": "TestEcho" });

    let mut node = PythonNodeInstance::from_source(source_code, "EchoNode", params)
        .expect("Failed to create EchoNode");

    node.initialize().expect("Failed to initialize");

    let result = node.process(json!("hello")).expect("Failed to process");
    assert_eq!(result["input"], json!("hello"));
    assert_eq!(result["counter"], json!(1));
    assert_eq!(result["node"], json!("TestEcho"));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 8: CalculatorNode - parametrized operations
#[test]
fn test_calculator_node() {
    let source_code = r#"
class CalculatorNode:
    """Basic calculator node"""

    def __init__(self, operation="add", operand=0):
        self.operation = operation
        self.operand = operand

    def process(self, data):
        if not isinstance(data, (int, float)):
            return {"error": "Invalid input type"}

        if self.operation == "add":
            result = data + self.operand
        elif self.operation == "subtract":
            result = data - self.operand
        elif self.operation == "multiply":
            result = data * self.operand
        elif self.operation == "divide":
            if self.operand == 0:
                return {"error": "Division by zero"}
            result = data / self.operand
        else:
            result = data

        return result
"#;

    let params = json!({
        "operation": "multiply",
        "operand": 5
    });

    let mut node = PythonNodeInstance::from_source(source_code, "CalculatorNode", params)
        .expect("Failed to create CalculatorNode");

    let result = node.process(json!(7)).expect("Failed to process");
    assert_eq!(result, json!(35));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 9: Node with complex data types
#[test]
fn test_complex_data_node() {
    let source_code = r#"
class ComplexDataNode:
    """Node that handles complex data structures"""

    def __init__(self):
        self.schema = {
            "name": str,
            "age": int,
            "active": bool
        }

    def process(self, data):
        if not isinstance(data, dict):
            return {"error": "Expected dict input"}

        # Validate and transform
        result = {
            "validated": True,
            "data": data,
            "has_name": "name" in data,
            "has_age": "age" in data,
            "metadata": {
                "keys": list(data.keys()),
                "count": len(data)
            }
        }

        return result
"#;

    let mut node = PythonNodeInstance::from_source(source_code, "ComplexDataNode", json!(null))
        .expect("Failed to create ComplexDataNode");

    let input = json!({
        "name": "Alice",
        "age": 30,
        "active": true
    });

    let result = node.process(input).expect("Failed to process");
    assert_eq!(result["validated"], json!(true));
    assert_eq!(result["has_name"], json!(true));
    assert_eq!(result["has_age"], json!(true));
    assert_eq!(result["metadata"]["count"], json!(3));

    node.cleanup().expect("Failed to cleanup");
}

/// Test 10: Node with list processing
#[test]
fn test_list_processing_node() {
    let source_code = r#"
class ListProcessingNode:
    """Node that processes lists"""

    def __init__(self, operation="sum"):
        self.operation = operation

    def process(self, data):
        if not isinstance(data, list):
            return {"error": "Expected list input"}

        if self.operation == "sum":
            result = sum(data)
        elif self.operation == "average":
            result = sum(data) / len(data) if data else 0
        elif self.operation == "min":
            result = min(data) if data else None
        elif self.operation == "max":
            result = max(data) if data else None
        elif self.operation == "count":
            result = len(data)
        else:
            result = data

        return {
            "operation": self.operation,
            "result": result,
            "input_length": len(data)
        }
"#;

    let params = json!({ "operation": "average" });

    let mut node = PythonNodeInstance::from_source(source_code, "ListProcessingNode", params)
        .expect("Failed to create ListProcessingNode");

    let input = json!([10, 20, 30, 40, 50]);
    let result = node.process(input).expect("Failed to process");

    assert_eq!(result["operation"], json!("average"));
    assert_eq!(result["result"], json!(30.0));
    assert_eq!(result["input_length"], json!(5));

    node.cleanup().expect("Failed to cleanup");
}

/// Integration test: Chain multiple nodes
#[test]
fn test_node_chaining() {
    // Create a simple pipeline by manually chaining nodes

    // Node 1: Double the input
    let source1 = r#"
class DoubleNode:
    def __init__(self):
        pass

    def process(self, data):
        return data * 2
"#;

    // Node 2: Add 10
    let source2 = r#"
class AddNode:
    def __init__(self, value=10):
        self.value = value

    def process(self, data):
        return data + self.value
"#;

    // Node 3: Format output
    let source3 = r#"
class FormatNode:
    def __init__(self):
        pass

    def process(self, data):
        return {"result": data, "formatted": f"The result is {data}"}
"#;

    let mut node1 = PythonNodeInstance::from_source(source1, "DoubleNode", json!(null))
        .expect("Failed to create DoubleNode");

    let mut node2 = PythonNodeInstance::from_source(source2, "AddNode", json!({"value": 10}))
        .expect("Failed to create AddNode");

    let mut node3 = PythonNodeInstance::from_source(source3, "FormatNode", json!(null))
        .expect("Failed to create FormatNode");

    // Process: 5 -> 10 -> 20 -> {"result": 20, ...}
    let input = json!(5);
    let result1 = node1.process(input).expect("Failed at node1");
    assert_eq!(result1, json!(10));

    let result2 = node2.process(result1).expect("Failed at node2");
    assert_eq!(result2, json!(20));

    let result3 = node3.process(result2).expect("Failed at node3");
    assert_eq!(result3["result"], json!(20));

    node1.cleanup().expect("Failed to cleanup node1");
    node2.cleanup().expect("Failed to cleanup node2");
    node3.cleanup().expect("Failed to cleanup node3");
}
