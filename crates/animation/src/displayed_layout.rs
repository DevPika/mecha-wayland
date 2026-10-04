//! The renderer-facing geometry and its transitions from Taffy's resolved target.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use app::prelude::*;
use geometry::{Insets, Rect};
use layout::{ComputedLayout, LayoutDone, LayoutRoot};
use window::{Frame, InWindow, RequestFrame};

use crate::{AnimationTime, Time, settings};

/// The displayed box in its layout root's coordinates. Initialized from
/// [`ComputedLayout`] on first resolution, then maintained by the animation
/// module. Renderers read this box; frame-time values may be fractional pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Layout {
    pub rect: Rect,
    pub padding: Insets<f32>,
    pub border: Insets<f32>,
}

impl Component for Layout {}

impl From<ComputedLayout> for Layout {
    fn from(value: ComputedLayout) -> Self {
        Self {
            rect: value.rect,
            padding: value.padding,
            border: value.border,
        }
    }
}

impl Layout {
    /// The rect inside padding and border, each dimension clamped at zero.
    pub fn content(&self) -> Rect {
        self.rect.inset(Insets::new(
            self.padding.top + self.border.top,
            self.padding.right + self.border.right,
            self.padding.bottom + self.border.bottom,
            self.padding.left + self.border.left,
        ))
    }
}

/// Per-node runtime state. A node is unresolved until its layout root's pass
/// first visits it, even when that pass produces the default zero-sized box.
/// Running data is absent when idle and resets when the node is removed.
#[derive(Default)]
pub(crate) struct Transition {
    resolved: bool,
    running: Option<Running>,
}
impl Component for Transition {}

/// A transition's fixed starting point, destination, clock origin and settings.
/// Frame samples always interpolate these endpoints, not the previous frame's
/// output, so the result depends on elapsed time rather than frame count.
struct Running {
    from: Layout,
    target: Layout,
    started: Instant,
    duration: Duration,
    easing: fn(f32) -> f32,
}

