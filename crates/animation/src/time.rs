//! Shared clock snapshots. Sampling records elapsed time; it does not schedule
//! work or wake a blocked runner when time passes.

use std::time::{Duration, Instant};

use app::prelude::*;

/// A monotonic clock sampled on [`PostTick`] and on each window's [`window::Frame`].
/// Reads remain constant until the next sample. `delta` spans consecutive
/// samples, not consecutive frames of any particular window.
#[derive(Debug)]
pub struct Time {
    started: Instant,
    last_update: Instant,
    delta: Duration,
}

impl Resource for Time {}

impl Time {
    /// The most recent sample. Use this and an animation's start time to
    /// measure its progress independently of other windows and ticks.
    pub fn now(&self) -> Instant {
        self.last_update
    }

    /// Time from resource creation to the latest sample, not to the instant of
    /// this call. Reading it does not advance the clock.
    pub fn elapsed(&self) -> Duration {
        self.last_update.duration_since(self.started)
    }

    /// Time between the latest two samples, initially zero. This is not a
    /// per-window frame delta: either a tick or any window's frame updates it.
    pub fn delta(&self) -> Duration {
        self.delta
    }

    /// Record a supplied monotonic instant. Production supplies `Instant::now()`;
    /// clock tests supply deterministic instants to avoid sleeps.
    fn update_at(&mut self, now: Instant) {
        self.delta = now.duration_since(self.last_update);
        self.last_update = now;
    }
}

impl Default for Time {
    /// Start the clock now with zero elapsed time and delta.
    fn default() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            last_update: now,
            delta: Duration::ZERO,
        }
    }
}

/// Sample the clock once per dispatched signal, before the consumers of that
/// sample run. Installed for both `PostTick` (before `LayoutDone` dispatch) and
/// `Frame` (before interpolation); the signal payload is deliberately unused.
pub(crate) fn update_time<S: Signal>(app: &mut App, _: &S) {
    app.resource_mut::<Time>().update_at(Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_store_delta_and_elapsed_and_reads_do_not_advance_them() {
        let mut time = Time::default();
        let start = time.now();
        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.elapsed(), Duration::ZERO);

        time.update_at(start + Duration::from_millis(10));
        assert_eq!(time.now(), start + Duration::from_millis(10));
        assert_eq!(time.delta(), Duration::from_millis(10));
        assert_eq!(time.elapsed(), Duration::from_millis(10));
        assert_eq!(time.delta(), time.delta());

        time.update_at(start + Duration::from_millis(25));
        assert_eq!(time.delta(), Duration::from_millis(15));
        assert_eq!(time.elapsed(), Duration::from_millis(25));
        time.update_at(time.now());
        assert_eq!(time.delta(), Duration::ZERO);
        assert_eq!(time.elapsed(), Duration::from_millis(25));
    }

    #[test]
    fn both_tick_and_frame_sample_the_clock() {
        let mut app = App::new();
        app.init_resource::<Time>()
            .system::<PostTick>(update_time)
            .system::<window::Frame>(update_time);
        let start = app.resource::<Time>().now();
        app.tick();
        let tick = app.resource::<Time>().now();
        assert!(tick >= start);
        assert_eq!(app.resource::<Time>().delta(), tick - start);
        app.signal(window::Frame(app.root()));
        app.flush();
        let frame = app.resource::<Time>().now();
        assert!(frame >= tick);
        assert_eq!(app.resource::<Time>().delta(), frame - tick);
        assert_eq!(app.resource::<Time>().elapsed(), frame - start);
    }
}
