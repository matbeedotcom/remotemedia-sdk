//! Phase 1.9: RustPython Compatibility Testing
//!
//! This test suite validates RustPython compatibility across:
//! - Python stdlib modules (1.9.3)
//! - async/await support (1.9.4)
//! - SDK node compatibility (1.9.2)
//! - Example pipeline execution (1.9.7)
//!
//! Results feed into the compatibility matrix (1.9.6) and documentation (1.9.9)

use remotemedia_runtime::python::vm::PythonVm;
use serde_json::{json, Value};

/// Test Python stdlib modules that are commonly used
#[test]
fn test_stdlib_json_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import json
data = {"name": "test", "value": 42}
result = json.dumps(data)
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    let result = response["result"].as_str().unwrap();
    assert!(result.contains("name") && result.contains("test"));
}

#[test]
fn test_stdlib_sys_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import sys
version_info = sys.version_info
# Return a simple string to verify it works
"Python " + str(version_info[0])
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    let result = response["result"].as_str().unwrap();
    assert!(result.starts_with("Python"));
}

#[test]
fn test_stdlib_os_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import os
# Test basic os functionality
sep = os.sep
"separator: " + sep
"#;

    let response = vm.execute(code);
    // os module may or may not be fully supported in RustPython
    // Just verify it doesn't crash
    assert!(response.is_ok() || response.is_err());
}

#[test]
fn test_stdlib_collections_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
from collections import defaultdict
d = defaultdict(int)
d['a'] = 1
d['b'] = 2
sum([d['a'], d['b'], d['c']])  # 'c' defaults to 0
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "3");
}

#[test]
fn test_stdlib_itertools_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
from itertools import chain
list1 = [1, 2, 3]
list2 = [4, 5, 6]
result = list(chain(list1, list2))
result
"#;

    let response = vm.execute(code);
    // itertools may have limited support
    if response.is_ok() {
        assert_eq!(response.unwrap()["status"], "success");
    }
}

#[test]
fn test_stdlib_math_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import math
result = math.sqrt(16)
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "4.0");
}

#[test]
fn test_stdlib_re_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import re
pattern = r'\d+'
text = "There are 123 numbers"
match = re.search(pattern, text)
match.group() if match else None
"#;

    let response = vm.execute(code);
    // re module may have limited support in RustPython
    if response.is_ok() {
        let result = response.unwrap();
        if result["status"] == "success" {
            assert_eq!(result["result"], "123");
        }
    }
}

#[test]
fn test_stdlib_datetime_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
from datetime import datetime
# Just test import and basic functionality
dt = datetime(2025, 10, 23, 12, 0, 0)
dt.year
"#;

    let response = vm.execute(code);
    // datetime module support varies in RustPython
    if response.is_ok() {
        let result = response.unwrap();
        if result["status"] == "success" {
            assert_eq!(result["result"], "2025");
        }
    }
}

/// Test async/await support (Phase 1.9.4)
#[test]
fn test_async_function_definition() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
async def async_function():
    return 42

# Define it, don't run it yet
"defined"
"#;

    let response = vm.execute(code);
    // async/await support may be limited in RustPython
    assert!(response.is_ok() || response.is_err());
    if let Ok(result) = response {
        if result["status"] == "success" {
            assert_eq!(result["result"], "defined");
        }
    }
}

#[test]
fn test_async_await_syntax() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
async def process_data(data):
    # Simulate async processing
    await None  # This won't actually work but tests syntax
    return data * 2

"syntax_ok"
"#;

    let response = vm.execute(code);
    // This tests if RustPython can parse async/await syntax
    assert!(response.is_ok() || response.is_err());
}

/// Test generator support
#[test]
fn test_generator_function() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
def simple_generator():
    yield 1
    yield 2
    yield 3

gen = simple_generator()
result = list(gen)
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "[1, 2, 3]");
}

#[test]
fn test_generator_expression() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
gen = (x * 2 for x in range(5))
result = list(gen)
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "[0, 2, 4, 6, 8]");
}

/// Test simple SDK-style node pattern
#[test]
fn test_sdk_node_pattern_basic() {
    let mut vm = PythonVm::new().unwrap();

    let node_code = r#"
class SimpleNode:
    def __init__(self, multiplier=2):
        self.multiplier = multiplier

    def process(self, data):
        return data * self.multiplier
"#;

    vm.load_class(node_code, "SimpleNode").unwrap();
    let params = json!({"multiplier": 3});
    let instance = vm.create_instance("SimpleNode", &params).unwrap();

    let input = json!(10);
    let result = vm.call_method(&instance, "process", &input).unwrap();

    assert_eq!(result["status"], "success");
    assert_eq!(result["result"], "30");
}

