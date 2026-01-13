//! Media constraint types for capability negotiation (spec 022)
//!
//! This module defines the constraint types used for declaring node capabilities.
//! Constraints support exact values, ranges, sets, and "any" (null in JSON).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Core Constraint Value Type (Phase 2: Foundational)
// =============================================================================

/// Generic constraint expression supporting exact values, ranges, sets, or "any".
///
/// This enum is used to express constraints on media properties like sample rate,
/// resolution, channel count, etc. It supports flexible matching to enable
/// capability negotiation between nodes.
///
/// # JSON Representations
///
/// - **Exact**: `48000` (single value)
/// - **Range**: `{"min": 16000, "max": 48000}` (inclusive range)
/// - **Set**: `[16000, 44100, 48000]` (discrete values)
/// - **Any**: `null` or field omitted (accepts any value)
///
/// # Example
///
/// ```rust
/// use remotemedia_runtime_core::capabilities::ConstraintValue;
///
/// // Exact value constraint
/// let exact = ConstraintValue::Exact(48000u32);
/// assert!(exact.satisfies(&48000));
/// assert!(!exact.satisfies(&16000));
///
/// // Range constraint
/// let range = ConstraintValue::Range { min: 16000, max: 48000 };
/// assert!(range.satisfies(&32000));
/// assert!(!range.satisfies(&8000));
///
/// // Set constraint
/// let set = ConstraintValue::Set(vec![16000u32, 44100, 48000]);
/// assert!(set.satisfies(&44100));
/// assert!(!set.satisfies(&22050));
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConstraintValue<T> {
    /// Single exact value required
    Exact(T),
    /// Inclusive range of acceptable values
    Range {
        /// Minimum value (inclusive)
        min: T,
        /// Maximum value (inclusive)
        max: T,
    },
    /// List of discrete acceptable values
    Set(Vec<T>),
}

impl<T: PartialOrd + PartialEq> ConstraintValue<T> {
    /// Check if a value satisfies this constraint.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to check against this constraint
    ///
    /// # Returns
    ///
    /// `true` if the value satisfies the constraint, `false` otherwise.
    pub fn satisfies(&self, value: &T) -> bool {
        match self {
            ConstraintValue::Exact(exact) => value == exact,
            ConstraintValue::Range { min, max } => value >= min && value <= max,
            ConstraintValue::Set(set) => set.iter().any(|v| v == value),
        }
    }

    /// Check if this constraint is flexible (Range or Set).
    ///
    /// Flexible constraints can adapt to match a fixed constraint from
    /// another node during negotiation.
    pub fn is_flexible(&self) -> bool {
        matches!(self, ConstraintValue::Range { .. } | ConstraintValue::Set(_))
    }

    /// Check if two constraints are compatible (for Ord types).
    ///
    /// Two constraints are compatible if there exists at least one value
    /// that satisfies both constraints.
    pub fn compatible_with(&self, other: &ConstraintValue<T>) -> bool
    where
        T: Clone + Ord,
    {
        self.compatible_with_partial(other)
    }

    /// Check if two constraints are compatible (for PartialOrd types like f32).
    ///
    /// Two constraints are compatible if there exists at least one value
    /// that satisfies both constraints. This version works with types
    /// that only implement PartialOrd (like f32).
    pub fn compatible_with_partial(&self, other: &ConstraintValue<T>) -> bool
    where
        T: Clone,
    {
        match (self, other) {
            // Exact vs Exact: must be equal
            (ConstraintValue::Exact(a), ConstraintValue::Exact(b)) => a == b,

            // Exact vs Range: exact must be in range
            (ConstraintValue::Exact(a), ConstraintValue::Range { min, max })
            | (ConstraintValue::Range { min, max }, ConstraintValue::Exact(a)) => {
                a >= min && a <= max
            }

            // Exact vs Set: exact must be in set
            (ConstraintValue::Exact(a), ConstraintValue::Set(set))
            | (ConstraintValue::Set(set), ConstraintValue::Exact(a)) => set.contains(a),

            // Range vs Range: ranges must overlap
            (
                ConstraintValue::Range {
                    min: min1,
                    max: max1,
                },
                ConstraintValue::Range {
                    min: min2,
                    max: max2,
                },
            ) => min1 <= max2 && min2 <= max1,

            // Range vs Set: at least one set element must be in range
            (ConstraintValue::Range { min, max }, ConstraintValue::Set(set))
            | (ConstraintValue::Set(set), ConstraintValue::Range { min, max }) => {
                set.iter().any(|v| v >= min && v <= max)
            }

            // Set vs Set: must have common elements
            (ConstraintValue::Set(set1), ConstraintValue::Set(set2)) => {
                set1.iter().any(|v| set2.contains(v))
            }
        }
    }
}

