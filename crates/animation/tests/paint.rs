use std::time::Duration;

use animation::{
    AnimatedPaint, AnimationModule, AnimationSettings, AnimationTime, PaintTransition, Time,
};
use app::prelude::*;
use geometry::{Color, Corners, Insets};
use layout::LayoutModule;
use paint::{Paint, PaintModule, Quad};
use window::{Frame, FrameRequested, WindowModule, window};

struct Leaf;
struct Painted(Paint);
impl Build for Painted {
    type Widget = Leaf;
}
impl Widget for Leaf {
    type Builder = Painted;
    fn build(b: Painted, me: Handle<Self>, spawner: &mut Spawner<'_, Self>) -> Self {
        *spawner.component_mut::<Paint>(me).unwrap() = b.0;
        Leaf
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(AnimationModule);
    app
}

#[test]
fn spawned_nodes_copy_their_final_paint_after_build() {
    let mut app = app();
    let quad = Paint::Quad(Quad::new(Color::WHITE));
    let first = app.spawn_with(app.root(), Painted(quad.clone()), (Paint::None,));
    let run = Paint::Monochrome(vec![]);
    let second = app.spawn_with(app.root(), Painted(run.clone()), (Paint::None,));
    let third = app.spawn(app.root(), Painted(Paint::None));
    app.flush();

    assert_eq!(
        app.component::<AnimatedPaint>(first),
        Some(&AnimatedPaint(quad))
    );
    assert!(app.component::<PaintTransition>(first).is_some());
    assert_eq!(
        app.component::<AnimatedPaint>(second),
        Some(&AnimatedPaint(run))
    );
    assert_eq!(
        app.component::<AnimatedPaint>(third),
        Some(&AnimatedPaint(Paint::None))
    );
}

#[test]
fn removed_nodes_do_not_leak_displayed_paint_to_reused_slots() {
    let mut app = app();
    let first = app.spawn(app.root(), Painted(Paint::Quad(Quad::new(Color::WHITE))));
    app.flush();
    app.remove(first);
    let replacement = app.spawn(app.root(), Painted(Paint::None));
    assert_eq!(replacement.id().slot(), first.id().slot());
    app.flush();
    assert_eq!(
        app.component::<AnimatedPaint>(replacement),
        Some(&AnimatedPaint(Paint::None))
    );
    assert!(app.component::<PaintTransition>(replacement).is_some());
}

#[derive(Default)]
struct Requests(Vec<NodeId>);
impl Resource for Requests {}

#[test]
fn changed_quads_start_from_displayed_paint_on_the_next_tick() {
    let mut app = app();
    app.init_resource::<Requests>()
        .system(|app, request: &FrameRequested| {
            app.resource_mut::<Requests>().0.push(request.0);
        });
    let window = app.spawn(app.root(), window());
    let first = Paint::Quad(Quad::new(Color::BLACK));
    let panel = app.spawn_with(
        window,
        Painted(first.clone()),
        (AnimationSettings::new(
            AnimationTime::Duration(Duration::from_secs(1)),
            |t| t,
        ),),
    );
    app.tick();
    assert!(
        !app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<AnimatedPaint>(panel).unwrap().0, first);

    let next = Paint::Quad(Quad::new(Color::WHITE));
    *app.component_mut::<Paint>(panel).unwrap() = next.clone();
    app.flush();
    assert!(
        !app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
    app.tick();
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<AnimatedPaint>(panel).unwrap().0, first);
    assert_eq!(app.resource::<Requests>().0, [window.id()]);

    // A repeated write of the same target does not restart or snap the transition.
    *app.component_mut::<Paint>(panel).unwrap() = next;
    app.tick();
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<AnimatedPaint>(panel).unwrap().0, first);
}