/// Reconcile resolved targets with displayed layouts; do not advance time here.
///
/// For each node, this system:
/// - Copies the target and cancels a transition when animation is disabled,
///   there is no window, or the node is a layout root.
/// - Keeps a running transition if its destination has not changed.
/// - Otherwise starts or replaces a transition from the currently displayed
///   layout, or leaves the node idle if it already equals its target.
/// - Requests one frame per window with a running transition or a changed copy.
///
/// `LayoutDone` is queued by layout's `PostTick` system. By its dispatch, all
/// `PostTick` systems (including the clock sample) and the queued change-event
/// handlers have run. A later `PostTick` system could also do this job if ordered
/// after layout and the clock; this signal explicitly uses layout's completion
/// point instead. Writes here are after the normal `Layout` change drain.
///
/// The recomputed roots identify nodes receiving their first resolution, even
/// if their target stayed zero. All nodes are still reconciled on clean ticks:
/// settings can change without layout recomputation, and active animations
/// still need frames. Positive timing/easing changes alone do not replace a
/// running transition; it keeps its captured settings until its target changes.
/// Copies made after the normal `Layout` drain are emitted here, so consumers
/// see initial and snapped layouts before the requested frame.
pub(crate) fn on_layout_done(app: &mut App, done: &LayoutDone) {
    let now = app.resource::<Time>().now();
    // A zero-sized first resolution produces no ComputedLayout change event.
    // LayoutDone identifies the entire subtree visited by each layout pass.
    let resolved_this_pass: HashSet<NodeId> = done
        .roots
        .iter()
        .flat_map(|&root| std::iter::once(root).chain(app.descendants(root)))
        .collect();
    let nodes: Vec<NodeId> = std::iter::once(app.root())
        .chain(app.descendants(app.root()))
        .collect();
    let mut windows = Vec::new();
    for id in nodes {
        let target = Layout::from(*app.component::<ComputedLayout>(id).unwrap());
        let window = app.component::<InWindow>(id).unwrap().0;
        if !app.component::<Transition>(id).unwrap().resolved {
            if !resolved_this_pass.contains(&id) {
                continue;
            }
            app.component_mut::<Transition>(id).unwrap().resolved = true;
            let changed = app.component_mut::<Layout>(id).unwrap().set_if_neq(target);
            if changed
                && let Some(window) = window
                && !windows.contains(&window)
            {
                windows.push(window);
            }
            continue;
        }
        let configuration = settings(app, id).filter(|s| {
            !matches!(s.time, AnimationTime::Duration(d) if d.is_zero())
                && window.is_some()
                && !app.component::<LayoutRoot>(id).unwrap().0
        });
        let Some(configuration) = configuration else {
            let changed = app.component_mut::<Layout>(id).unwrap().set_if_neq(target);
            app.component_mut::<Transition>(id).unwrap().running = None;
            // Request directly even without a renderer listening to the
            // change notification queued below.
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
        if transition
            .running
            .as_ref()
            .is_some_and(|t| t.target == target)
        {
            // Keep the original start and clock across clean layout passes.
        } else if current != target {
            let duration = match configuration.time {
                AnimationTime::Duration(duration) => duration,
                AnimationTime::Speed(speed) => {
                    // This just finds the max distance from which the duration
                    // will be calculated
                    let distance = [
                        (current.rect.x(), target.rect.x()),
                        (current.rect.y(), target.rect.y()),
                        (current.rect.width(), target.rect.width()),
                        (current.rect.height(), target.rect.height()),
                        (current.padding.top, target.padding.top),
                        (current.padding.right, target.padding.right),
                        (current.padding.bottom, target.padding.bottom),
                        (current.padding.left, target.padding.left),
                        (current.border.top, target.border.top),
                        (current.border.right, target.border.right),
                        (current.border.bottom, target.border.bottom),
                        (current.border.left, target.border.left),
                    ]
                    .into_iter()
                    .map(|(from, to)| (f64::from(to) - f64::from(from)).abs())
                    .fold(0.0, f64::max);
                    Duration::try_from_secs_f64(distance / f64::from(speed))
                        .unwrap_or(Duration::MAX)
                }
            };
            transition.running = Some(Running {
                from: current,
                target,
                started: now,
                duration,
                easing: configuration.easing,
            });
        } else {
            transition.running = None;
        }
        if transition.running.is_some() && !windows.contains(&window) {
            windows.push(window);
        }
    }
    // Transition is private bookkeeping, not a consumer-facing change event.
    app.take_changed::<Transition>().for_each(drop);

    // The normal PostTick drain already ran. Notify consumers about displayed
    // Layout writes made here now, and clear their records so they don't fire
    // again on the next tick. This includes later snaps, not just first copies.
    let changed: Vec<NodeId> = app.take_changed::<Layout>().collect();
    if !changed.is_empty() {
        app.emit(OnChanged::<Layout>::new(), changed);
    }
    for window in windows {
        app.signal(RequestFrame(window));
    }
}

/// Advance existing transitions for the window whose drawing opportunity arrived.
/// The clock's `Frame` system has already sampled `Time`, and render runs after
/// this system. It neither discovers new targets nor requests another frame;
/// those jobs belong to `on_layout_done`. A frame is not a layout pass.
pub(crate) fn on_frame(app: &mut App, frame: &Frame) {
    advance(app, frame.0, app.resource::<Time>().now());
}

/// Sample one window's active transitions at `now`, writing only displayed layout.
/// Each transition uses elapsed time since its own start, not global `Time::delta`,
/// which may include intervening ticks or other windows' frames. Completion writes
/// the exact target and clears the running state. Other windows are untouched.
/// The explicit timestamp also lets tests sample progress without sleeping.
fn advance(app: &mut App, window: NodeId, now: Instant) {
    let (membership, mut layouts, mut transitions) =
        app.query::<(&InWindow, &mut Layout, &mut Transition)>();
    for (id, mut transition) in transitions.iter_mut() {
        if membership[id].0 != Some(window) {
            continue;
        }
        let Some(running) = &transition.running else {
            continue; // skip everything that doesn't have a currently running animation
        };
        let elapsed = now.duration_since(running.started);
        let finished = elapsed >= running.duration;
        let value = if finished {
            running.target
        } else {
            let progress = elapsed.as_secs_f32() / running.duration.as_secs_f32();
            interpolate(running.from, running.target, (running.easing)(progress))
        };
        layouts.get_mut(id).unwrap().set_if_neq(value); // set the layout here
        if finished {
            transition.running = None;
        }
    }
    drop((membership, layouts, transitions));
    app.take_changed::<Transition>().for_each(drop);
}

/// Blend every numeric layout field using the already-eased amount `t`.
/// This does not rerun Taffy or enforce intermediate layout constraints. The
/// amount is not clamped and the result is not rounded to device pixels here.
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
#[path = "tests.rs"]
mod tests;
