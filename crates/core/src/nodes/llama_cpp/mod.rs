//! llama.cpp Nodes — native GGUF inference via [llama-cpp-4](https://crates.io/crates/llama-cpp-4)
//!
//! Provides four streaming nodes that wrap the llama.cpp C library through
//! safe Rust bindings:
//!
//! | Node | Purpose |
//! |---|---|
//! | [`LlamaCppGenerationNode`] | Text generation (chat / completion) with token streaming |
//! | [`LlamaCppEmbeddingNode`] | Text → dense vector embeddings |
//! | [`LlamaCppActivationNode`] | Capture hidden-state activations at arbitrary layers |
//! | [`LlamaCppSteerNode`] | Inject activation deltas (emotion steering, DoG, etc.) |
//!
//! # Activation vector analysis
//!
//! The [`LlamaCppActivationNode`] uses llama.cpp's `TensorCapture` callback
//! to extract per-token hidden states at any layer during `llama_decode`.
//! This is the Rust equivalent of PyTorch's `register_forward_hook()` and
//! enables the same emotion-vector pipeline that lives in
//! `remotemedia-candle-nodes::emotion`:
//!
//! ```text
//!  ┌─────────────────────┐   Tensor [n_tokens × hidden]
//!  │ LlamaCppActivation  │──────────────────────────────┐
//!  │       Node          │                              │
//!  └─────────────────────┘                              ▼
//!                                                    ┌──────────────────┐
//!                                                    │ EmotionExtractor │
//!                                                    │     Node         │
//!                                                    └────────┬─────────┘
//!                                                             │
//!                                                             ▼
//!                                                    ┌──────────────────┐
//!                                                    │ EmotionSteering  │
//!                                                    │     Node         │
//!                                                    └────────┬─────────┘
//!                                                             │
//!                                                             ▼
//!  ┌─────────────────────┐   Tensor (delta vector)
//!  │ LlamaCppSteerNode   │◄──────────────────────────────────┘
//!  │       (optional)    │
//!  └─────────────────────┘
//! ```
//!
//! # GPU support
//!
//! Enable GPU acceleration via cargo features on the `llama-cpp-4` crate:
//!
//! ```toml
//! # In workspace Cargo.toml
//! [dependencies]
//! llama-cpp-4 = { version = "0.2.13", features = ["cuda"] }
//! # or features = ["metal"] for Apple Silicon
//! # or features = ["vulkan"] for cross-platform GPU
//! ```

mod config;
mod generation;
mod embedding;
mod activation;
mod steer;
mod factory;
mod inference;

pub use config::{
    LlamaCppConfig, LlamaCppGenerationConfig, LlamaCppEmbeddingConfig,
    LlamaCppActivationConfig, LlamaCppSteerConfig, LlamaCppSteerVector,
    LlamaBackendConfig, GpuOffload,
};
pub use generation::{LlamaCppGenerationNode, LlamaCppGenerationNodeFactory};
pub use embedding::{LlamaCppEmbeddingNode, LlamaCppEmbeddingNodeFactory};
pub use activation::{LlamaCppActivationNode, LlamaCppActivationNodeFactory};
pub use steer::{LlamaCppSteerNode, LlamaCppSteerNodeFactory};
pub use factory::LlamaCppNodesProvider;
