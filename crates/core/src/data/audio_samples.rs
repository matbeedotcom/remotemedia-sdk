//! Storage enum for audio samples with zero-copy + RT-bridging variants.
//!
//! `AudioSamples` is the backing store for `RuntimeData::Audio.samples`.
//! It replaces the older `Vec<f32>` field so real-time producers (CoreAudio
//! HAL callback, AU plugin, JACK client, pre-warmed buffer pool, etc.) can
//! hand audio into the pipeline without a forced heap allocation.
//!
//! # Variants
//!
//! - `Vec(Vec<f32>)` — historical form. `Clone` copies.
//! - `Arc(Arc<[f32]>)` — zero-copy shared; `Clone` is just a ref-count bump.
//!   The fast path for async fan-out.
//! - `Pooled(PooledAudioBuf)` — backing storage comes from an
//!   [`AudioBufferPool`](super::audio_buffer_pool::AudioBufferPool) and returns
//!   to the pool on drop. The fast path for an RT thread that wants zero
//!   allocations steady-state.
//!
//! # Consumers
//!
//! All three variants `Deref` to `&[f32]`, so read-only consumers
//! (`.len()`, `.iter()`, indexing, slicing, `.to_vec()`) work unchanged
//! regardless of storage. Only code that previously consumed a
//! `Vec<f32>` by value needs to call [`AudioSamples::into_vec`] explicitly.
//!
//! # Clone cost
//!
//! | variant | clone |
//! |---------|-------|
//! | `Vec`    | full `Vec<f32>` copy (same as before) |
//! | `Arc`    | ref-count bump (zero copy)            |
//! | `Pooled` | copy into a new `Vec` variant (documented) |
//!
//! Cloning a `Pooled` variant is not free because the pool buffer must
//! stay attached to its pool for RAII return-on-drop semantics. If you
//! need cheap sharing, promote to `Arc` once at the boundary.

use super::audio_buffer_pool::PooledAudioBuf;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::Arc;

/// Backing storage for `RuntimeData::Audio` samples.
///
/// See module docs for variant semantics.
pub enum AudioSamples {
    /// Owned heap vector. Historical form; `Clone` copies.
    Vec(Vec<f32>),
    /// Shared immutable slice. `Clone` bumps ref count — zero copy.
    Arc(Arc<[f32]>),
    /// Pool-backed buffer. Returns to the pool on drop.
    Pooled(PooledAudioBuf),
}

impl AudioSamples {
    /// Borrow the samples as a slice. Zero-copy for every variant.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        self
    }

    /// Number of samples.
    #[inline]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Whether the sample buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Consume and return a `Vec<f32>`.
    ///
    /// * `Vec`    — O(1), returns the existing vector.
    /// * `Arc`    — O(n) copy.
    /// * `Pooled` — O(1), detaches the pool buffer. The pool will **not**
    ///   receive this buffer back on drop; it is no longer recyclable.
    pub fn into_vec(self) -> Vec<f32> {
        match self {
            AudioSamples::Vec(v) => v,
            AudioSamples::Arc(a) => a.to_vec(),
            AudioSamples::Pooled(p) => p.into_inner(),
        }
    }

    /// Consume and return an `Arc<[f32]>`.
    ///
    /// * `Arc`    — O(1), returns the existing `Arc`.
    /// * `Vec`    — O(1) on modern stdlib (boxes the vec then wraps).
    /// * `Pooled` — O(1), detaches and wraps.
    pub fn into_arc(self) -> Arc<[f32]> {
        match self {
            AudioSamples::Vec(v) => Arc::from(v.into_boxed_slice()),
            AudioSamples::Arc(a) => a,
            AudioSamples::Pooled(p) => Arc::from(p.into_inner().into_boxed_slice()),
        }
    }

    /// Which variant am I? For diagnostics / tests.
    pub fn variant_name(&self) -> &'static str {
        match self {
            AudioSamples::Vec(_) => "Vec",
            AudioSamples::Arc(_) => "Arc",
            AudioSamples::Pooled(_) => "Pooled",
        }
    }
}

impl Deref for AudioSamples {
    type Target = [f32];

    #[inline]
    fn deref(&self) -> &[f32] {
        match self {
            AudioSamples::Vec(v) => v.as_slice(),
            AudioSamples::Arc(a) => &a[..],
            AudioSamples::Pooled(p) => p.as_slice(),
        }
    }
}

impl AsRef<[f32]> for AudioSamples {
    #[inline]
    fn as_ref(&self) -> &[f32] {
        self
    }
}

impl std::fmt::Debug for AudioSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioSamples")
            .field("variant", &self.variant_name())
            .field("len", &self.len())
            .finish()
    }
}

impl Clone for AudioSamples {
    /// Clone policy:
    ///
    /// - `Vec` → full copy (same cost as today)
    /// - `Arc` → ref-count bump (zero copy)
    /// - `Pooled` → copy into a `Vec` variant. The original pool buffer
    ///   stays attached to its pool; the clone is a freshly-allocated
    ///   `Vec<f32>`. To share without copying, call
    ///   [`AudioSamples::into_arc`] first.
    fn clone(&self) -> Self {
        match self {
            AudioSamples::Vec(v) => AudioSamples::Vec(v.clone()),
            AudioSamples::Arc(a) => AudioSamples::Arc(Arc::clone(a)),
            AudioSamples::Pooled(p) => AudioSamples::Vec(p.to_vec()),
        }
    }
}

