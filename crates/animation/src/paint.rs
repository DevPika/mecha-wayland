use std::time::{Duration, Instant};

use app::{App, Component, Emitted, OnChanged, Spawned};
use paint::{Paint, Quad};
use window::{InWindow, RequestFrame};

use crate::{AnimationTime, Time, settings};

/// The displayed paint, initialized from [`Paint`] at spawn and snapped when
/// its target cannot animate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimatedPaint(pub Paint);

impl Component for AnimatedPaint {}

/// Per-node paint transition state. Idle until a quad transition is started.
#[derive(Default)]
pub struct PaintTransition(Option<Running>);

impl Component for PaintTransition {}

impl PaintTransition {
    /// Whether this node has a running quad transition.
    pub fn is_running(&self) -> bool {
        self.0.is_some()
    }
}

struct Running {
    from: Quad,
    target: Quad,
    started: Instant,
    duration: Duration,
    easing: fn(f32) -> f32,
}

pub(crate) fn on_spawned(app: &mut App, spawned: &Spawned) {
    let Some(paint) = app.component::<Paint>(spawned.id).cloned() else {
        return;
    };
    app.component_mut::<AnimatedPaint>(spawned.id)
        .unwrap()
        .set_if_neq(AnimatedPaint(paint));
}

/// Reconcile only nodes whose target paint changed since the last `PostTick`.
pub(crate) fn on_paint_changed(app: &mut App, changed: &Emitted<OnChanged<Paint>>) {
    let now = app.resource::<Time>().now();
    let mut windows = Vec::new();
    for &id in changed.targets.iter() {
        let Some(target) = app.component::<Paint>(id).cloned() else {
            continue;
        };
        let window = app.component::<InWindow>(id).unwrap().0;

        let duration = settings(app, id).and_then(|s| match s.time {
				// duration == zero is the same as snapping
            AnimationTime::Duration(duration) if !duration.is_zero() && window.is_some() => {
                Some((duration, s.easing))
            }
            AnimationTime::Duration(_) | AnimationTime::Speed(_) => None,
				// TODO: speed snaps for now, impl Animatable will give us a max
				// distance and speed -> duration function
        });
        let from = match &app.component::<AnimatedPaint>(id).unwrap().0 {
            Paint::Quad(quad) => Some(*quad),
            _ => None,
        };
        if let (Some(from), Paint::Quad(to), Some((duration, easing))) = (from, &target, duration) {
            let mut transition = app.component_mut::<PaintTransition>(id).unwrap();
            if transition.0.as_ref().is_some_and(|t| t.target == *to) {
                // An equal target keeps its original timing and starting point.
            } else if from != *to {
                transition.0 = Some(Running {
                    from,
                    target: *to,
                    started: now,
                    duration,
                    easing,
                });
            } else {
					 // TODO: This branch should never execute because transition.0
					 // should be None anyways
                transition.0 = None;
            }
            if transition.is_running() {
                let window = window.unwrap();
                if !windows.contains(&window) {
                    windows.push(window);
                }
            }
        } else {
				// TODO: FOR NOW ONLY we copy Paint to AnimatedPaint if the target
				// isn't a quad
            let copied = app
                .component_mut::<AnimatedPaint>(id)
                .unwrap()
                .set_if_neq(AnimatedPaint(target));
            app.component_mut::<PaintTransition>(id).unwrap().0 = None;
            if copied
                && let Some(window) = window
                && !windows.contains(&window)
            {
                windows.push(window); // request new frame for this window so that the copy happens
            }
        }
    }

	 // take care of bookeeping
	 // this would generate OnChanged<PaintTransition> otherwise, which is
	 // rather useless
    app.take_changed::<PaintTransition>().for_each(drop);

	 // Request frames to start the animation
    for window in windows {
        app.signal(RequestFrame(window));
    }
}