// =============================================================================
// Format Enums (Phase 2: Foundational)
// =============================================================================

/// Audio sample format enumeration (FR-003).
///
/// Represents the data format of individual audio samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum AudioSampleFormat {
    /// 32-bit floating point [-1.0, 1.0]
    F32,
    /// 16-bit signed integer [-32768, 32767]
    I16,
    /// 32-bit signed integer
    I32,
    /// 8-bit unsigned integer [0, 255]
    U8,
}

/// Video pixel format enumeration (FR-004).
///
/// Represents the pixel data format for video frames.
/// This extends the existing PixelFormat in data::video with additional formats
/// commonly used in capability negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "UPPERCASE")]
pub enum PixelFormat {
    /// 24-bit RGB (8 bits per channel, packed)
    RGB24,
    /// 32-bit RGBA (8 bits per channel, packed)
    RGBA,
    /// 24-bit BGR (8 bits per channel, packed)
    BGR24,
    /// 32-bit BGRA (8 bits per channel, packed)
    BGRA,
    /// YUV 4:2:0 planar
    YUV420,
    /// YUV 4:2:2 planar
    YUV422,
    /// NV12 (Y plane + interleaved UV)
    NV12,
    /// NV21 (Y plane + interleaved VU)
    NV21,
}

/// Tensor element data type enumeration (FR-008).
///
/// Represents the data type of tensor elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum TensorDataType {
    /// 32-bit floating point
    Float32,
    /// 64-bit floating point
    Float64,
    /// 32-bit signed integer
    Int32,
    /// 64-bit signed integer
    Int64,
    /// 8-bit unsigned integer
    Uint8,
    /// Boolean
    Bool,
}

// =============================================================================
// Media Type Constraints (Phase 3: US1)
// =============================================================================

/// Audio format constraints (FR-003).
///
/// Specifies constraints on audio data format including sample rate,
/// channel count, and sample format. Each field is optional; `None` means
/// "accept any value" for that property.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioConstraints {
    /// Sample rate constraint in Hz. `None` = any sample rate accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<ConstraintValue<u32>>,

    /// Channel count constraint. `None` = any channel count accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<ConstraintValue<u32>>,

    /// Sample format constraint. `None` = any format accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ConstraintValue<AudioSampleFormat>>,
}

/// Video format constraints (FR-004).
///
/// Specifies constraints on video frame format including resolution,
/// framerate, and pixel format. Each field is optional; `None` means
/// "accept any value" for that property.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VideoConstraints {
    /// Frame width constraint in pixels. `None` = any width accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<ConstraintValue<u32>>,

    /// Frame height constraint in pixels. `None` = any height accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<ConstraintValue<u32>>,

    /// Framerate constraint in frames per second. `None` = any framerate accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framerate: Option<ConstraintValue<f32>>,

    /// Pixel format constraint. `None` = any format accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_format: Option<ConstraintValue<PixelFormat>>,
}

/// Tensor/Numpy data constraints (FR-008).
///
/// Specifies constraints on tensor data including shape and element data type.
/// Shape dimensions can be `None` to indicate dynamic/variable dimensions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TensorConstraints {
    /// Shape constraint. Inner `None` values indicate dynamic dimensions.
    /// Outer `None` = any shape accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<ConstraintValue<Vec<Option<usize>>>>,

    /// Data type constraint. `None` = any dtype accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dtype: Option<ConstraintValue<TensorDataType>>,
}

