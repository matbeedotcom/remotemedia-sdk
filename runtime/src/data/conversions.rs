// Conversion functions: Protobuf ↔ RuntimeData
// Feature: 004-generic-streaming

use crate::data::RuntimeData;
use crate::data::validation::{validate_video_frame, validate_tensor_size, validate_text_buffer};
use crate::grpc_service::generated::{DataBuffer, data_buffer, AudioBuffer, VideoFrame, TensorBuffer, JsonData, TextBuffer, BinaryBuffer};
use crate::Error;
use prost::bytes::Bytes;
use std::time::Instant;

/// Convert protobuf DataBuffer to runtime representation
///
/// This function:
/// 1. Validates the data type variant is set
/// 2. Performs data-specific validation (video frame size, tensor dimensions, JSON parsing, UTF-8)
/// 3. Converts to RuntimeData enum
///
/// Returns error if:
/// - No oneof variant is set
/// - JSON parsing fails
/// - UTF-8 validation fails
/// - Video frame size mismatch
/// - Tensor size mismatch
pub fn convert_proto_to_runtime_data(proto: DataBuffer) -> Result<RuntimeData, Error> {
    let start = Instant::now();

    let result = match proto.data_type {
        Some(data_buffer::DataType::Audio(buf)) => {
            Ok(RuntimeData::Audio(buf))
        },
        Some(data_buffer::DataType::Video(frame)) => {
            // Validate video frame dimensions
            validate_video_frame(&frame)?;
            Ok(RuntimeData::Video(frame))
        },
        Some(data_buffer::DataType::Tensor(tensor)) => {
            // Validate tensor size matches shape * dtype
            validate_tensor_size(&tensor)?;
            Ok(RuntimeData::Tensor(tensor))
        },
        Some(data_buffer::DataType::Json(json_data)) => {
            // Parse JSON string into serde_json::Value
            let value = serde_json::from_str(&json_data.json_payload)
                .map_err(|e| Error::InvalidInput {
                    message: format!("JSON parsing failed at line {}, column {}: {}",
                        e.line(), e.column(), e),
                    node_id: String::new(),
                    context: json_data.schema_type.clone(),
                })?;
            Ok(RuntimeData::Json(value))
        },
        Some(data_buffer::DataType::Text(text_buf)) => {
            // Validate UTF-8 encoding
            let text = validate_text_buffer(&text_buf)?;
            Ok(RuntimeData::Text(text))
        },
        Some(data_buffer::DataType::Binary(bin)) => {
            Ok(RuntimeData::Binary(Bytes::from(bin.data)))
        },
        None => {
            Err(Error::InvalidInput {
                message: "DataBuffer has no data_type variant set".into(),
                node_id: String::new(),
                context: "Expected exactly one of: audio, video, tensor, json, text, binary".into(),
            })
        },
    };

    let elapsed = start.elapsed();
    tracing::trace!(
        "Proto to runtime conversion took {}µs for {:?}",
        elapsed.as_micros(),
        result.as_ref().map(|r| r.type_name()).unwrap_or("error")
    );

    result
}

/// Convert runtime representation back to protobuf DataBuffer
///
/// This is used for:
/// - Returning results to clients
/// - Passing data between nodes
/// - Serializing for network transport
pub fn convert_runtime_to_proto_data(runtime: RuntimeData) -> DataBuffer {
    let start = Instant::now();

    tracing::info!("convert_runtime_to_proto_data: Converting RuntimeData type={}",
        match &runtime {
            RuntimeData::Audio(b) => format!("Audio({} samples)", b.num_samples),
            RuntimeData::Video(_) => "Video".to_string(),
            RuntimeData::Tensor(_) => "Tensor".to_string(),
            RuntimeData::Json(_) => "Json".to_string(),
            RuntimeData::Text(s) => format!("Text({} chars)", s.len()),
            RuntimeData::Binary(b) => format!("Binary({} bytes)", b.len()),
        }
    );

    let data_type = Some(match runtime {
        RuntimeData::Audio(buf) => {
            tracing::info!("convert_runtime_to_proto_data: Audio buffer: samples={} bytes, sample_rate={}, channels={}, format={}, num_samples={}",
                buf.samples.len(), buf.sample_rate, buf.channels, buf.format, buf.num_samples);
            data_buffer::DataType::Audio(buf)
        },
        RuntimeData::Video(frame) => data_buffer::DataType::Video(frame),
        RuntimeData::Tensor(tensor) => data_buffer::DataType::Tensor(tensor),
        RuntimeData::Json(value) => data_buffer::DataType::Json(JsonData {
            json_payload: serde_json::to_string(&value).unwrap_or_default(),
            schema_type: String::new(),
        }),
        RuntimeData::Text(s) => data_buffer::DataType::Text(TextBuffer {
            text_data: s.into_bytes(),
            encoding: "utf-8".into(),
            language: String::new(),
        }),
        RuntimeData::Binary(bytes) => data_buffer::DataType::Binary(BinaryBuffer {
            data: bytes.to_vec(),
            mime_type: "application/octet-stream".into(),
        }),
    });

    let elapsed = start.elapsed();
    tracing::trace!("Runtime to proto conversion took {}µs", elapsed.as_micros());

    DataBuffer {
        data_type,
        metadata: Default::default(),
    }
}

