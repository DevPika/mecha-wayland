#![forbid(unsafe_code)]
//! Duration-based transitions from Taffy's `ComputedLayout` to displayed `Layout`.
//!
//! Install after `LayoutModule` and `WindowModule`, and before `RenderModule`.
//! Installation sets `LayoutControl::externally_driven` automatically. This
//! module then maintains all displayed layouts: nonanimated nodes copy their
//! targets, while animated nodes interpolate.
//! Settings inherit from the nearest configured ancestor; a zero duration
//! overrides inheritance and snaps. Layout roots always snap. The first layout
//! is initialized by the layout module without an entrance animation.
//!
//! After each layout pass, changed targets start transitions and active windows
//! request frames. On `Frame`, the clock is sampled and displayed geometry is
//! advanced before rendering. A replacement target restarts from the currently
//! displayed layout, without velocity matching. Hit testing still uses targets.

use std::time::{Duration, Instant};

use app::prelude::*;
use geometry::{Insets, Rect};
use layout::{ComputedLayout, Layout, LayoutControl, LayoutDone, LayoutRoot};
use window::{Frame, InWindow, RequestFrame};

mod time;

pub use time::Time;

pub mod prelude {
    pub use crate::{AnimationModule, AnimationSettings, AnimationTime, Time};
}

#[derive(Debug, Clone, Copy)]
pub enum AnimationTime {
    Duration(Duration),
}

/// Default means inherit. Explicit settings replace the nearest ancestor's
/// settings; an explicit zero duration disables transitions for the subtree.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimationSettings(Option<Settings>);

impl Component for AnimationSettings {}

impl AnimationSettings {
    pub fn new(time: AnimationTime, easing: fn(f32) -> f32) -> Self {
        let AnimationTime::Duration(duration) = time;
        Self(Some(Settings { duration, easing }))
    }
}
// impl Default for AnimationSettings

#[derive(Debug, Clone, Copy)]
struct Settings {
    duration: Duration,
    easing: fn(f32) -> f32,
}

#[derive(Default)]
struct Transition(Option<Running>);
impl Component for Transition {}

struct Running {
    from: Layout,
    target: Layout,
    started: Instant,
    settings: Settings,
}

pub struct AnimationModule;
impl Module for AnimationModule {
    fn install(self, app: &mut App) {
        app.resource_mut::<LayoutControl>().externally_driven = true;
        app.init_resource::<Time>()
            .system::<PostTick>(time::update_time)
            .system::<Frame>(time::update_time)
            .register_component::<AnimationSettings>()
            .register_component::<Transition>()
            .system(on_layout_done)
            .system(on_frame);
    }
}

fn settings(app: &App, id: NodeId) -> Option<Settings> {
    std::iter::once(id)
        .chain(app.ancestors(id))
        .find_map(|node| app.component::<AnimationSettings>(node)?.0)
}

fn on_layout_done(app: &mut App, _: &LayoutDone) {
    let now = app.resource::<Time>().now();
    let nodes: Vec<NodeId> = std::iter::once(app.root())
        .chain(app.descendants(app.root()))
        .collect();
    let mut windows = Vec::new();
    for id in nodes {
        let target = Layout::from(*app.component::<ComputedLayout>(id).unwrap());
        let window = app.component::<InWindow>(id).unwrap().0;
        let configuration = settings(app, id).filter(|s| {
            !s.duration.is_zero() && window.is_some() && !app.component::<LayoutRoot>(id).unwrap().0
        });
        let Some(configuration) = configuration else {
            let changed = app.component_mut::<Layout>(id).unwrap().set_if_neq(target);
            app.component_mut::<Transition>(id).unwrap().0 = None;
            // Layout's change drain already ran. Request now rather than
            // waiting for another tick to display this copy.
            if changed
                && let Some(window) = window
                && !windows.contains(&window)
            {
                windows.push(window);
            }
            continue;
        };
        let current = *app.component::<Layout>(id).unwrap();
        let window = window.unwrap();
        let mut transition = app.component_mut::<Transition>(id).unwrap();
        if transition.0.as_ref().is_some_and(|t| t.target == target) {
            // Keep the original start and clock across clean layout passes.
        } else if current != target {
            transition.0 = Some(Running {
                from: current,
                target,
                started: now,
                settings: configuration,
            });
        } else {
            transition.0 = None;
        }
        if transition.0.is_some() && !windows.contains(&window) {
            windows.push(window);
        }
    }
    app.take_changed::<Transition>().for_each(drop);
    for window in windows {
        app.signal(RequestFrame(window));
    }
}

fn on_frame(app: &mut App, frame: &Frame) {
    advance(app, frame.0, app.resource::<Time>().now());
}

fn advance(app: &mut App, window: NodeId, now: Instant) {
    let (membership, mut layouts, mut transitions) =
        app.query::<(&InWindow, &mut Layout, &mut Transition)>();
    for (id, mut transition) in transitions.iter_mut() {
        if membership[id].0 != Some(window) {
            continue;
        }
        let Some(running) = &transition.0 else {
            continue;
        };
        let elapsed = now.duration_since(running.started);
        let finished = elapsed >= running.settings.duration;
        let value = if finished {
            running.target
        } else {
            let progress = elapsed.as_secs_f32() / running.settings.duration.as_secs_f32();
            interpolate(
                running.from,
                running.target,
                (running.settings.easing)(progress),
            )
        };
        layouts.get_mut(id).unwrap().set_if_neq(value); // set the layout here
        if finished {
            transition.0 = None;
        }
    }
    drop((membership, layouts, transitions));
    app.take_changed::<Transition>().for_each(drop);
}

fn interpolate(from: Layout, to: Layout, t: f32) -> Layout {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    let insets = |a: Insets<f32>, b: Insets<f32>| {
        Insets::new(
            lerp(a.top, b.top),
            lerp(a.right, b.right),
            lerp(a.bottom, b.bottom),
            lerp(a.left, b.left),
        )
    };
    Layout {
        rect: Rect::new(
            lerp(from.rect.x(), to.rect.x()),
            lerp(from.rect.y(), to.rect.y()),
            lerp(from.rect.width(), to.rect.width()),
            lerp(from.rect.height(), to.rect.height()),
        ),
        padding: insets(from.padding, to.padding),
        border: insets(from.border, to.border),
    }
}

#[cfg(test)]
mod tests;
