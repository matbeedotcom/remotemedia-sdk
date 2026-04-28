//! Response curves — Rust port of `ResponseCurves.cs`.
//!
//! The persona-engine uses these to reshape the linear blendshape
//! activations the solvers emit into more visually-pleasing curves
//! before the renderer's ARKit→VBridger mapper sees them. Two shapes:
//!
//! - [`ease_in`] — steep at start, flat at end. Used for `JawOpen` /
//!   `MouthOpen` so a small audio response opens the mouth quickly.
//! - [`center_weighted`] — flat at extremes, steep through center.
//!   Used for `MouthPressLipOpen` and `EyeBallY` where the
//!   informative range is the middle of the activation envelope.

/// Hermite spline `f(t) = t · (2 − t)`. Steep tangent at `t=0`, flat
/// at `t=1`. Output range `[0, 1]`. Input is clamped to `[0, 1]`.
pub fn ease_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * (2.0 - t)
}

/// Three-key center-weighted Hermite curve. Keys at `(lo, lo, m=0)`,
/// `(0, 0, m=2)`, `(hi, hi, m=0)`. Steep through zero, flat at the
/// extremes. Input clamped to `[lo, hi]`.
///
/// `lo` is expected ≤ 0 and `hi` ≥ 0; degenerate spans (`|span|<ε`)
/// pass `t` through unchanged for the matching segment.
pub fn center_weighted(t: f32, lo: f32, hi: f32) -> f32 {
    let t = t.clamp(lo, hi);
    if t <= 0.0 {
        // Lower segment: lo..0
        let span = -lo;
        if span < 1e-6 {
            return t;
        }
        let s = (t - lo) / span;
        hermite_segment(s, lo, 0.0, 0.0, 2.0 * span)
    } else {
        // Upper segment: 0..hi
        let span = hi;
        if span < 1e-6 {
            return t;
        }
        let s = t / span;
        hermite_segment(s, 0.0, hi, 2.0 * span, 0.0)
    }
}

/// Cubic Hermite interpolation between (p0, m0) and (p1, m1).
/// `s ∈ [0, 1]`. Standard basis polynomials.
fn hermite_segment(s: f32, p0: f32, p1: f32, m0: f32, m1: f32) -> f32 {
    let s2 = s * s;
    let s3 = s2 * s;
    let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    let h10 = s3 - 2.0 * s2 + s;
    let h01 = -2.0 * s3 + 3.0 * s2;
    let h11 = s3 - s2;
    h00 * p0 + h10 * m0 + h01 * p1 + h11 * m1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn ease_in_endpoints() {
        assert_eq!(ease_in(0.0), 0.0);
        assert_eq!(ease_in(1.0), 1.0);
    }

    #[test]
    fn ease_in_clamps_negative() {
        assert_eq!(ease_in(-0.5), 0.0);
    }

    #[test]
    fn ease_in_clamps_above_one() {
        assert_eq!(ease_in(1.7), 1.0);
    }

    #[test]
    fn ease_in_midpoint_steeper_than_linear() {
        // f(0.5) = 0.5 * 1.5 = 0.75, well above linear midpoint 0.5.
        assert!(approx(ease_in(0.5), 0.75, 1e-6));
    }

    #[test]
    fn ease_in_monotonic_increasing() {
        let mut prev = ease_in(0.0);
        for i in 1..=20 {
            let t = i as f32 / 20.0;
            let cur = ease_in(t);
            assert!(cur >= prev, "monotonic: {prev} -> {cur} at t={t}");
            prev = cur;
        }
    }

    #[test]
    fn center_weighted_endpoints_match_inputs() {
        // Curve passes through (lo, lo) and (hi, hi).
        assert!(approx(center_weighted(-0.5, -0.5, 0.5), -0.5, 1e-6));
        assert!(approx(center_weighted(0.5, -0.5, 0.5), 0.5, 1e-6));
    }

    #[test]
    fn center_weighted_zero_is_zero() {
        assert!(approx(center_weighted(0.0, -0.5, 0.5), 0.0, 1e-6));
    }

    #[test]
    fn center_weighted_steep_around_zero() {
        // Very small input → steep tangent → output ≈ 2 · t.
        let t = 0.01f32;
        let out = center_weighted(t, -0.5, 0.5);
        // At s = 0.02, h00=~1, h10=~0.02, h01=~0, h11=~0; with
        // p0=0, p1=hi=0.5, m0=2*hi=1, m1=0:
        // out = 0 + 0.02 * 1 + 0 + 0 ≈ 0.02 → about 2x t.
        assert!(out > t, "should be steeper than linear near zero");
    }

    #[test]
    fn center_weighted_clamps_outside_range() {
        // Outside [-1, 1] → clamped to endpoints.
        assert!(approx(center_weighted(5.0, -1.0, 1.0), 1.0, 1e-6));
        assert!(approx(center_weighted(-3.0, -1.0, 1.0), -1.0, 1e-6));
    }

    #[test]
    fn center_weighted_degenerate_lower_passes_through() {
        // lo = 0 → lower span is degenerate → t ≤ 0 returns t unchanged.
        assert_eq!(center_weighted(-0.0, 0.0, 1.0), -0.0);
    }
}