// TODO: These functions require protobuf types to implement Serialize/Deserialize
// which they don't by default. Commenting out for now as they're unused.
/*
/// Convert RuntimeData to serde_json::Value for Python node processing
///
/// This serializes the RuntimeData into a JSON format that Python nodes can consume.
pub fn convert_runtime_to_value(runtime: RuntimeData) -> Result<serde_json::Value, Error> {
    // Convert to proto first, then serialize to JSON
    let proto = convert_runtime_to_proto_data(runtime);

    // Serialize the proto message to JSON
    serde_json::to_value(&proto).map_err(|e| Error::Execution(format!(
        "Failed to serialize RuntimeData to JSON: {}",
        e
    )))
}

/// Convert serde_json::Value from Python node output back to RuntimeData
///
/// This deserializes the JSON output from Python nodes back into RuntimeData.
pub fn convert_value_to_runtime(value: serde_json::Value) -> Result<RuntimeData, Error> {
    // Deserialize JSON to DataBuffer proto
    let proto: DataBuffer = serde_json::from_value(value).map_err(|e| Error::Execution(format!(
        "Failed to deserialize JSON to DataBuffer: {}",
        e
    )))?;

    // Convert proto to RuntimeData
    convert_proto_to_runtime_data(proto)
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_round_trip() {
        let original = DataBuffer {
            data_type: Some(data_buffer::DataType::Audio(AudioBuffer {
                samples: vec![0u8; 1600],
                sample_rate: 16000,
                channels: 1,
                format: 1, // F32
                num_samples: 400,
            })),
            metadata: Default::default(),
        };

        let runtime = convert_proto_to_runtime_data(original.clone()).unwrap();
        match &runtime {
            RuntimeData::Audio(buf) => {
                assert_eq!(buf.sample_rate, 16000);
                assert_eq!(buf.num_samples, 400);
            },
            _ => panic!("Expected audio"),
        }

        let back_to_proto = convert_runtime_to_proto_data(runtime);
        match back_to_proto.data_type {
            Some(data_buffer::DataType::Audio(buf)) => {
                assert_eq!(buf.sample_rate, 16000);
            },
            _ => panic!("Expected audio"),
        }
    }

    #[test]
    fn test_json_parsing() {
        let json_buf = DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"operation": "add", "operands": [10, 20]}"#.into(),
                schema_type: "CalculatorRequest".into(),
            })),
            metadata: Default::default(),
        };

        let runtime = convert_proto_to_runtime_data(json_buf).unwrap();
        match runtime {
            RuntimeData::Json(value) => {
                assert_eq!(value["operation"], "add");
                assert_eq!(value["operands"][0], 10);
            },
            _ => panic!("Expected JSON"),
        }
    }

    #[test]
    fn test_invalid_json() {
        let bad_json = DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: "{invalid json}".into(),
                schema_type: String::new(),
            })),
            metadata: Default::default(),
        };

        let result = convert_proto_to_runtime_data(bad_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_data_buffer() {
        let empty = DataBuffer {
            data_type: None,
            metadata: Default::default(),
        };

        let result = convert_proto_to_runtime_data(empty);
        assert!(result.is_err());
    }

    #[test]
    fn test_text_utf8_validation() {
        let text_buf = DataBuffer {
            data_type: Some(data_buffer::DataType::Text(TextBuffer {
                text_data: "Hello, 世界!".as_bytes().to_vec(),
                encoding: "utf-8".into(),
                language: "en".into(),
            })),
            metadata: Default::default(),
        };

        let runtime = convert_proto_to_runtime_data(text_buf).unwrap();
        match runtime {
            RuntimeData::Text(s) => {
                assert_eq!(s, "Hello, 世界!");
            },
            _ => panic!("Expected text"),
        }
    }
}
