//! Adapters for converting between RuntimeData and Protobuf types
//!
//! This module provides conversion functions between runtime-core's RuntimeData
//! and the gRPC Protobuf DataBuffer types.

use crate::generated::data_buffer::DataType;
use crate::generated::{
    AudioBuffer, AudioFormat, BatchHint, BinaryBuffer, CancelSpeculation, ControlMessage,
    DataBuffer, DeadlineWarning, JsonData, PixelFormat, TensorBuffer, TensorDtype, TextBuffer,
    VideoFrame,
};
use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::transport::TransportData;

/// Convert runtime-core RuntimeData to Protobuf DataBuffer
pub fn runtime_data_to_data_buffer(data: &RuntimeData) -> DataBuffer {
    let data_type = match data {
        RuntimeData::Audio {
            samples,
            sample_rate,
            channels,
        } => DataType::Audio(AudioBuffer {
            samples: samples.iter().flat_map(|f| f.to_le_bytes()).collect(),
            sample_rate: *sample_rate,
            channels: *channels,
            format: AudioFormat::F32 as i32,
            num_samples: samples.len() as u64,
        }),
        RuntimeData::Video {
            pixel_data,
            width,
            height,
            format,
            frame_number,
            timestamp_us,
        } => DataType::Video(VideoFrame {
            pixel_data: pixel_data.clone(),
            width: *width,
            height: *height,
            format: *format,
            frame_number: *frame_number,
            timestamp_us: *timestamp_us,
        }),
        RuntimeData::Tensor { data, shape, dtype } => {
            DataType::Tensor(TensorBuffer {
                data: data.clone(),
                shape: shape.iter().map(|&s| s as u64).collect(),
                dtype: *dtype,
                layout: String::new(), // Empty layout - nodes document expected layout
            })
        }
        RuntimeData::Json(value) => DataType::Json(JsonData {
            json_payload: serde_json::to_string(value).unwrap_or_default(),
            schema_type: String::new(),
        }),
        RuntimeData::Text(s) => DataType::Text(TextBuffer {
            text_data: s.as_bytes().to_vec(),
            encoding: "utf-8".to_string(),
            language: String::new(),
        }),
        RuntimeData::Binary(bytes) => DataType::Binary(BinaryBuffer {
            data: bytes.clone(),
            mime_type: "application/octet-stream".to_string(),
        }),
        RuntimeData::ControlMessage {
            message_type,
            segment_id,
            timestamp_ms,
            metadata,
        } => {
            use remotemedia_runtime_core::data::ControlMessageType;
            use crate::generated::control_message::MessageType;

            // Convert ControlMessageType to protobuf oneof
            let proto_message_type = match message_type {
                ControlMessageType::CancelSpeculation {
                    from_timestamp,
                    to_timestamp,
                } => MessageType::CancelSpeculation(CancelSpeculation {
                    from_timestamp: *from_timestamp,
                    to_timestamp: *to_timestamp,
                }),
                ControlMessageType::BatchHint {
                    suggested_batch_size,
                } => MessageType::BatchHint(BatchHint {
                    suggested_batch_size: *suggested_batch_size as u32,
                }),
                ControlMessageType::DeadlineWarning { deadline_us } => {
                    MessageType::DeadlineWarning(DeadlineWarning {
                        deadline_us: *deadline_us,
                    })
                }
            };

            DataType::Control(ControlMessage {
                message_type: Some(proto_message_type),
                segment_id: segment_id.clone().unwrap_or_default(),
                timestamp_ms: *timestamp_ms,
                metadata: serde_json::to_string(metadata).unwrap_or_default(),
            })
        }
    };

    DataBuffer {
        data_type: Some(data_type),
        metadata: std::collections::HashMap::new(),
    }
}

/// Convert Protobuf DataBuffer to runtime-core RuntimeData
pub fn data_buffer_to_runtime_data(buffer: &DataBuffer) -> Option<RuntimeData> {
    match &buffer.data_type {
        Some(DataType::Audio(audio)) => {
            // Convert bytes back to f32 samples
            let samples: Vec<f32> = audio
                .samples
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();

            Some(RuntimeData::Audio {
                samples,
                sample_rate: audio.sample_rate,
                channels: audio.channels,
            })
        }
        Some(DataType::Video(video)) => Some(RuntimeData::Video {
            pixel_data: video.pixel_data.clone(),
            width: video.width,
            height: video.height,
            format: video.format,
            frame_number: video.frame_number,
            timestamp_us: video.timestamp_us,
        }),
        Some(DataType::Tensor(tensor)) => Some(RuntimeData::Tensor {
            data: tensor.data.clone(),
            shape: tensor.shape.iter().map(|&s| s as i32).collect(),
            dtype: tensor.dtype,
        }),
        Some(DataType::Json(json)) => serde_json::from_str(&json.json_payload)
            .ok()
            .map(RuntimeData::Json),
        Some(DataType::Text(text)) => String::from_utf8(text.text_data.clone())
            .ok()
            .map(RuntimeData::Text),
        Some(DataType::Binary(bin)) => Some(RuntimeData::Binary(bin.data.clone())),
        _ => None,
    }
}

/// Convert TransportData to Protobuf DataBuffer
pub fn transport_data_to_data_buffer(data: &TransportData) -> DataBuffer {
    let mut buffer = runtime_data_to_data_buffer(&data.data);

    // Add sequence number to metadata if present
    if let Some(seq) = data.sequence {
        buffer
            .metadata
            .insert("sequence".to_string(), seq.to_string());
    }

    // Add transport metadata
    for (key, value) in &data.metadata {
        buffer.metadata.insert(key.clone(), value.clone());
    }

    buffer
}

/// Convert Protobuf DataBuffer to TransportData
pub fn data_buffer_to_transport_data(buffer: &DataBuffer) -> Option<TransportData> {
    let runtime_data = data_buffer_to_runtime_data(buffer)?;
    let mut transport_data = TransportData::new(runtime_data);

    // Extract sequence number from metadata if present
    if let Some(seq_str) = buffer.metadata.get("sequence") {
        if let Ok(seq) = seq_str.parse::<u64>() {
            transport_data.sequence = Some(seq);
        }
    }

    // Copy metadata
    for (key, value) in &buffer.metadata {
        if key != "sequence" {
            // Don't duplicate sequence in metadata
            transport_data.metadata.insert(key.clone(), value.clone());
        }
    }

    Some(transport_data)
}