impl PartialEq for AudioSamples {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Default for AudioSamples {
    fn default() -> Self {
        AudioSamples::Vec(Vec::new())
    }
}

impl From<Vec<f32>> for AudioSamples {
    #[inline]
    fn from(v: Vec<f32>) -> Self {
        AudioSamples::Vec(v)
    }
}

impl From<Arc<[f32]>> for AudioSamples {
    #[inline]
    fn from(a: Arc<[f32]>) -> Self {
        AudioSamples::Arc(a)
    }
}

impl From<Box<[f32]>> for AudioSamples {
    #[inline]
    fn from(b: Box<[f32]>) -> Self {
        AudioSamples::Arc(Arc::from(b))
    }
}

impl From<PooledAudioBuf> for AudioSamples {
    #[inline]
    fn from(p: PooledAudioBuf) -> Self {
        AudioSamples::Pooled(p)
    }
}

impl From<&[f32]> for AudioSamples {
    #[inline]
    fn from(s: &[f32]) -> Self {
        AudioSamples::Vec(s.to_vec())
    }
}

// Serialize as a flat `[f32]` (bincode/JSON-compatible with the old
// `Vec<f32>` representation). Deserialize always lands in the `Vec`
// variant — there is no wire format for `Arc` or `Pooled`.
impl Serialize for AudioSamples {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.as_slice().serialize(s)
    }
}

impl<'de> Deserialize<'de> for AudioSamples {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<f32>::deserialize(d).map(AudioSamples::Vec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::audio_buffer_pool::AudioBufferPool;

    #[test]
    fn vec_variant_round_trip() {
        let s: AudioSamples = vec![1.0, 2.0, 3.0].into();
        assert_eq!(s.len(), 3);
        assert_eq!(s.as_slice(), &[1.0, 2.0, 3.0]);
        assert_eq!(s.into_vec(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn arc_variant_zero_copy_clone() {
        let arc: Arc<[f32]> = Arc::from(vec![4.0, 5.0, 6.0].into_boxed_slice());
        let s: AudioSamples = arc.clone().into();
        let s2 = s.clone();
        // Both variants share the same heap buffer.
        if let (AudioSamples::Arc(a1), AudioSamples::Arc(a2)) = (&s, &s2) {
            assert!(Arc::ptr_eq(a1, a2));
        } else {
            panic!("expected Arc variant");
        }
        assert_eq!(s.as_slice(), s2.as_slice());
    }

    #[test]
    fn pooled_variant_returns_to_pool_on_drop() {
        let pool = Arc::new(AudioBufferPool::new(4, 256));
        assert!(pool.is_empty());
        {
            let mut buf = pool.acquire();
            buf.extend_from_slice(&[0.5; 128]);
            let samples: AudioSamples = buf.into();
            assert_eq!(samples.len(), 128);
            assert_eq!(samples.variant_name(), "Pooled");
        }
        // Dropping the AudioSamples(Pooled(...)) returned the buffer.
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn pooled_clone_becomes_vec_and_pool_retains_original() {
        let pool = Arc::new(AudioBufferPool::new(4, 256));
        let mut buf = pool.acquire();
        buf.extend_from_slice(&[0.25; 64]);
        let a: AudioSamples = buf.into();
        let b = a.clone();
        assert_eq!(b.variant_name(), "Vec");
        assert_eq!(a.as_slice(), b.as_slice());
        drop(a);
        // Original pool buffer returned; clone was an independent Vec.
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn deref_exposes_slice_methods() {
        let s: AudioSamples = vec![1.0, 2.0, 3.0, 4.0].into();
        // Slice methods go through Deref.
        assert_eq!(s.iter().sum::<f32>(), 10.0);
        assert_eq!(&s[1..3], &[2.0, 3.0]);
        assert_eq!(s.to_vec(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn equality_across_variants() {
        let pool = Arc::new(AudioBufferPool::new(4, 16));
        let mut pooled = pool.acquire();
        pooled.extend_from_slice(&[1.0, 2.0, 3.0]);

        let v = AudioSamples::from(vec![1.0, 2.0, 3.0]);
        let a = AudioSamples::from(Arc::from(vec![1.0, 2.0, 3.0].into_boxed_slice()));
        let p = AudioSamples::from(pooled);

        assert_eq!(v, a);
        assert_eq!(a, p);
        assert_eq!(p, v);
    }

    #[test]
    fn into_arc_from_vec_zero_copy_semantics() {
        let v: AudioSamples = vec![7.0, 8.0, 9.0].into();
        let arc = v.into_arc();
        assert_eq!(arc.as_ref(), &[7.0, 8.0, 9.0]);
    }

    #[test]
    fn into_vec_from_pooled_detaches_from_pool() {
        let pool = Arc::new(AudioBufferPool::new(4, 64));
        let mut buf = pool.acquire();
        buf.extend_from_slice(&[0.1; 32]);
        let s: AudioSamples = buf.into();
        let _detached = s.into_vec();
        // Pool did not get the buffer back.
        assert!(pool.is_empty());
    }

    #[test]
    fn serde_round_trip_as_flat_vec() {
        let s: AudioSamples = vec![1.5, 2.5, 3.5].into();
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, "[1.5,2.5,3.5]");
        let back: AudioSamples = serde_json::from_str(&j).unwrap();
        assert_eq!(back.variant_name(), "Vec");
        assert_eq!(back.as_slice(), &[1.5, 2.5, 3.5]);
    }
}
