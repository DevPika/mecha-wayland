use std::time::Instant;

use app::{App, Component, Emitted, OnChanged, Spawned};
use geometry::{Color, Corners, Insets};
use paint::{Paint, Quad};
use window::{Frame, InWindow, RequestFrame};

use crate::{
    AnimationTime, PaintAnimationSettings, Time,
    animatable::{Animatable, Running},
    settings,
};

/// The displayed paint, initialized from [`Paint`] at spawn and snapped when
/// its target cannot animate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimatedPaint(pub Paint);

impl Component for AnimatedPaint {}

/// Per-node paint transition state. Idle until a quad transition is started.
#[derive(Default)]
pub struct PaintTransition(Option<Running<Quad>>);

impl Component for PaintTransition {}

impl PaintTransition {
    /// Whether this node has a running quad transition.
    pub fn is_running(&self) -> bool {
        self.0.is_some()
    }
}

impl Animatable for Quad {
    fn max_delta(self, target: Self) -> f64 {
        [
            (self.color.r, target.color.r),
            (self.color.g, target.color.g),
            (self.color.b, target.color.b),
            (self.color.a, target.color.a),
            (self.border_color.r, target.border_color.r),
            (self.border_color.g, target.border_color.g),
            (self.border_color.b, target.border_color.b),
            (self.border_color.a, target.border_color.a),
            (self.radii.top_left, target.radii.top_left),
            (self.radii.top_right, target.radii.top_right),
            (self.radii.bottom_right, target.radii.bottom_right),
            (self.radii.bottom_left, target.radii.bottom_left),
            (self.border.top, target.border.top),
            (self.border.right, target.border.right),
            (self.border.bottom, target.border.bottom),
            (self.border.left, target.border.left),
        ]
        .into_iter()
        .map(|(a, b)| (f64::from(b) - f64::from(a)).abs())
        .fold(0.0, f64::max)
    }

    fn interpolate(self, target: Self, amount: f32) -> Self {
        let lerp = |a: f32, b: f32| a + (b - a) * amount;
        let color = |a: Color, b: Color| {
            Color::rgba(
                lerp(a.r, b.r),
                lerp(a.g, b.g),
                lerp(a.b, b.b),
                lerp(a.a, b.a),
            )
        };
        Self {
            color: color(self.color, target.color),
            border_color: color(self.border_color, target.border_color),
            radii: Corners::new(
                lerp(self.radii.top_left, target.radii.top_left),
                lerp(self.radii.top_right, target.radii.top_right),
                lerp(self.radii.bottom_right, target.radii.bottom_right),
                lerp(self.radii.bottom_left, target.radii.bottom_left),
            ),
            border: Insets::new(
                lerp(self.border.top, target.border.top),
                lerp(self.border.right, target.border.right),
                lerp(self.border.bottom, target.border.bottom),
                lerp(self.border.left, target.border.left),
            ),
            is_opaque: target.is_opaque,
        }
    }
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
                transition.0 = Some(Running::new(from, *to, now, configuration));
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
        let (quad, finished) = running.advance(now);
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