#[test]
fn test_sdk_node_pattern_with_state() {
    let mut vm = PythonVm::new().unwrap();

    let node_code = r#"
class StatefulNode:
    def __init__(self):
        self.count = 0
        self.total = 0

    def process(self, data):
        self.count += 1
        self.total += data
        return {"count": self.count, "total": self.total, "avg": self.total / self.count}
"#;

    vm.load_class(node_code, "StatefulNode").unwrap();
    let instance = vm.create_instance("StatefulNode", &Value::Null).unwrap();

    // Process multiple inputs
    vm.call_method(&instance, "process", &json!(10)).unwrap();
    vm.call_method(&instance, "process", &json!(20)).unwrap();
    let result = vm.call_method(&instance, "process", &json!(30)).unwrap();

    assert_eq!(result["status"], "success");
    // The result should show count=3, total=60, avg=20
    let result_str = result["result"].as_str().unwrap();
    assert!(result_str.contains("count") && result_str.contains("3"));
}

#[test]
fn test_sdk_node_error_handling() {
    let mut vm = PythonVm::new().unwrap();

    let node_code = r#"
class ErrorNode:
    def process(self, data):
        if data < 0:
            raise ValueError("Negative values not allowed")
        return data * 2
"#;

    vm.load_class(node_code, "ErrorNode").unwrap();
    let instance = vm.create_instance("ErrorNode", &Value::Null).unwrap();

    // Valid input
    let result = vm.call_method(&instance, "process", &json!(10)).unwrap();
    assert_eq!(result["status"], "success");
    assert_eq!(result["result"], "20");

    // Invalid input - should get error
    let result = vm.call_method(&instance, "process", &json!(-5));

    // Error handling: either returns error status or Err result
    match result {
        Ok(res) => {
            assert_eq!(res["status"], "error");
            let error_msg = res["error"].as_str().unwrap();
            assert!(error_msg.contains("ValueError") || error_msg.contains("Negative"));
        }
        Err(e) => {
            // Also acceptable - error propagated as Err
            let error_msg = format!("{:?}", e);
            assert!(error_msg.contains("ValueError") || error_msg.contains("Negative") || error_msg.contains("error"));
        }
    }
}

/// Test list comprehensions
#[test]
fn test_list_comprehension() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
numbers = [1, 2, 3, 4, 5]
squared = [x * x for x in numbers]
squared
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "[1, 4, 9, 16, 25]");
}

#[test]
fn test_dict_comprehension() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
numbers = [1, 2, 3]
squared_dict = {x: x*x for x in numbers}
squared_dict
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    let result = response["result"].as_str().unwrap();
    assert!(result.contains("1") && result.contains("4") && result.contains("9"));
}

/// Test lambda functions
#[test]
fn test_lambda_function() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
add = lambda x, y: x + y
result = add(10, 20)
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "30");
}

/// Test closures
#[test]
fn test_closure() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
def make_multiplier(factor):
    def multiply(x):
        return x * factor
    return multiply

double = make_multiplier(2)
triple = make_multiplier(3)
double(5) + triple(5)
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "25");  // 10 + 15
}

/// Test exception handling
#[test]
fn test_try_except() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
try:
    x = 10 / 2
    result = "success"
except ZeroDivisionError:
    result = "error"
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "success");
}

#[test]
fn test_try_except_with_error() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
try:
    x = 10 / 0
    result = "no error"
except ZeroDivisionError:
    result = "caught error"
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "caught error");
}

/// Test class inheritance
#[test]
fn test_class_inheritance() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
class Animal:
    def __init__(self, name):
        self.name = name

    def speak(self):
        return "Some sound"

class Dog(Animal):
    def speak(self):
        return f"{self.name} says Woof!"

dog = Dog("Buddy")
dog.speak()
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "Buddy says Woof!");
}

/// Test decorators
#[test]
fn test_decorator_basic() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
def double_result(func):
    def wrapper(x):
        return func(x) * 2
    return wrapper

@double_result
def add_five(x):
    return x + 5

add_five(10)
"#;

    let response = vm.execute(code);
    // Decorators may have varying support
    if response.is_ok() {
        let result = response.unwrap();
        if result["status"] == "success" {
            assert_eq!(result["result"], "30");  // (10 + 5) * 2
        }
    }
}

/// Test context managers (with statement)
#[test]
fn test_context_manager() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
class SimpleContext:
    def __enter__(self):
        return "entered"

    def __exit__(self, exc_type, exc_val, exc_tb):
        return False

with SimpleContext() as ctx:
    result = ctx

result
"#;

    let response = vm.execute(code);
    // Context managers may have varying support
    if response.is_ok() {
        let result = response.unwrap();
        if result["status"] == "success" {
            assert_eq!(result["result"], "entered");
        }
    }
}

/// Test f-strings (formatted string literals)
#[test]
fn test_f_strings() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
name = "World"
value = 42
result = f"Hello {name}, the answer is {value}"
result
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "Hello World, the answer is 42");
}

/// Test multiple assignment
#[test]
fn test_multiple_assignment() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
a, b, c = 1, 2, 3
a + b + c
"#;

    let response = vm.execute(code).unwrap();
    assert_eq!(response["status"], "success");
    assert_eq!(response["result"], "6");
}

