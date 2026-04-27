//! Emotion vector extraction and steering nodes
//!
//! Implements the two-phase pipeline from [Anthropic's 2026 emotion-vector
//! research](https://transformer-circuits.pub/2026/emotions/index.html):
//!
//! **Phase 1 — Extraction** (offline, one-time):
//!   Given labelled prompts grouped by emotion, capture residual-stream
//!   activations at a target layer, compute mean-subtraction vectors,
//!   and emit them as `RuntimeData::Tensor`.
//!
//! **Phase 2 — Steering** (runtime inference):
//!   During generation, add `coef × layer_norm × vector` to the
//!   residual stream at the target layer. This shifts the model's
//!   output toward (or away from) the target emotion.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │  EmotionExtractorNode  (offline)                             │
//! │                                                              │
//! │  Input:  RuntimeData::Json  (labelled prompts)               │
//! │  Output: RuntimeData::Tensor  (shape [hidden_size])          │
//! │          RuntimeData::Json    (metadata: layer, emotion…)    │
//! └──────────────────────────────────────────────────────────────┘
//!
//! ┌──────────────────────────────────────────────────────────────┐
//! │  EmotionSteeringNode  (runtime)                              │
//! │                                                              │
//! │  Input:  RuntimeData::Text       (user prompt)               │
//! │          RuntimeData::Tensor     (emotion vector, side-input) │
//! │  Output: RuntimeData::Text       (steered generation)        │
//! │          RuntimeData::Json       (coefficients applied)      │
//! │                                                              │
//! │  Aux ports:                                                   │
//! │    steering.in.coefficients  → { "happy": 0.5, "sad": 0.0 }  │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Current limitations
//!
//! The Candle framework does not yet expose a forward-hook API
//! equivalent to PyTorch's `register_forward_hook()`. Until Candle
//! gains this (or we build a layer-iteration wrapper), the steering
//! node operates in **metadata-only** mode: it validates the vector
//! shape, computes the delta, and emits the coefficients as JSON
//! metadata. The actual residual-stream injection requires a
//! Candle-side change (tracked as `TODO: candle-forward-hooks`).
//!
//! The extraction node works fully today because it uses Candle's
//! public tensor operations (`.mean()`, subtraction, `.normalize()`)
//! on pre-captured activation data passed as `RuntimeData::Tensor`
//! input. For live model inference, pair it with a Python node that
//! uses PyTorch hooks (see `llm_feeling_weather` reference repo).

mod config;
mod extract;
mod steer;
mod vector_io;

pub use config::{
    EmotionExtractConfig, EmotionSteerConfig, EmotionVectorMetadata, PoolingMode, SteeringVectorConfig,
};
pub use extract::{EmotionExtractorNode, EmotionExtractorNodeFactory};
pub use steer::{EmotionSteeringNode, EmotionSteeringNodeFactory};
pub use vector_io::{load_emotion_vector, save_emotion_vector};
