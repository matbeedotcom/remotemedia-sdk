//! Audio2Face math kernel — Rust port of the persona-engine's
//! blendshape solver chain.
//!
//! This module ports the *math* portion of
//! [`external/handcrafted-persona-engine/.../LipSync/Audio2Face/`]
//! — SolverMath utilities, the `BlendshapeSolver` trait, BVLS and PGD
//! solvers, and response curves. It does NOT yet ship the ONNX
//! inference wrapper or the `Audio2FaceLipSyncNode` coordinator that
//! plumbs them together into a streaming node.
//!
//! ## Why split it
//!
//! Two reasons ONNX inference + node wiring is deferred:
//!
//! 1. **Model artifacts.** The persona-engine bootstrapper downloads
//!    `audio2face.onnx` (hundreds of MB) plus two NPZ archives
//!    (`bs_skin.npz`, `model_data.npz`) containing 24002-vertex neutral
//!    pose, per-blendshape deformation deltas, and frontal mask
//!    indices. Without those on disk we can't end-to-end-test the
//!    inference pipeline. The solvers, by contrast, are pure linear
//!    algebra — testable with hand-crafted small matrices.
//!
//! 2. **Scope.** ONNX inference + GRU state + Box-Muller noise gen +
//!    custom NPZ reader is another ~600 lines of port. Splitting at
//!    the math/inference seam lets each pass be reviewable and
//!    correctness-verifiable in isolation.
//!
//! ## What's here vs there
//!
//! | C# source | Rust port | Status |
//! |---|---|---|
//! | `SolverMath.cs` | [`solver_math`] | ✅ |
//! | `IBlendshapeSolver.cs` | [`BlendshapeSolver`] trait | ✅ |
//! | `PgdBlendshapeSolver.cs` | [`pgd_solver::PgdBlendshapeSolver`] | ✅ |
//! | `BvlsBlendshapeSolver.cs` | [`bvls_solver::BvlsBlendshapeSolver`] | ✅ |
//! | `ResponseCurves.cs` | [`response_curves`] | ✅ |
//! | `Audio2FaceInference.cs` | (deferred — needs `ort` + audio2face.onnx) | ⏳ |
//! | `BlendshapeData.cs` | (deferred — needs NPZ/NPY reader + .npz files) | ⏳ |
//! | `Audio2FaceLipSyncProcessor.cs` | (deferred — coordinator above all of the above) | ⏳ |
//! | `ParamSmoother.cs` | per spec §3.4 belongs in renderer; M4 will port | ⏳ |
//!
//! In the meantime, [`super::SyntheticLipSyncNode`] satisfies the
//! `LipSyncNode` contract for tests and manifest fallback.

#[cfg(feature = "avatar-audio2face")]
pub mod animator_skin_config;
#[cfg(feature = "avatar-audio2face")]
pub mod blendshape_data;
pub mod bvls_solver;
#[cfg(feature = "avatar-audio2face")]
pub mod identity;
#[cfg(feature = "avatar-audio2face")]
pub mod inference;
#[cfg(feature = "avatar-audio2face")]
pub mod npy;
#[cfg(feature = "avatar-audio2face")]
pub mod npz;
pub mod pgd_solver;
pub mod response_curves;
pub mod solver_math;
pub mod solver_trait;

#[cfg(feature = "avatar-audio2face")]
pub use animator_skin_config::{AnimatorSkinConfig, AnimatorSkinConfigError};
#[cfg(feature = "avatar-audio2face")]
pub use blendshape_data::{BlendshapeConfig, BlendshapeData, BlendshapeDataError};
#[cfg(feature = "avatar-audio2face")]
pub use identity::{Audio2FaceIdentity, BundlePaths};
#[cfg(feature = "avatar-audio2face")]
pub use inference::{Audio2FaceInference, Audio2FaceOutput};

pub use bvls_solver::BvlsBlendshapeSolver;
pub use pgd_solver::PgdBlendshapeSolver;
pub use solver_math::{
    apply_regularization, compute_bounding_box_diagonal, compute_dt_d, compute_transpose,
    L1_MULTIPLIER, L2_MULTIPLIER, TEMPORAL_MULTIPLIER,
};
pub use solver_trait::BlendshapeSolver;
