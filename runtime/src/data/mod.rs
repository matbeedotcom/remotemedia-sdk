// Generic data types and conversion module
// Feature: 004-generic-streaming
//
// This module provides:
// - RuntimeData: In-memory representation of all data types
// - Conversion functions: Proto ↔ Runtime
// - Validation functions: Type checking and size validation

pub mod conversions;
pub mod runtime_data;
pub mod validation;

// Re-export main types for convenience
pub use conversions::{convert_proto_to_runtime_data, convert_runtime_to_proto_data};
pub use runtime_data::RuntimeData;
pub use validation::{validate_tensor_size, validate_text_buffer, validate_video_frame};
