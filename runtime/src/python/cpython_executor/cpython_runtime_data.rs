//! RuntimeData conversion utilities for CPython nodes
//!
//! Handles conversion between Python RuntimeData objects and Rust RuntimeData.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::data::RuntimeData;
use crate::{Error, Result};

/// Extract RuntimeData from a Python object
pub fn extract_runtime_data(py: Python, py_obj: &Bound<'_, PyAny>) -> Result<RuntimeData> {
    // Check if this is a dict with "_audio_numpy" key (TTS node workaround)
    if py_obj.is_instance_of::<PyDict>() {
        if let Ok(dict) = py_obj.downcast::<PyDict>() {
            if let Ok(Some(_audio_numpy)) = dict.get_item("_audio_numpy") {
                return extract_audio_from_numpy_dict(py, dict);
            }
        }
    }

    // Try to extract as PyRuntimeData object first
    if let Ok(data_type) = py_obj
        .call_method0("data_type")
        .and_then(|v| v.extract::<String>())
    {
        // This is a PyRuntimeData object with proper methods
        return match data_type.as_str() {
            "text" => extract_text(py_obj),
            "audio" => extract_audio(py_obj),
            "json" => extract_json(py, py_obj),
            "binary" => extract_binary(py_obj),
            _ => Err(Error::Execution(format!(
                "Unsupported RuntimeData type: {}",
                data_type
            ))),
        };
    }

    // Fallback: Convert raw Python objects to RuntimeData
    // This allows Python nodes to yield plain dicts, strings, etc. without wrapping them
    if py_obj.is_instance_of::<pyo3::types::PyString>() {
        // String -> Text
        let text: String = py_obj
            .extract()
            .map_err(|e| Error::Execution(format!("Failed to extract string: {}", e)))?;
        Ok(RuntimeData::Text(text))
    } else if py_obj.is_instance_of::<PyDict>() || py_obj.is_instance_of::<pyo3::types::PyList>() {
        // Dict or List -> JSON
        let json_module = py
            .import("json")
            .map_err(|e| Error::Execution(format!("Failed to import json: {}", e)))?;
        let json_str: String = json_module
            .call_method1("dumps", (py_obj,))
            .and_then(|v| v.extract())
            .map_err(|e| Error::Execution(format!("Failed to serialize to JSON: {}", e)))?;
        let json_value: serde_json::Value = serde_json::from_str(&json_str)?;
        Ok(RuntimeData::Json(json_value))
    } else if py_obj.is_instance_of::<pyo3::types::PyBytes>() {
        // Bytes -> Binary
        let bytes: Vec<u8> = py_obj
            .extract()
            .map_err(|e| Error::Execution(format!("Failed to extract bytes: {}", e)))?;
        Ok(RuntimeData::Binary(prost::bytes::Bytes::from(bytes)))
    } else if let Ok(num) = py_obj.extract::<i64>() {
        // Integer -> JSON
        Ok(RuntimeData::Json(serde_json::Value::Number(num.into())))
    } else if let Ok(num) = py_obj.extract::<f64>() {
        // Float -> JSON
        let json_num = serde_json::Number::from_f64(num)
            .ok_or_else(|| Error::Execution(format!("Invalid float value: {}", num)))?;
        Ok(RuntimeData::Json(serde_json::Value::Number(json_num)))
    } else {
        let type_name = py_obj
            .get_type()
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        Err(Error::Execution(format!(
            "Cannot convert Python object to RuntimeData: {}",
            type_name
        )))
    }
}

fn extract_text(py_obj: &Bound<'_, PyAny>) -> Result<RuntimeData> {
    let text_opt: Option<String> = py_obj
        .call_method0("as_text")
        .and_then(|v| v.extract())
        .map_err(|e| Error::Execution(format!("Failed to extract text: {}", e)))?;

    let text = text_opt.ok_or_else(|| Error::Execution("as_text() returned None".to_string()))?;

    Ok(RuntimeData::Text(text))
}

