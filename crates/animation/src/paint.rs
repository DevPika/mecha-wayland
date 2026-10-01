use std::time::{Duration, Instant};

use app::{App, Component, Spawned};
use paint::{Paint, Quad};

/// The displayed paint copied from a newly spawned node's [`Paint`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimatedPaint(pub Paint);

impl Component for AnimatedPaint {}

/// Per-node paint transition state. Idle until a quad transition is started.
#[derive(Default)]
#[expect(dead_code, reason = "paint transition systems are not installed yet")]
pub struct PaintTransition(Option<Running>);

impl Component for PaintTransition {}

#[expect(dead_code, reason = "paint transition systems are not installed yet")]
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