#[test]
fn unsupported_paint_and_speed_snap_and_cancel_a_running_transition() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let first = Paint::Quad(Quad::new(Color::BLACK));
    let panel = app.spawn_with(
        window,
        Painted(first),
        (AnimationSettings::new(
            AnimationTime::Duration(Duration::from_secs(1)),
            |t| t,
        ),),
    );
    app.tick();
    let white = Paint::Quad(Quad::new(Color::WHITE));
    *app.component_mut::<Paint>(panel).unwrap() = white;
    app.tick();
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );

    *app.component_mut::<AnimationSettings>(panel).unwrap() =
        AnimationSettings::new(AnimationTime::Speed(100.0), |t| t);
    let black = Paint::Quad(Quad::new(Color::BLACK));
    *app.component_mut::<Paint>(panel).unwrap() = black.clone();
    app.tick();
    assert_eq!(app.component::<AnimatedPaint>(panel).unwrap().0, black);
    assert!(
        !app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );

    *app.component_mut::<AnimationSettings>(panel).unwrap() =
        AnimationSettings::new(AnimationTime::Duration(Duration::from_secs(1)), |t| t);
    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(Quad::new(Color::WHITE));
    app.tick();
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );

    let run = Paint::Monochrome(vec![]);
    *app.component_mut::<Paint>(panel).unwrap() = run.clone();
    app.tick();
    assert_eq!(app.component::<AnimatedPaint>(panel).unwrap().0, run);
    assert!(
        !app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );

    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(Quad::new(Color::WHITE));
    app.tick();
    assert_eq!(
        app.component::<AnimatedPaint>(panel).unwrap().0,
        Paint::Quad(Quad::new(Color::WHITE))
    );
    assert!(
        !app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
}

#[test]
fn paint_without_animation_settings_snaps() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let panel = app.spawn(window, Painted(Paint::Quad(Quad::new(Color::BLACK))));
    app.tick();
    let target = Paint::Quad(Quad::new(Color::WHITE));
    *app.component_mut::<Paint>(panel).unwrap() = target.clone();
    app.tick();
    assert_eq!(app.component::<AnimatedPaint>(panel).unwrap().0, target);
    assert!(
        !app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
}

#[test]
fn inherited_duration_animates_but_explicit_zero_duration_snaps() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    *app.component_mut::<AnimationSettings>(window).unwrap() =
        AnimationSettings::new(AnimationTime::Duration(Duration::from_secs(1)), |t| t);
    let first = Paint::Quad(Quad::new(Color::BLACK));
    let inherited = app.spawn(window, Painted(first.clone()));
    let disabled = app.spawn_with(
        window,
        Painted(first.clone()),
        (AnimationSettings::new(
            AnimationTime::Duration(Duration::ZERO),
            |t| t,
        ),),
    );
    app.tick();
    let target = Paint::Quad(Quad::new(Color::WHITE));
    *app.component_mut::<Paint>(inherited).unwrap() = target.clone();
    *app.component_mut::<Paint>(disabled).unwrap() = target.clone();
    app.tick();
    assert!(
        app.component::<PaintTransition>(inherited)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<AnimatedPaint>(inherited).unwrap().0, first);
    assert!(
        !app.component::<PaintTransition>(disabled)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<AnimatedPaint>(disabled).unwrap().0, target);
}

