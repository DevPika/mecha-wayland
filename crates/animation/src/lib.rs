#![forbid(unsafe_code)]
//! Duration- or speed-based transitions from Taffy's [`ComputedLayout`] to displayed [`Layout`].
//!
//! Install after `LayoutModule` and `WindowModule`, and before `RenderModule`.
//! Installation sets `LayoutControl::externally_driven` automatically. This
//! module then maintains all displayed layouts: nonanimated nodes copy their
//! targets, while animated nodes interpolate.
//! Settings inherit from the nearest configured ancestor; a zero duration
//! overrides inheritance and snaps. Layout roots always snap. The first layout
//! resolution is copied immediately by the layout module. Later resolutions,
//! including a compositor resize during startup, can start transitions.
//!
//! After each layout pass, changed targets start transitions and active windows
//! request frames. On `Frame`, the clock is sampled and displayed geometry is
//! advanced before rendering. A replacement target restarts from the currently
//! displayed layout, without velocity matching. Hit testing still uses targets.
//!
//! # Scheduling
//!
//! There are two separate jobs, not two ways to advance an animation:
//!
//! - [`LayoutDone`] means the tick's layout pass has finished. Its system
//!   reconciles targets and settings with existing transitions, copies nodes
//!   that should not animate, and requests frames for windows needing work.
//!   The signal is sent even when no root needed recomputing, so disabling
//!   animation and continuing existing transitions do not require a new layout.
//! - [`Frame`] names one window with a drawing opportunity. Its system samples
//!   existing transitions into `Layout` before the renderer reads it. This is
//!   not a notification that pixels have reached the display.
//!
//! The normal order, with the modules installed as described above, is:
//!
//! ```text
//! PostTick:   layout resolves targets and queues LayoutDone; Time is sampled
//! LayoutDone: copy nonanimated targets, prepare transitions, request frames
//! Frame(w):   sample Time, advance w's transitions, then render w
//! ```
//!
//! These are queued signals, not a fixed-rate clock or necessarily adjacent
//! operations. The platform decides when requested frames can run. [`Time`]
//! only samples the clock; it does not install a timer or wake the runner.

use std::time::{Duration, Instant};

use app::prelude::*;
use geometry::{Insets, Rect};
use layout::{ComputedLayout, Layout, LayoutControl, LayoutDone, LayoutRoot};
use window::{Frame, InWindow, RequestFrame};

mod paint;
mod time;

pub use paint::{AnimatedPaint, PaintTransition};
pub use time::Time;

/// The animation types normally imported by a consumer.
pub mod prelude {
    pub use crate::{
        AnimatedPaint, AnimationModule, AnimationSettings, AnimationTime, PaintTransition, Time,
    };
}

/// How a transition's duration is determined.
#[derive(Debug, Clone, Copy)]
pub enum AnimationTime {
    /// Elapsed monotonic time from a transition's start to its exact target.
    /// Zero means copy immediately rather than start a transition.
    Duration(Duration),
    /// Nominal logical pixels per second, finite and strictly positive.
    ///
    /// Duration is the largest absolute change among position, size, padding,
    /// and border fields divided by this speed. All fields share that duration;
    /// diagonal position changes use the largest axis change, not path length.
    /// Easing still applies, so only linear easing gives constant field rates.
    /// Durations beyond the representable range saturate at `Duration::MAX`.
    Speed(f32),
}

/// A node's optional explicit animation configuration.
///
/// Every live node has this component once [`AnimationModule`] is installed.
/// `Default` contains `None`, meaning "inherit", not "disable animation".
/// The nearest explicit settings on the node or its ancestors win. If none
/// exist, the node's displayed layout copies its target without animating.
///
/// [`AnimationSettings::new`] contains `Some(settings)`, even for a zero
/// duration. An explicit zero duration therefore overrides an animated parent
/// and snaps; descendants inherit that choice unless they override it again.
#[derive(Debug, Clone, Copy, Default)]
pub struct AnimationSettings(Option<Settings>);

impl Component for AnimationSettings {}

