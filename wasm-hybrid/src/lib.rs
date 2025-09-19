use serde::{Deserialize, Serialize};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[derive(Serialize, Deserialize, Debug)]
pub struct MathInput {
    pub data: Vec<f64>,
    pub operations: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MathOutput {
    pub data: Vec<f64>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Fast math operations implemented in WASM
pub fn process_math_operations(input: &MathInput) -> MathOutput {
    let mut result_data = input.data.clone();
    let mut metadata = std::collections::HashMap::new();

    for operation in &input.operations {
        match operation.as_str() {
            "square" => {
                result_data = result_data.iter().map(|x| x * x).collect();
                metadata.insert("last_operation".to_string(), "square".to_string());
            },
            "double" => {
                result_data = result_data.iter().map(|x| x * 2.0).collect();
                metadata.insert("last_operation".to_string(), "double".to_string());
            },
            "sqrt" => {
                result_data = result_data.iter().map(|x| x.sqrt()).collect();
                metadata.insert("last_operation".to_string(), "sqrt".to_string());
            },
            "abs" => {
                result_data = result_data.iter().map(|x| x.abs()).collect();
                metadata.insert("last_operation".to_string(), "abs".to_string());
            },
            _ => {
                metadata.insert("warning".to_string(), format!("Unknown operation: {}", operation));
            }
        }
    }

    metadata.insert("processed_count".to_string(), result_data.len().to_string());
    metadata.insert("wasm_processed".to_string(), "true".to_string());

    MathOutput {
        data: result_data,
        metadata,
    }
}

/// C-compatible interface for WASM export
#[no_mangle]
pub extern "C" fn process_math_json(input_ptr: *const c_char) -> *mut c_char {
    let input_cstr = unsafe { CStr::from_ptr(input_ptr) };
    let input_str = match input_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let input: MathInput = match serde_json::from_str(input_str) {
        Ok(input) => input,
        Err(_) => return std::ptr::null_mut(),
    };

    let output = process_math_operations(&input);

    let output_json = match serde_json::to_string(&output) {
        Ok(json) => json,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(output_json) {
        Ok(cstring) => cstring.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Memory deallocation function for the host
#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_operations() {
        let input = MathInput {
            data: vec![1.0, 2.0, 3.0, 4.0],
            operations: vec!["square".to_string(), "double".to_string()],
        };

        let output = process_math_operations(&input);

        // After square: [1, 4, 9, 16], then double: [2, 8, 18, 32]
        assert_eq!(output.data, vec![2.0, 8.0, 18.0, 32.0]);
        assert_eq!(output.metadata.get("wasm_processed"), Some(&"true".to_string()));
    }
}