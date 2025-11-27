//! Adapters for converting between RuntimeData and Protobuf types
//!
//! This module provides conversion functions between runtime-core's RuntimeData
//! and the gRPC Protobuf DataBuffer types.

// Phase 4/5 adapter infrastructure - some functions for future integration
#![allow(dead_code)]

use crate::generated::data_buffer::DataType;
use crate::generated::{
    AudioBuffer, AudioFormat, BatchHint, BinaryBuffer, CancelSpeculation, ControlMessage,
    DataBuffer, DeadlineWarning, JsonData, NumpyBuffer, PixelFormat as ProtoPixelFormat,
    TensorBuffer, TextBuffer, VideoCodec as ProtoVideoCodec, VideoFrame,
};
use remotemedia_runtime_core::data::{PixelFormat, RuntimeData, VideoCodec};
use remotemedia_runtime_core::transport::TransportData;

/// Convert runtime-core PixelFormat to protobuf PixelFormat
fn pixel_format_to_proto(format: PixelFormat) -> i32 {
    match format {
        PixelFormat::Unspecified => ProtoPixelFormat::Unspecified as i32,
        PixelFormat::Yuv420p => ProtoPixelFormat::Yuv420p as i32,
        PixelFormat::I420 => ProtoPixelFormat::I420 as i32,
        PixelFormat::NV12 => ProtoPixelFormat::Nv12 as i32,
        PixelFormat::Rgb24 => ProtoPixelFormat::Rgb24 as i32,
        PixelFormat::Rgba32 => ProtoPixelFormat::Rgba32 as i32,
        PixelFormat::Encoded => ProtoPixelFormat::Encoded as i32,
    }
}

/// Convert protobuf PixelFormat (i32) to runtime-core PixelFormat
fn proto_to_pixel_format(format: i32) -> PixelFormat {
    match ProtoPixelFormat::try_from(format) {
        Ok(ProtoPixelFormat::Unspecified) => PixelFormat::Unspecified,
        Ok(ProtoPixelFormat::Rgb24) => PixelFormat::Rgb24,
        Ok(ProtoPixelFormat::Rgba32) => PixelFormat::Rgba32,
        Ok(ProtoPixelFormat::Yuv420p) => PixelFormat::Yuv420p,
        Ok(ProtoPixelFormat::Gray8) => PixelFormat::Unspecified, // Map to Unspecified (not in runtime-core)
        Ok(ProtoPixelFormat::I420) => PixelFormat::I420,
        Ok(ProtoPixelFormat::Nv12) => PixelFormat::NV12,
        Ok(ProtoPixelFormat::Encoded) => PixelFormat::Encoded,
        _ => PixelFormat::Unspecified,
    }
}

/// Convert runtime-core VideoCodec to protobuf VideoCodec
fn video_codec_to_proto(codec: Option<VideoCodec>) -> i32 {
    match codec {
        None => ProtoVideoCodec::Unspecified as i32,
        Some(VideoCodec::Vp8) => ProtoVideoCodec::Vp8 as i32,
        Some(VideoCodec::H264) => ProtoVideoCodec::H264 as i32,
        Some(VideoCodec::Av1) => ProtoVideoCodec::Av1 as i32,
    }
}

/// Convert protobuf VideoCodec (i32) to runtime-core VideoCodec
fn proto_to_video_codec(codec: i32) -> Option<VideoCodec> {
    match ProtoVideoCodec::try_from(codec) {
        Ok(ProtoVideoCodec::Unspecified) => None,
        Ok(ProtoVideoCodec::Vp8) => Some(VideoCodec::Vp8),
        Ok(ProtoVideoCodec::H264) => Some(VideoCodec::H264),
        Ok(ProtoVideoCodec::Av1) => Some(VideoCodec::Av1),
        _ => None,
    }
}

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
            codec,
            is_keyframe,
        } => DataType::Video(VideoFrame {
            pixel_data: pixel_data.clone(),
            width: *width,
            height: *height,
            format: pixel_format_to_proto(*format),
            frame_number: *frame_number,
            timestamp_us: *timestamp_us,
            codec: video_codec_to_proto(*codec),
            is_keyframe: *is_keyframe,
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
        RuntimeData::Numpy {
            data,
            shape,
            dtype,
            strides,
            c_contiguous,
            f_contiguous,
        } => DataType::Numpy(NumpyBuffer {
            data: data.clone(),
            shape: shape.iter().map(|&s| s as u64).collect(),
            dtype: dtype.clone(),
            strides: strides.iter().map(|&s| s as i64).collect(),
            c_contiguous: *c_contiguous,
            f_contiguous: *f_contiguous,
        }),
        RuntimeData::ControlMessage {
            message_type,
            segment_id,
            timestamp_ms,
            metadata,
        } => {
            use crate::generated::control_message::MessageType;
            use remotemedia_runtime_core::data::ControlMessageType;

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
            format: proto_to_pixel_format(video.format),
            frame_number: video.frame_number,
            timestamp_us: video.timestamp_us,
            codec: proto_to_video_codec(video.codec),
            is_keyframe: video.is_keyframe,
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
        Some(DataType::Numpy(numpy)) => Some(RuntimeData::Numpy {
            data: numpy.data.clone(),
            shape: numpy.shape.iter().map(|&s| s as usize).collect(),
            dtype: numpy.dtype.clone(),
            strides: numpy.strides.iter().map(|&s| s as isize).collect(),
            c_contiguous: numpy.c_contiguous,
            f_contiguous: numpy.f_contiguous,
        }),
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