fn extract_audio(py_obj: &Bound<'_, PyAny>) -> Result<RuntimeData> {
    tracing::info!("extract_audio: Starting audio extraction from Python object");

    let audio_tuple_opt = py_obj.call_method0("as_audio").map_err(|e| {
        tracing::error!("extract_audio: Failed to call as_audio(): {}", e);
        Error::Execution(format!("Failed to call as_audio(): {}", e))
    })?;

    tracing::info!("extract_audio: as_audio() called successfully, extracting tuple...");

    let (samples, sample_rate, channels, format_str, num_samples): (
        Vec<u8>,
        u32,
        u32,
        String,
        u64,
    ) = audio_tuple_opt.extract().map_err(|e| {
        tracing::error!("extract_audio: Failed to extract audio tuple: {}", e);
        Error::Execution(format!("Failed to extract audio tuple: {}", e))
    })?;

    tracing::info!(
        "extract_audio: Extracted audio - {} samples, {}Hz, {} channels, format: {}, bytes: {}",
        num_samples,
        sample_rate,
        channels,
        format_str,
        samples.len()
    );

    let format = match format_str.as_str() {
        "f32" => 0,
        "i16" => 1,
        "i32" => 2,
        _ => 0,
    };

    let audio_buffer = crate::grpc_service::generated::AudioBuffer {
        samples,
        sample_rate,
        channels,
        format,
        num_samples,
    };

    tracing::info!("extract_audio: Created AudioBuffer successfully");

    Ok(RuntimeData::Audio(audio_buffer))
}

fn extract_json(py: Python, py_obj: &Bound<'_, PyAny>) -> Result<RuntimeData> {
    let inner = py_obj
        .getattr("inner")
        .map_err(|e| Error::Execution(format!("Failed to get inner: {}", e)))?;

    let json_module = py
        .import("json")
        .map_err(|e| Error::Execution(format!("Failed to import json: {}", e)))?;

    let json_str: String = json_module
        .call_method1("dumps", (inner,))
        .and_then(|v| v.extract())
        .map_err(|e| Error::Execution(format!("Failed to serialize JSON: {}", e)))?;

    let json_value: serde_json::Value = serde_json::from_str(&json_str)?;

    Ok(RuntimeData::Json(json_value))
}

fn extract_binary(py_obj: &Bound<'_, PyAny>) -> Result<RuntimeData> {
    let inner = py_obj
        .getattr("inner")
        .map_err(|e| Error::Execution(format!("Failed to get inner: {}", e)))?;

    let bytes: Vec<u8> = inner
        .extract()
        .map_err(|e| Error::Execution(format!("Failed to extract bytes: {}", e)))?;

    Ok(RuntimeData::Binary(prost::bytes::Bytes::from(bytes)))
}

fn extract_audio_from_numpy_dict(py: Python, dict: &Bound<'_, PyDict>) -> Result<RuntimeData> {
    let audio_numpy = dict
        .get_item("_audio_numpy")
        .ok()
        .flatten()
        .ok_or_else(|| Error::Execution("Missing _audio_numpy in dict".to_string()))?;

    let sample_rate = dict
        .get_item("_sample_rate")
        .ok()
        .flatten()
        .and_then(|v| v.extract::<u32>().ok())
        .ok_or_else(|| Error::Execution("Missing _sample_rate in dict".to_string()))?;

    let channels = dict
        .get_item("_channels")
        .ok()
        .flatten()
        .and_then(|v| v.extract::<u32>().ok())
        .ok_or_else(|| Error::Execution("Missing _channels in dict".to_string()))?;

    // Call numpy_to_audio
    use crate::python::runtime_data_py::PyRuntimeData;
    let numpy_to_audio_fn = py
        .import("remotemedia_runtime.runtime_data")
        .and_then(|m| m.getattr("numpy_to_audio"))
        .map_err(|e| Error::Execution(format!("Failed to import numpy_to_audio: {}", e)))?;

    let py_runtime_data_obj = numpy_to_audio_fn
        .call1((audio_numpy, sample_rate, channels))
        .map_err(|e| Error::Execution(format!("Failed to call numpy_to_audio: {}", e)))?;

    let py_runtime_data = py_runtime_data_obj
        .extract::<PyRuntimeData>()
        .map_err(|e| {
            Error::Execution(format!(
                "Failed to extract PyRuntimeData from numpy_to_audio: {}",
                e
            ))
        })?;

    Ok(py_runtime_data.inner)
}
