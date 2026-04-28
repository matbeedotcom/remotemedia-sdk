//! Shared test helpers for avatar-pipeline integration tests
//! (avatar plan M2/M3/M4).
//!
//! Owns the env-var contract that tier-2 (real-model) tests bail out on
//! cleanly when env vars are unset, and the synthetic audio fixture
//! generator that tier-1 tests use without external tools.
//!
//! Compiled into integration tests only via `#[path]` includes — no
//! need to ship as a crate. See e.g. `lipsync_envelope_test.rs`.

#![allow(dead_code)] // not every test uses every helper

/// Skip the test cleanly with a friendly message if any env var listed
/// is unset or empty. Use this at the top of any test that needs real
/// Audio2Face / RVC / Live2D / Cubism artifacts on disk.
///
/// Pattern matches the liquid-audio plan's `skip_if_no_real_gguf!`.
#[macro_export]
macro_rules! skip_if_no_real_avatar_models {
    ($($var:literal),+ $(,)?) => {
        $(
            if std::env::var($var).ok().filter(|v| !v.is_empty()).is_none() {
                eprintln!(
                    "[skip] {} not set; skipping real-model assertion (set the env var to enable)",
                    $var
                );
                return;
            }
        )+
    };
}

/// Build an in-memory mono 16 kHz f32 sine sweep audio buffer of
/// `seconds` length, sweeping linearly from `f0` Hz to `f1` Hz.
///
/// We generate this in code rather than committing a binary fixture
/// because (a) Rust + a few floating-point lines is cheaper than a
/// `sox` build dependency, and (b) the values are reproducible across
/// hosts so test assertions are deterministic.
pub fn sine_sweep_16k_mono(seconds: f32, f0: f32, f1: f32) -> Vec<f32> {
    const SR: f32 = 16_000.0;
    let n = (SR * seconds) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase: f32 = 0.0;
    for i in 0..n {
        let t = i as f32 / SR;
        // Linear frequency sweep: instantaneous freq f(t) = f0 + (f1 - f0) * (t / T)
        let freq = f0 + (f1 - f0) * (t / seconds);
        // Increment phase by 2*pi*freq/SR each sample (numerically stable
        // even for long sweeps — no accumulating cos(2πft) drift).
        phase += std::f32::consts::TAU * freq / SR;
        out.push(phase.sin() * 0.5);
    }
    out
}

/// Compute RMS energy of a mono audio buffer. Sanity-check helper.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}
