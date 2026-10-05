use std::time::{Duration, Instant};

use app::{App, Component, Emitted, OnChanged, Spawned};
use geometry::{Color, Corners, Insets};
use paint::{Paint, Quad};
use window::{Frame, InWindow, RequestFrame};

use crate::{AnimationTime, Easing, PaintAnimationSettings, Time, settings};

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
    easing: Easing,
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

        // duration == zero is the same as snapping
        let configuration = settings::<PaintAnimationSettings>(app, id).filter(|s| {
            !matches!(s.time, AnimationTime::Duration(duration) if duration.is_zero())
                && window.is_some()
        });
        let from = match &app.component::<AnimatedPaint>(id).unwrap().0 {
            Paint::Quad(quad) => Some(*quad),
            _ => None,
        };
        if let (Some(from), Paint::Quad(to), Some(configuration)) =
            (from, &target, configuration)
        {
            let mut transition = app.component_mut::<PaintTransition>(id).unwrap();
            if transition.0.as_ref().is_some_and(|t| t.target == *to) {
                // An equal target keeps its original timing and starting point.
            } else if from != *to {
                let duration = match configuration.time {
                    AnimationTime::Duration(duration) => duration,
                    AnimationTime::Speed(speed) => {
                        let distance = [
                            (from.color.r, to.color.r),
                            (from.color.g, to.color.g),
                            (from.color.b, to.color.b),
                            (from.color.a, to.color.a),
                            (from.border_color.r, to.border_color.r),
                            (from.border_color.g, to.border_color.g),
                            (from.border_color.b, to.border_color.b),
                            (from.border_color.a, to.border_color.a),
                            (from.radii.top_left, to.radii.top_left),
                            (from.radii.top_right, to.radii.top_right),
                            (from.radii.bottom_right, to.radii.bottom_right),
                            (from.radii.bottom_left, to.radii.bottom_left),
                            (from.border.top, to.border.top),
                            (from.border.right, to.border.right),
                            (from.border.bottom, to.border.bottom),
                            (from.border.left, to.border.left),
                        ]
                        .into_iter()
                        .map(|(a, b)| (f64::from(b) - f64::from(a)).abs())
                        .fold(0.0, f64::max);
                        Duration::try_from_secs_f64(distance / f64::from(speed))
                            .unwrap_or(Duration::MAX)
                    }
                };
                transition.0 = Some(Running {
                    from,
                    target: *to,
                    started: now,
                    duration,
                    easing: configuration.easing,
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

/// Advance this window's quad transitions before the renderer reads displayed paint.
pub(crate) fn on_frame(app: &mut App, frame: &Frame) {
    advance(app, frame.0, app.resource::<Time>().now());
}

fn advance(app: &mut App, window: app::NodeId, now: Instant) {
    let (membership, mut paints, mut transitions) =
        app.query::<(&InWindow, &mut AnimatedPaint, &mut PaintTransition)>();
    for (id, mut transition) in transitions.iter_mut() {
        if membership[id].0 != Some(window) {
            continue;
        }
        let Some(running) = &transition.0 else {
            continue;
        };
        let elapsed = now.duration_since(running.started);
        let finished = elapsed >= running.duration;
        let quad = if finished {
            running.target
        } else {
            let progress = elapsed.as_secs_f32() / running.duration.as_secs_f32();
            interpolate(running.from, running.target, running.easing.resolve(progress))
        };
        paints
            .get_mut(id)
            .unwrap()
            .set_if_neq(AnimatedPaint(Paint::Quad(quad)));
        if finished {
            transition.0 = None;
        }
    }
    drop((membership, paints, transitions));
    app.take_changed::<PaintTransition>().for_each(drop);
}

fn interpolate(from: Quad, to: Quad, t: f32) -> Quad {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let color = |a: Color, b: Color| {
        Color::rgba(
            lerp(a.r, b.r),
            lerp(a.g, b.g),
            lerp(a.b, b.b),
            lerp(a.a, b.a),
        )
    };
    Quad {
        color: color(from.color, to.color),
        border_color: color(from.border_color, to.border_color),
        radii: Corners::new(
            lerp(from.radii.top_left, to.radii.top_left),
            lerp(from.radii.top_right, to.radii.top_right),
            lerp(from.radii.bottom_right, to.radii.bottom_right),
            lerp(from.radii.bottom_left, to.radii.bottom_left),
        ),
        border: Insets::new(
            lerp(from.border.top, to.border.top),
            lerp(from.border.right, to.border.right),
            lerp(from.border.bottom, to.border.bottom),
            lerp(from.border.left, to.border.left),
        ),
        is_opaque: to.is_opaque,
    }
}