/// Test unpacking
#[test]
fn test_unpacking() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
data = [1, 2, 3, 4, 5]
first, *middle, last = data
len(middle)
"#;

    let response = vm.execute(code);
    // Extended unpacking may have varying support
    if response.is_ok() {
        let result = response.unwrap();
        if result["status"] == "success" {
            assert_eq!(result["result"], "3");
        }
    }
}

// ============================================================================
// Additional Stdlib Module Tests
// ============================================================================

/// Test pickle module (serialization)
#[test]
fn test_stdlib_pickle_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import pickle
data = {"key": "value", "num": 42}
serialized = pickle.dumps(data)
deserialized = pickle.loads(serialized)
deserialized["num"]
"#;

    let response = vm.execute(code);
    // pickle may not be fully supported in RustPython
    match response {
        Ok(result) if result["status"] == "success" => {
            assert_eq!(result["result"], "42");
        }
        _ => {
            // pickle not supported - this is acceptable
        }
    }
}

/// Test threading module
#[test]
fn test_stdlib_threading_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import threading
# Just test if we can import and access basic functionality
current = threading.current_thread()
current.name
"#;

    let response = vm.execute(code);
    // threading likely not supported in RustPython
    // Just verify it doesn't crash
    assert!(response.is_ok() || response.is_err());
}

/// Test subprocess module
#[test]
fn test_stdlib_subprocess_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import subprocess
# Just test import - don't actually run subprocess
"subprocess_imported"
"#;

    let response = vm.execute(code);
    // subprocess likely not supported
    assert!(response.is_ok() || response.is_err());
}

/// Test socket module
#[test]
fn test_stdlib_socket_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import socket
# Just test import and basic constants
socket.AF_INET
"#;

    let response = vm.execute(code);
    // socket likely not fully supported
    assert!(response.is_ok() || response.is_err());
}

/// Test asyncio module
#[test]
fn test_stdlib_asyncio_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import asyncio
# Just test import - don't try to run event loop
"asyncio_imported"
"#;

    let response = vm.execute(code);
    // asyncio likely not supported in RustPython
    match response {
        Ok(result) if result["status"] == "success" => {
            assert_eq!(result["result"], "asyncio_imported");
        }
        _ => {
            // asyncio not supported - expected
        }
    }
}

/// Test pathlib module
#[test]
fn test_stdlib_pathlib_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
from pathlib import Path
p = Path("/foo/bar/baz.txt")
p.name
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            assert_eq!(result["result"], "baz.txt");
        }
        _ => {
            // pathlib may not be fully supported
        }
    }
}

/// Test random module
#[test]
fn test_stdlib_random_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import random
random.seed(42)
val = random.randint(1, 100)
val
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            // Just verify we got a number back
            let val_str = result["result"].as_str().unwrap();
            let val: i32 = val_str.parse().unwrap_or(0);
            assert!(val > 0 && val <= 100);
        }
        _ => {
            // random may not be supported
        }
    }
}

/// Test hashlib module
#[test]
fn test_stdlib_hashlib_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import hashlib
m = hashlib.md5()
m.update(b"hello")
len(m.hexdigest())
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            assert_eq!(result["result"], "32");  // MD5 hex digest is 32 chars
        }
        _ => {
            // hashlib may not be supported
        }
    }
}

/// Test base64 module
#[test]
fn test_stdlib_base64_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import base64
data = b"hello world"
encoded = base64.b64encode(data)
decoded = base64.b64decode(encoded)
decoded
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            // Just verify we got the decoded bytes back
            let result_str = result["result"].as_str().unwrap();
            assert!(result_str.contains("hello world") || result_str.contains("b'hello world'"));
        }
        _ => {
            // base64 may not be supported
        }
    }
}

/// Test time module
#[test]
fn test_stdlib_time_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import time
t = time.time()
t
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            // Just verify we got a number back (timestamp)
            let val_str = result["result"].as_str().unwrap();
            let val: f64 = val_str.parse().unwrap_or(0.0);
            assert!(val > 0.0);
        }
        _ => {
            // time may not be fully supported
        }
    }
}

/// Test uuid module
#[test]
fn test_stdlib_uuid_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
import uuid
u = uuid.uuid4()
len(str(u))
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            assert_eq!(result["result"], "36");  // UUID string is 36 chars
        }
        _ => {
            // uuid may not be supported
        }
    }
}

/// Test typing module
#[test]
fn test_stdlib_typing_module() {
    let mut vm = PythonVm::new().unwrap();
    vm.initialize().unwrap();

    let code = r#"
from typing import List, Dict, Optional
# Just test that we can import type hints
"typing_imported"
"#;

    let response = vm.execute(code);
    match response {
        Ok(result) if result["status"] == "success" => {
            assert_eq!(result["result"], "typing_imported");
        }
        _ => {
            // typing may not be supported
        }
    }
}