/// Text data constraints (FR-006).
///
/// Specifies constraints on text data including character encoding
/// and format identifier.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TextConstraints {
    /// Character encoding constraint (e.g., "UTF-8", "ASCII").
    /// `None` = any encoding accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<ConstraintValue<String>>,

    /// Text format constraint (e.g., "plain", "markdown", "json").
    /// `None` = any format accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ConstraintValue<String>>,
}

/// File data constraints (FR-007).
///
/// Specifies constraints on file references including accepted
/// file extensions and MIME types.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FileConstraints {
    /// Accepted file extensions (without leading dot).
    /// `None` = any extension accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<ConstraintValue<Vec<String>>>,

    /// Accepted MIME types (e.g., "video/mp4", "audio/*").
    /// `None` = any MIME type accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_types: Option<ConstraintValue<Vec<String>>>,
}

/// JSON data constraints (FR-005).
///
/// Specifies constraints on JSON data including optional JSON Schema
/// for structure validation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JsonConstraints {
    /// JSON Schema for structure validation. `None` = any JSON accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

// =============================================================================
// Media Constraints Union (Phase 3: US1)
// =============================================================================

/// Union type for constraints on different media types.
///
/// Each variant contains the specific constraint type for that media format.
/// This is used in `MediaCapabilities` to specify what formats a node
/// accepts as input or produces as output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MediaConstraints {
    /// Audio data constraints
    Audio(AudioConstraints),
    /// Video data constraints
    Video(VideoConstraints),
    /// Tensor/Numpy data constraints
    Tensor(TensorConstraints),
    /// Text data constraints
    Text(TextConstraints),
    /// File reference constraints
    File(FileConstraints),
    /// JSON data constraints
    Json(JsonConstraints),
    /// Binary data (no constraints applicable)
    Binary,
}

impl MediaConstraints {
    /// Get the media type name as a string.
    pub fn media_type(&self) -> &'static str {
        match self {
            MediaConstraints::Audio(_) => "audio",
            MediaConstraints::Video(_) => "video",
            MediaConstraints::Tensor(_) => "tensor",
            MediaConstraints::Text(_) => "text",
            MediaConstraints::File(_) => "file",
            MediaConstraints::Json(_) => "json",
            MediaConstraints::Binary => "binary",
        }
    }

    /// Check if this constraint is flexible (has Range or Set constraints).
    ///
    /// Flexible constraints can adapt during negotiation.
    pub fn is_flexible(&self) -> bool {
        match self {
            MediaConstraints::Audio(c) => {
                c.sample_rate.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.channels.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.format.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
            }
            MediaConstraints::Video(c) => {
                c.width.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.height.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.framerate.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.pixel_format.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
            }
            MediaConstraints::Tensor(c) => {
                c.shape.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.dtype.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
            }
            MediaConstraints::Text(c) => {
                c.encoding.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.format.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
            }
            MediaConstraints::File(c) => {
                c.extensions.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
                    || c.mime_types.as_ref().map(|v| v.is_flexible()).unwrap_or(false)
            }
            MediaConstraints::Json(_) => false, // JSON schema is not flexible
            MediaConstraints::Binary => false,
        }
    }
}

// =============================================================================
// Node Media Capabilities (Phase 3: US1)
// =============================================================================

