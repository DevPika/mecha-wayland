//! Value-level animation math and the shared clock for running transitions.

use std::time::{Duration, Instant};

use crate::{AnimationTime, Easing, Settings};

/// Fields that can be blended, with speed measured by their largest absolute
/// numeric change (not the length of a geometric path).
pub(crate) trait Animatable: Copy {
    fn max_delta(self, target: Self) -> f64;
    fn interpolate(self, target: Self, amount: f32) -> Self;
}

/// Fixed endpoints and timing for one transition. Every frame samples from
/// `from`, not the previous frame's value; retargeting creates a new `Running`.
pub(crate) struct Running<T: Animatable> {
    pub(crate) from: T,
    pub(crate) target: T,
    pub(crate) started: Instant,
    pub(crate) duration: Duration,
    pub(crate) easing: Easing,
}

impl<T: Animatable> Running<T> {
    pub(crate) fn new(from: T, target: T, started: Instant, settings: Settings) -> Self {
        let duration = match settings.time {
            AnimationTime::Duration(duration) => duration,
            AnimationTime::Speed(speed) => {
                Duration::try_from_secs_f64(from.max_delta(target) / f64::from(speed))
                    .unwrap_or(Duration::MAX)
            }
        };
        Self {
            from,
            target,
            started,
            duration,
            easing: settings.easing,
        }
    }

    /// Sample elapsed time and report completion. A finished transition writes
    /// the exact target, even when easing overshoots or has a different endpoint.
    pub(crate) fn advance(&self, now: Instant) -> (T, bool) {
        let elapsed = now.duration_since(self.started);
        if elapsed >= self.duration {
            (self.target, true)
        } else {
            let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
            (
                self.from
                    .interpolate(self.target, self.easing.resolve(progress)),
                false,
            )
        }
    }
}
