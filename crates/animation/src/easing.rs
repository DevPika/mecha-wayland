//! Named easings and parameterized cubic Bézier curves.

/// A normalized progress curve. Its output is not clamped: a custom function
/// or Bézier with control points outside the y range can overshoot. Animation
/// still writes its exact target when the transition completes.
#[derive(Debug, Clone, Copy)]
pub enum Easing {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuartic,
    EaseInQuintic,
    EaseInSextic,
    EaseInSeptic,
    EaseInOctic,
    /// A curve from (0, 0) to (1, 1), configured with [`Easing::cubic_bezier`].
    CubicBezier(CubicBezier),
    /// A function or noncapturing closure, with the same behavior as the
    /// original easing-function setting.
    Custom(fn(f32) -> f32),
}

/// Validated control points of a cubic Bézier easing curve.
#[derive(Debug, Clone, Copy)]
pub struct CubicBezier {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl Easing {
    /// Build a CSS-style cubic Bézier between (0, 0) and (1, 1).
    /// The x coordinates control timing, while y may leave `0..=1` to
    /// overshoot. To evaluate it, `resolve` finds the curve parameter whose
    /// x coordinate equals elapsed progress and returns its y coordinate.
    ///
    /// # Panics
    ///
    /// Panics unless all coordinates are finite and both x coordinates are
    /// within `0..=1`, which ensures the time axis is monotone.
    pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        assert!(
            [x1, y1, x2, y2].into_iter().all(f32::is_finite)
                && (0.0..=1.0).contains(&x1)
                && (0.0..=1.0).contains(&x2),
            "cubic Bézier controls must be finite and x coordinates within 0..=1"
        );
        Self::CubicBezier(CubicBezier { x1, y1, x2, y2 })
    }

    /// Convert normalized elapsed time to an interpolation amount.
    pub fn resolve(self, t: f32) -> f32 {
        match self {
            Self::Linear => linear(t),
            Self::EaseInQuad => quadratic(t),
            Self::EaseOutQuad => 1.0 - quadratic(1.0 - t),
            Self::EaseInOutQuad => in_out(t, quadratic),
            Self::EaseInCubic => cubic(t),
            Self::EaseOutCubic => 1.0 - cubic(1.0 - t),
            Self::EaseInOutCubic => in_out(t, cubic),
            Self::EaseInQuartic => quartic(t),
            Self::EaseInQuintic => quintic(t),
            Self::EaseInSextic => sextic(t),
            Self::EaseInSeptic => septic(t),
            Self::EaseInOctic => octic(t),
            Self::CubicBezier(curve) => curve.resolve(t),
            Self::Custom(easing) => easing(t),
        }
    }
}

impl CubicBezier {
    fn resolve(self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        // x is monotone when x1 and x2 are in 0..=1. Bisection remains
        // stable even when its derivative vanishes at either endpoint.
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..32 {
            let u = (low + high) * 0.5;
            if bezier(f64::from(self.x1), f64::from(self.x2), u) < f64::from(t) {
                low = u;
            } else {
                high = u;
            }
        }
        bezier(f64::from(self.y1), f64::from(self.y2), (low + high) * 0.5) as f32
    }
}

fn bezier(p1: f64, p2: f64, u: f64) -> f64 {
    let inv = 1.0 - u;
    3.0 * inv * inv * u * p1 + 3.0 * inv * u * u * p2 + u * u * u
}

fn in_out(t: f32, power: fn(f32) -> f32) -> f32 {
    if t < 0.5 {
        power(2.0 * t) * 0.5
    } else {
        1.0 - power(2.0 - 2.0 * t) * 0.5
    }
}

fn linear(t: f32) -> f32 {
    t
}

fn quadratic(t: f32) -> f32 {
    t * t
}

fn cubic(t: f32) -> f32 {
    t * t * t
}

fn quartic(t: f32) -> f32 {
    let t2 = t * t;
    t2 * t2
}

fn quintic(t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    t3 * t2
}

fn sextic(t: f32) -> f32 {
    let t3 = t * t * t;
    t3 * t3
}

fn septic(t: f32) -> f32 {
    let t3 = t * t * t;
    let t6 = t3 * t3;
    t6 * t
}

fn octic(t: f32) -> f32 {
    let t2 = t * t;
    let t4 = t2 * t2;
    t4 * t4
}

#[cfg(test)]
mod tests {
    use super::Easing;

    #[test]
    fn named_curves_have_expected_progress() {
        for (curve, midpoint) in [
            (Easing::Linear, 0.5),
            (Easing::EaseInQuad, 0.25),
            (Easing::EaseOutQuad, 0.75),
            (Easing::EaseInOutQuad, 0.5),
            (Easing::EaseInCubic, 0.125),
            (Easing::EaseOutCubic, 0.875),
            (Easing::EaseInOutCubic, 0.5),
            (Easing::EaseInQuartic, 0.0625),
            (Easing::EaseInQuintic, 0.03125),
            (Easing::EaseInSextic, 0.015625),
            (Easing::EaseInSeptic, 0.0078125),
            (Easing::EaseInOctic, 0.00390625),
            (Easing::Custom(|t| t * 2.0), 1.0),
        ] {
            assert_eq!(curve.resolve(0.5), midpoint, "{curve:?}");
        }
    }

    #[test]
    fn bezier_solves_the_time_axis_not_just_the_curve_parameter() {
        let linear = Easing::cubic_bezier(0.0, 0.0, 1.0, 1.0);
        let ease_in = Easing::cubic_bezier(0.42, 0.0, 1.0, 1.0);
        let ease_out = Easing::cubic_bezier(0.0, 0.0, 0.58, 1.0);
        for t in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            assert!((linear.resolve(t) - t).abs() < 1e-6);
        }
        assert!(ease_in.resolve(0.5) < 0.5);
        assert!(ease_out.resolve(0.5) > 0.5);
        // With both x controls at zero, x(u) = u³: elapsed 1/8 means u = 1/2.
        assert!((Easing::cubic_bezier(0.0, 0.0, 0.0, 1.0).resolve(0.125) - 0.5).abs() < 1e-6);
        assert_eq!(ease_in.resolve(0.0), 0.0);
        assert_eq!(ease_out.resolve(1.0), 1.0);
        assert!(Easing::cubic_bezier(0.5, -1.0, 0.5, 2.0).resolve(0.2) < 0.0);
    }

    #[test]
    fn rejects_invalid_control_points() {
        for (x1, y1, x2, y2) in [
            (-0.1, 0.0, 0.5, 1.0),
            (0.5, 0.0, 1.1, 1.0),
            (f32::NAN, 0.0, 0.5, 1.0),
            (0.5, f32::INFINITY, 0.5, 1.0),
            (0.5, 0.0, 0.5, f32::NEG_INFINITY),
        ] {
            assert!(std::panic::catch_unwind(|| Easing::cubic_bezier(x1, y1, x2, y2)).is_err());
        }
    }
}