/// Complete capability declaration for a node (FR-001, FR-002).
///
/// Specifies what media formats a node accepts as input and produces as output.
/// Port names are used for nodes with multiple inputs or outputs; single-port
/// nodes typically use "default" as the port name.
///
/// # Example
///
/// ```rust
/// use remotemedia_runtime_core::capabilities::{
///     MediaCapabilities, MediaConstraints, AudioConstraints, ConstraintValue,
/// };
///
/// // Node that requires 16kHz mono audio and outputs text
/// let caps = MediaCapabilities {
///     inputs: [("default".to_string(), MediaConstraints::Audio(AudioConstraints {
///         sample_rate: Some(ConstraintValue::Exact(16000)),
///         channels: Some(ConstraintValue::Exact(1)),
///         format: None,
///     }))].into_iter().collect(),
///     outputs: [("default".to_string(), MediaConstraints::Text(Default::default()))]
///         .into_iter().collect(),
/// };
///
/// assert!(!caps.accepts_any());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MediaCapabilities {
    /// Input port requirements. Key = port name, Value = constraints.
    /// Empty map means "accept any input" (FR-008a).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub inputs: HashMap<String, MediaConstraints>,

    /// Output port capabilities. Key = port name, Value = constraints.
    /// Empty map means "output format is unspecified/passthrough" (FR-008a).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub outputs: HashMap<String, MediaConstraints>,
}

impl MediaCapabilities {
    /// Create capabilities with a single default input.
    pub fn with_input(constraints: MediaConstraints) -> Self {
        let mut inputs = HashMap::new();
        inputs.insert("default".to_string(), constraints);
        Self {
            inputs,
            outputs: HashMap::new(),
        }
    }

    /// Create capabilities with a single default output.
    pub fn with_output(constraints: MediaConstraints) -> Self {
        let mut outputs = HashMap::new();
        outputs.insert("default".to_string(), constraints);
        Self {
            inputs: HashMap::new(),
            outputs,
        }
    }

    /// Create capabilities with both default input and output.
    pub fn with_input_output(input: MediaConstraints, output: MediaConstraints) -> Self {
        let mut inputs = HashMap::new();
        let mut outputs = HashMap::new();
        inputs.insert("default".to_string(), input);
        outputs.insert("default".to_string(), output);
        Self { inputs, outputs }
    }

    /// Check if this node accepts any input (FR-008a).
    ///
    /// Returns `true` if no input constraints are specified.
    pub fn accepts_any(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Check if this node's output is unspecified (FR-008a).
    ///
    /// Returns `true` if no output constraints are specified.
    pub fn output_unspecified(&self) -> bool {
        self.outputs.is_empty()
    }

    /// Get the default input constraints, if any.
    pub fn default_input(&self) -> Option<&MediaConstraints> {
        self.inputs.get("default")
    }

    /// Get the default output constraints, if any.
    pub fn default_output(&self) -> Option<&MediaConstraints> {
        self.outputs.get("default")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ConstraintValue tests
    // =========================================================================

    #[test]
    fn test_constraint_value_satisfies_exact() {
        let constraint = ConstraintValue::Exact(48000u32);
        assert!(constraint.satisfies(&48000));
        assert!(!constraint.satisfies(&16000));
    }

    #[test]
    fn test_constraint_value_satisfies_range() {
        let constraint = ConstraintValue::Range {
            min: 16000u32,
            max: 48000,
        };
        assert!(constraint.satisfies(&16000));
        assert!(constraint.satisfies(&32000));
        assert!(constraint.satisfies(&48000));
        assert!(!constraint.satisfies(&8000));
        assert!(!constraint.satisfies(&96000));
    }

    #[test]
    fn test_constraint_value_satisfies_set() {
        let constraint = ConstraintValue::Set(vec![16000u32, 44100, 48000]);
        assert!(constraint.satisfies(&16000));
        assert!(constraint.satisfies(&44100));
        assert!(constraint.satisfies(&48000));
        assert!(!constraint.satisfies(&22050));
    }

    #[test]
    fn test_constraint_value_is_flexible() {
        assert!(!ConstraintValue::Exact(48000u32).is_flexible());
        assert!(ConstraintValue::Range { min: 16000u32, max: 48000 }.is_flexible());
        assert!(ConstraintValue::Set(vec![16000u32, 48000]).is_flexible());
    }

    #[test]
    fn test_constraint_value_compatible_exact_exact() {
        let a = ConstraintValue::Exact(48000u32);
        let b = ConstraintValue::Exact(48000u32);
        let c = ConstraintValue::Exact(16000u32);

        assert!(a.compatible_with(&b));
        assert!(!a.compatible_with(&c));
    }

    #[test]
    fn test_constraint_value_compatible_exact_range() {
        let exact = ConstraintValue::Exact(32000u32);
        let range = ConstraintValue::Range { min: 16000, max: 48000 };
        let out_of_range = ConstraintValue::Exact(8000u32);

        assert!(exact.compatible_with(&range));
        assert!(range.compatible_with(&exact));
        assert!(!out_of_range.compatible_with(&range));
    }

    #[test]
    fn test_constraint_value_compatible_range_range() {
        let r1 = ConstraintValue::Range { min: 16000u32, max: 48000 };
        let r2 = ConstraintValue::Range { min: 32000, max: 96000 };
        let r3 = ConstraintValue::Range { min: 64000, max: 96000 };

        assert!(r1.compatible_with(&r2)); // Overlap at 32000-48000
        assert!(!r1.compatible_with(&r3)); // No overlap
    }

    // =========================================================================
    // JSON serialization tests
    // =========================================================================

    #[test]
    fn test_constraint_value_json_exact() {
        let constraint = ConstraintValue::Exact(48000u32);
        let json = serde_json::to_string(&constraint).unwrap();
        assert_eq!(json, "48000");

        let parsed: ConstraintValue<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, constraint);
    }

    #[test]
    fn test_constraint_value_json_range() {
        let constraint = ConstraintValue::Range {
            min: 16000u32,
            max: 48000,
        };
        let json = serde_json::to_string(&constraint).unwrap();
        assert!(json.contains("\"min\""));
        assert!(json.contains("\"max\""));

        let parsed: ConstraintValue<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, constraint);
    }

