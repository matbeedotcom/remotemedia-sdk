//! Adapters for converting between RuntimeData and Protobuf types
//!
//! This module provides conversion functions between runtime-core's RuntimeData
//! and the gRPC Protobuf DataBuffer types.

use remotemedia_runtime_core::data::RuntimeData;
use remotemedia_runtime_core::transport::TransportData;
use crate::generated::{DataBuffer, AudioBuffer, TextBuffer, BinaryBuffer, AudioFormat};
use crate::generated::data_buffer::DataType;

/// Convert runtime-core RuntimeData to Protobuf DataBuffer
pub fn runtime_data_to_data_buffer(data: &RuntimeData) -> DataBuffer {
    let data_type = match data {
        RuntimeData::Audio { samples, sample_rate, channels } => {
            DataType::Audio(AudioBuffer {
                samples: samples.iter().flat_map(|f| f.to_le_bytes()).collect(),
                sample_rate: *sample_rate,
                channels: *channels,
                format: AudioFormat::F32le as i32,
                num_samples: samples.len() as u64,
            })
        }
        RuntimeData::Text(s) => {
            DataType::Text(TextBuffer {
                content: s.clone(),
                encoding: "utf-8".to_string(),
            })
        }
        RuntimeData::Binary(bytes) => {
            DataType::Binary(BinaryBuffer {
                data: bytes.clone(),
                mime_type: "application/octet-stream".to_string(),
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
        Some(DataType::Text(text)) => {
            Some(RuntimeData::Text(text.content.clone()))
        }
        Some(DataType::Binary(bin)) => {
            Some(RuntimeData::Binary(bin.data.clone()))
        }
        _ => None,
    }
}

/// Convert TransportData to Protobuf DataBuffer
pub fn transport_data_to_data_buffer(data: &TransportData) -> DataBuffer {
    let mut buffer = runtime_data_to_data_buffer(&data.data);

    // Add sequence number to metadata if present
    if let Some(seq) = data.sequence {
        buffer.metadata.insert("sequence".to_string(), seq.to_string());
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
        if key != "sequence" {  // Don't duplicate sequence in metadata
            transport_data.metadata.insert(key.clone(), value.clone());
        }
    }

    Some(transport_data)
}