impl AnimationSettings {
    /// Set this node's timing and easing, overriding inherited settings.
    ///
    /// `easing` accepts normalized elapsed time and returns the interpolation
    /// amount. Its output is not clamped, allowing overshoot. At completion,
    /// the exact target is assigned regardless of the easing function.
    /// Functions and noncapturing closures can be supplied.
    ///
    /// # Panics
    ///
    /// Panics if a speed is zero, negative, or nonfinite. Use zero duration to
    /// disable animation rather than zero speed.
    ///
    /// ```
    /// use animation::{AnimationSettings, AnimationTime};
    /// use std::time::Duration;
    ///
    /// let settings = AnimationSettings::new(
    ///     AnimationTime::Duration(Duration::from_millis(1500)),
    ///     |t| t * t,
    /// );
    /// ```
    pub fn new(time: AnimationTime, easing: fn(f32) -> f32) -> Self {
        if let AnimationTime::Speed(speed) = time {
            assert!(
                speed.is_finite() && speed > 0.0,
                "animation speed must be finite and strictly positive"
            );
        }
        Self(Some(Settings { time, easing }))
    }
}

/// Concrete configuration with no inheritance state. A running transition
/// captures its resolved duration and easing until it ends or is replaced.
#[derive(Debug, Clone, Copy)]
struct Settings {
    time: AnimationTime,
    easing: fn(f32) -> f32,
}

/// Per-node runtime state: `None` is idle, `Some` is a transition in progress.
/// Keeping the active data optional avoids dummy start times and layouts on
/// nodes that have never animated. The component resets when its node is removed.
#[derive(Default)]
struct Transition(Option<Running>);
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

/// Installs the clock, settings and transition components, and animation systems.
///
/// Install after `layout::LayoutModule` and `window::WindowModule`, and before
/// the renderer. Installation claims displayed-layout updates through
/// [`LayoutControl`]; layout still initializes every node's first resolved box.
///
/// The systems sample [`Time`] on [`PostTick`] and [`Frame`], reconcile layout
/// targets on [`LayoutDone`], and prepare paint transitions when target paint
/// changes. Both kinds of transitions advance on `Frame` before rendering.
/// There is no independent timer or frame-rate loop here.
pub struct AnimationModule;
impl Module for AnimationModule {
    /// Claim displayed layouts and register the systems in execution order.
    fn install(self, app: &mut App) {
        app.resource_mut::<LayoutControl>().externally_driven = true;
        app.init_resource::<Time>()
            .system::<PostTick>(time::update_time)
            .system::<Frame>(time::update_time)
            .register_component::<AnimationSettings>()
            .register_component::<Transition>()
            .register_component::<AnimatedPaint>()
            .register_component::<PaintTransition>()
            .system(paint::on_spawned)
            .system(on_layout_done)
            .system(paint::on_paint_changed)
            .system(on_frame)
            .system(paint::on_frame);
    }
}

/// Resolve a live node's explicit settings, then its nearest ancestor's.
/// Siblings are never consulted. Zero-duration settings still stop the search;
/// only `None` means continue inheriting. Nothing is copied into child settings.
fn settings(app: &App, id: NodeId) -> Option<Settings> {
    std::iter::once(id)
        .chain(app.ancestors(id))
        .find_map(|node| app.component::<AnimationSettings>(node)?.0)
}
// TODO: maybe remove that ^ function ? Surely there's a better function to
// write, there's only one use for this, and we shouldn't need to further
// filter the output.

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
/// The signal's dirty-root list is deliberately unused: settings can change
/// without any layout recomputation, and active animations still need frames
/// on clean ticks. Positive timing/easing changes alone do not replace a
/// running transition; it keeps its captured settings until its target changes.
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
            !matches!(s.time, AnimationTime::Duration(d) if d.is_zero())
                && window.is_some()
                && !app.component::<LayoutRoot>(id).unwrap().0
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
            transition.0 = Some(Running {
                from: current,
                target,
                started: now,
                duration,
                easing: configuration.easing,
            });
        } else {
            transition.0 = None;
        }
        if transition.0.is_some() && !windows.contains(&window) {
            windows.push(window);
        }
    }
    // Transition is private bookkeeping, not a consumer-facing change event.
    app.take_changed::<Transition>().for_each(drop);
    for window in windows {
        app.signal(RequestFrame(window));
    }
}

/// Advance existing transitions for the window whose drawing opportunity arrived.
/// The clock's `Frame` system has already sampled `Time`, and render runs after
/// this system. It neither discovers new targets nor requests another frame;
/// those jobs belong to `on_layout_done`. A frame is not a layout pass.
fn on_frame(app: &mut App, frame: &Frame) {
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
        let Some(running) = &transition.0 else {
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
            transition.0 = None;
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
mod tests;