    #[test]
    fn test_constraint_value_json_set() {
        let constraint = ConstraintValue::Set(vec![16000u32, 44100, 48000]);
        let json = serde_json::to_string(&constraint).unwrap();
        assert_eq!(json, "[16000,44100,48000]");

        let parsed: ConstraintValue<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, constraint);
    }

    #[test]
    fn test_audio_constraints_json() {
        let constraints = AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(48000)),
            channels: Some(ConstraintValue::Range { min: 1, max: 2 }),
            format: Some(ConstraintValue::Set(vec![
                AudioSampleFormat::F32,
                AudioSampleFormat::I16,
            ])),
        };

        let json = serde_json::to_string(&constraints).unwrap();
        let parsed: AudioConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, constraints);
    }

    #[test]
    fn test_media_constraints_json_tagged() {
        let constraints = MediaConstraints::Audio(AudioConstraints {
            sample_rate: Some(ConstraintValue::Exact(16000)),
            channels: Some(ConstraintValue::Exact(1)),
            format: None,
        });

        let json = serde_json::to_string(&constraints).unwrap();
        assert!(json.contains("\"type\":\"audio\""));

        let parsed: MediaConstraints = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, constraints);
    }

    // =========================================================================
    // MediaCapabilities tests
    // =========================================================================

    #[test]
    fn test_media_capabilities_accepts_any() {
        let empty = MediaCapabilities::default();
        assert!(empty.accepts_any());
        assert!(empty.output_unspecified());

        let with_input =
            MediaCapabilities::with_input(MediaConstraints::Audio(AudioConstraints::default()));
        assert!(!with_input.accepts_any());
        assert!(with_input.output_unspecified());
    }

    #[test]
    fn test_media_capabilities_with_input_output() {
        let caps = MediaCapabilities::with_input_output(
            MediaConstraints::Audio(AudioConstraints::default()),
            MediaConstraints::Text(TextConstraints::default()),
        );

        assert!(!caps.accepts_any());
        assert!(!caps.output_unspecified());
        assert!(caps.default_input().is_some());
        assert!(caps.default_output().is_some());
    }

    #[test]
    fn test_media_constraints_media_type() {
        assert_eq!(
            MediaConstraints::Audio(AudioConstraints::default()).media_type(),
            "audio"
        );
        assert_eq!(
            MediaConstraints::Video(VideoConstraints::default()).media_type(),
            "video"
        );
        assert_eq!(MediaConstraints::Binary.media_type(), "binary");
    }
}
