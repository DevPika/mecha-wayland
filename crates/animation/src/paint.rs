use app::{App, Component, Spawned};
use paint::Paint;

/// The displayed paint copied from a newly spawned node's [`Paint`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimatedPaint(pub Paint);

impl Component for AnimatedPaint {}

pub(crate) fn on_spawned(app: &mut App, spawned: &Spawned) {
    let Some(paint) = app.component::<Paint>(spawned.id).cloned() else {
        return;
    };
    app.component_mut::<AnimatedPaint>(spawned.id)
        .unwrap()
        .set_if_neq(AnimatedPaint(paint));
}