#[test]
fn frame_interpolates_every_numeric_quad_field_with_easing() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let duration = Duration::from_secs(60);
    let panel = app.spawn_with(
        window,
        Painted(Paint::Quad(Quad::default())),
        (AnimationSettings::new(
            AnimationTime::Duration(duration),
            |t| t * t,
        ),),
    );
    app.tick();
    let target = Quad {
        color: Color::rgba(1.0, 0.5, 0.25, 0.75),
        radii: Corners::new(20.0, 30.0, 40.0, 50.0),
        border: Insets::new(2.0, 4.0, 6.0, 8.0),
        border_color: Color::rgba(0.5, 1.0, 0.25, 0.75),
        is_opaque: false,
    };
    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(target);
    app.tick();
    let started = app.resource::<Time>().now();

    app.signal(Frame(window.id()));
    app.flush();
    let progress = (app.resource::<Time>().now() - started).as_secs_f32() / duration.as_secs_f32();
    assert!(progress < 1.0);
    let t = progress * progress;
    assert_eq!(
        app.component::<AnimatedPaint>(panel).unwrap().0,
        Paint::Quad(Quad {
            color: Color::rgba(t, 0.5 * t, 0.25 * t, 0.75 * t),
            radii: Corners::new(20.0 * t, 30.0 * t, 40.0 * t, 50.0 * t),
            border: Insets::new(2.0 * t, 4.0 * t, 6.0 * t, 8.0 * t),
            border_color: Color::rgba(0.5 * t, t, 0.25 * t, 0.75 * t),
            is_opaque: false,
        })
    );
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<Paint>(panel).unwrap(), &Paint::Quad(target));
}

#[test]
fn frames_only_advance_their_window_and_completion_writes_the_exact_target() {
    let mut app = app();
    let a = app.spawn(app.root(), window());
    let b = app.spawn(app.root(), window());
    let initial = Paint::Quad(Quad::new(Color::BLACK));
    let settings = AnimationSettings::new(AnimationTime::Duration(Duration::from_nanos(1)), |t| {
        t * 0.5
    });
    let first = app.spawn_with(a, Painted(initial.clone()), (settings,));
    let second = app.spawn_with(b, Painted(initial.clone()), (settings,));
    app.tick();
    let target = Paint::Quad(Quad::new(Color::WHITE));
    *app.component_mut::<Paint>(first).unwrap() = target.clone();
    *app.component_mut::<Paint>(second).unwrap() = target.clone();
    app.tick();

    app.signal(Frame(a.id()));
    app.flush();
    assert_eq!(app.component::<AnimatedPaint>(first).unwrap().0, target);
    assert!(
        !app.component::<PaintTransition>(first)
            .unwrap()
            .is_running()
    );
    assert_eq!(app.component::<AnimatedPaint>(second).unwrap().0, initial);
    assert!(
        app.component::<PaintTransition>(second)
            .unwrap()
            .is_running()
    );

    app.signal(Frame(b.id()));
    app.flush();
    assert_eq!(app.component::<AnimatedPaint>(second).unwrap().0, target);
    assert!(
        !app.component::<PaintTransition>(second)
            .unwrap()
            .is_running()
    );
}

#[test]
fn retargeting_a_quad_starts_from_its_last_displayed_frame() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let duration = Duration::from_secs(60);
    let panel = app.spawn_with(
        window,
        Painted(Paint::Quad(Quad::new(Color::BLACK))),
        (AnimationSettings::new(
            AnimationTime::Duration(duration),
            |t| t,
        ),),
    );
    app.tick();
    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(Quad::new(Color::WHITE));
    app.tick();
    app.signal(Frame(window.id()));
    app.flush();
    let Paint::Quad(shown) = app.component::<AnimatedPaint>(panel).unwrap().0 else {
        panic!("a quad transition displays a quad");
    };

    let target = Quad::new(Color::rgb(0.0, 0.0, 1.0));
    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(target);
    app.tick();
    let started = app.resource::<Time>().now();
    app.signal(Frame(window.id()));
    app.flush();
    let progress = (app.resource::<Time>().now() - started).as_secs_f32() / duration.as_secs_f32();
    assert!(progress < 1.0);
    let lerp = |a: f32, b: f32| a + (b - a) * progress;
    assert_eq!(
        app.component::<AnimatedPaint>(panel).unwrap().0,
        Paint::Quad(Quad {
            color: Color::rgba(
                lerp(shown.color.r, target.color.r),
                lerp(shown.color.g, target.color.g),
                lerp(shown.color.b, target.color.b),
                lerp(shown.color.a, target.color.a),
            ),
            ..target
        })
    );
}
