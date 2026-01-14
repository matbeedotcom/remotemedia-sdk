//! Candle ML Inference Nodes for RemoteMedia Pipelines
//!
//! This crate provides native Rust-based machine learning inference nodes
//! powered by the Hugging Face Candle framework. Nodes implement the
//! `StreamingNode` trait for seamless pipeline integration.
//!
//! # Features
//!
//! - `whisper` - Speech-to-text transcription via Whisper models
//! - `yolo` - Object detection via YOLO models
//! - `llm` - Text generation via Phi and LLaMA models
//! - `cuda` - NVIDIA GPU acceleration
//! - `metal` - Apple GPU acceleration
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_candle_nodes::WhisperNode;
//!
//! let node = WhisperNode::new("whisper-1", &config)?;
//! node.initialize().await?;
//! let result = node.process_async(audio_data).await?;
//! ```

pub mod error;

#[cfg(feature = "whisper")]
pub mod whisper;

#[cfg(feature = "yolo")]
pub mod yolo;

#[cfg(feature = "llm")]
pub mod llm;

mod device;
mod cache;
mod convert;
mod registry;

pub use error::{CandleNodeError, Result};
pub use device::{InferenceDevice, DeviceSelector};
pub use cache::{ModelCache, CachedModel};
pub use convert::{RuntimeDataConverter, TensorExt};
pub use registry::{register_candle_nodes, CandleNodeFactory};

#[cfg(feature = "whisper")]
pub use whisper::{WhisperNode, WhisperConfig, WhisperNodeFactory};

#[cfg(feature = "yolo")]
pub use yolo::{YoloNode, YoloConfig, YoloNodeFactory, DetectionResult, Detection};

#[cfg(feature = "llm")]
pub use llm::{PhiNode, LlamaNode, LlmConfig, GenerationConfig};
