use std::time::Duration;

use animation::{
    AnimatedPaint, AnimationModule, AnimationTime, Easing, Layout, LayoutAnimationSettings,
    PaintAnimationSettings, PaintTransition, Time,
};
use app::prelude::*;
use geometry::{Color, Corners, Insets};
use layout::{ComputedLayout, LayoutModule, LayoutStyle, auto, px};
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
        (PaintAnimationSettings::custom(
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
fn unsupported_paint_snaps_and_cancels_a_running_transition() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let first = Paint::Quad(Quad::new(Color::BLACK));
    let panel = app.spawn_with(
        window,
        Painted(first),
        (PaintAnimationSettings::custom(
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

    *app.component_mut::<PaintAnimationSettings>(panel).unwrap() =
        PaintAnimationSettings::custom(AnimationTime::Speed(100.0), |t| t);
    let green = Paint::Quad(Quad::new(Color::rgb(0.0, 1.0, 0.0)));
    *app.component_mut::<Paint>(panel).unwrap() = green;
    app.tick();
    assert_eq!(
        app.component::<AnimatedPaint>(panel).unwrap().0,
        Paint::Quad(Quad::new(Color::BLACK))
    );
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );

    *app.component_mut::<PaintAnimationSettings>(panel).unwrap() =
        PaintAnimationSettings::custom(AnimationTime::Duration(Duration::from_secs(1)), |t| t);
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
    *app.component_mut::<PaintAnimationSettings>(window).unwrap() =
        PaintAnimationSettings::custom(AnimationTime::Duration(Duration::from_secs(1)), |t| t);
    let first = Paint::Quad(Quad::new(Color::BLACK));
    let inherited = app.spawn(window, Painted(first.clone()));
    let disabled = app.spawn_with(
        window,
        Painted(first.clone()),
        (PaintAnimationSettings::custom(
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
fn layout_and_paint_inherit_and_override_independently() {
    let mut app = app();
    let window = app.spawn(
        app.root(),
        window().layout(LayoutStyle::default().size(px(200.0), px(100.0))),
    );
    *app.component_mut::<LayoutAnimationSettings>(window)
        .unwrap() =
        LayoutAnimationSettings::custom(AnimationTime::Duration(Duration::from_secs(1)), |t| t);
    *app.component_mut::<PaintAnimationSettings>(window).unwrap() =
        PaintAnimationSettings::custom(AnimationTime::Duration(Duration::from_secs(1)), |t| t);
    let style = LayoutStyle::default()
        .absolute()
        .inset(Insets::new(px(0.0), auto(), auto(), px(0.0)))
        .size(px(20.0), px(20.0));
    let initial = Paint::Quad(Quad::new(Color::BLACK));
    let snap_layout = app.spawn_with(
        window,
        Painted(initial.clone()),
        (
            style.clone(),
            LayoutAnimationSettings::custom(AnimationTime::Duration(Duration::ZERO), |t| t),
        ),
    );
    let snap_paint = app.spawn_with(
        window,
        Painted(initial.clone()),
        (
            style,
            PaintAnimationSettings::custom(AnimationTime::Duration(Duration::ZERO), |t| t),
        ),
    );
    app.tick();

    let target = Paint::Quad(Quad::new(Color::WHITE));
    for node in [snap_layout.id(), snap_paint.id()] {
        app.component_mut::<LayoutStyle>(node).unwrap().inset.left = px(50.0);
        *app.component_mut::<Paint>(node).unwrap() = target.clone();
    }
    app.tick();

    assert_eq!(
        app.component::<ComputedLayout>(snap_layout)
            .unwrap()
            .rect
            .x(),
        50.0
    );
    assert_eq!(app.component::<Layout>(snap_layout).unwrap().rect.x(), 50.0);
    assert_eq!(
        app.component::<AnimatedPaint>(snap_layout).unwrap().0,
        initial
    );
    assert!(
        app.component::<PaintTransition>(snap_layout)
            .unwrap()
            .is_running()
    );

    assert_eq!(
        app.component::<ComputedLayout>(snap_paint)
            .unwrap()
            .rect
            .x(),
        50.0
    );
    assert_eq!(app.component::<Layout>(snap_paint).unwrap().rect.x(), 0.0);
    assert_eq!(
        app.component::<AnimatedPaint>(snap_paint).unwrap().0,
        target
    );
    assert!(
        !app.component::<PaintTransition>(snap_paint)
            .unwrap()
            .is_running()
    );
}

#[test]
fn frame_interpolates_every_numeric_quad_field_with_easing() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let duration = Duration::from_secs(60);
    let panel = app.spawn_with(
        window,
        Painted(Paint::Quad(Quad::default())),
        (PaintAnimationSettings::new(
            AnimationTime::Duration(duration),
            Easing::EaseInQuad,
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
fn a_bezier_curve_eases_displayed_paint_on_frame() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let duration = Duration::from_secs(60);
    let easing = Easing::cubic_bezier(0.25, 0.1, 0.25, 1.0);
    let panel = app.spawn_with(
        window,
        Painted(Paint::Quad(Quad::default())),
        (PaintAnimationSettings::new(
            AnimationTime::Duration(duration),
            easing,
        ),),
    );
    app.tick();
    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(Quad::new(Color::WHITE));
    app.tick();
    let started = app.resource::<Time>().now();

    app.signal(Frame(window.id()));
    app.flush();
    let progress = (app.resource::<Time>().now() - started).as_secs_f32() / duration.as_secs_f32();
    let Paint::Quad(shown) = app.component::<AnimatedPaint>(panel).unwrap().0 else {
        panic!("a quad transition displays a quad");
    };
    assert!(progress < 1.0);
    assert_eq!(shown.color.r, easing.resolve(progress));
    assert!(
        app.component::<PaintTransition>(panel)
            .unwrap()
            .is_running()
    );
}

#[test]
fn frames_only_advance_their_window_and_completion_writes_the_exact_target() {
    let mut app = app();
    let a = app.spawn(app.root(), window());
    let b = app.spawn(app.root(), window());
    let initial = Paint::Quad(Quad::new(Color::BLACK));
    let settings =
        PaintAnimationSettings::custom(AnimationTime::Duration(Duration::from_nanos(1)), |t| {
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
        (PaintAnimationSettings::custom(
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

#[test]
fn speed_measures_every_numeric_quad_field() {
    for field in 0..16 {
        let mut app = app();
        let window = app.spawn(app.root(), window());
        let panel = app.spawn_with(
            window,
            Painted(Paint::Quad(Quad::default())),
            (PaintAnimationSettings::custom(
                AnimationTime::Speed(1.0),
                |t| t,
            ),),
        );
        app.tick();

        let mut target = Quad::default();
        let fields = [
            &mut target.color.r,
            &mut target.color.g,
            &mut target.color.b,
            &mut target.color.a,
            &mut target.border_color.r,
            &mut target.border_color.g,
            &mut target.border_color.b,
            &mut target.border_color.a,
            &mut target.radii.top_left,
            &mut target.radii.top_right,
            &mut target.radii.bottom_right,
            &mut target.radii.bottom_left,
            &mut target.border.top,
            &mut target.border.right,
            &mut target.border.bottom,
            &mut target.border.left,
        ];
        let distance = if field < 8 { 1.0 } else { 4.0 };
        *fields.into_iter().nth(field).unwrap() = distance;
        *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(target);
        app.tick();
        assert!(
            app.component::<PaintTransition>(panel)
                .unwrap()
                .is_running()
        );
        let started = app.resource::<Time>().now();
        app.signal(Frame(window.id()));
        app.flush();

        let progress = (app.resource::<Time>().now() - started).as_secs_f32() / distance;
        assert!(progress < 1.0, "field {field}");
        let Paint::Quad(shown) = app.component::<AnimatedPaint>(panel).unwrap().0 else {
            panic!("a quad transition displays a quad");
        };
        let values = [
            shown.color.r,
            shown.color.g,
            shown.color.b,
            shown.color.a,
            shown.border_color.r,
            shown.border_color.g,
            shown.border_color.b,
            shown.border_color.a,
            shown.radii.top_left,
            shown.radii.top_right,
            shown.radii.bottom_right,
            shown.radii.bottom_left,
            shown.border.top,
            shown.border.right,
            shown.border.bottom,
            shown.border.left,
        ];
        assert_eq!(values[field], distance * progress, "field {field}");
        assert!(
            app.component::<PaintTransition>(panel)
                .unwrap()
                .is_running()
        );
    }
}

#[test]
fn speed_uses_the_largest_change_for_a_shared_eased_duration() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let panel = app.spawn_with(
        window,
        Painted(Paint::Quad(Quad::default())),
        (PaintAnimationSettings::custom(
            AnimationTime::Speed(2.0),
            |t| t * t,
        ),),
    );
    app.tick();
    let target = Quad::new(Color::rgba(1.0, 0.5, 0.25, 1.0))
        .border(2.0, Color::rgb(0.25, 1.0, 0.5))
        .radii(Corners::new(1.0, 6.0, 2.0, 1.0));
    *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(target);
    app.tick();
    let started = app.resource::<Time>().now();
    app.signal(Frame(window.id()));
    app.flush();

    let progress = (app.resource::<Time>().now() - started).as_secs_f32() / 3.0;
    assert!(progress < 1.0);
    let t = progress * progress;
    let Paint::Quad(shown) = app.component::<AnimatedPaint>(panel).unwrap().0 else {
        panic!("a quad transition displays a quad");
    };
    assert_eq!(shown.radii.top_right, 6.0 * t);
    assert_eq!(shown.border.top, 2.0 * t);
    assert_eq!(shown.color.r, t);
    assert_eq!(shown.border_color.g, t);
}

#[test]
fn speed_retargeting_measures_from_the_displayed_quad() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let panel = app.spawn_with(
        window,
        Painted(Paint::Quad(Quad::new(Color::BLACK))),
        (PaintAnimationSettings::custom(
            AnimationTime::Speed(0.5),
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
    let distance = (f64::from(target.color.r) - f64::from(shown.color.r))
        .abs()
        .max((f64::from(target.color.g) - f64::from(shown.color.g)).abs())
        .max((f64::from(target.color.b) - f64::from(shown.color.b)).abs());
    let duration = Duration::try_from_secs_f64(distance / 0.5).unwrap();
    let progress = (app.resource::<Time>().now() - started).as_secs_f32() / duration.as_secs_f32();
    assert!(progress < 1.0);
    let lerp = |a: f32, b: f32| a + (b - a) * progress;
    let Paint::Quad(current) = app.component::<AnimatedPaint>(panel).unwrap().0 else {
        panic!("a quad transition displays a quad");
    };
    assert_eq!(current.color.r, lerp(shown.color.r, target.color.r));
    assert_eq!(current.color.g, lerp(shown.color.g, target.color.g));
    assert_eq!(current.color.b, lerp(shown.color.b, target.color.b));
}

#[test]
fn extreme_speeds_saturate_or_complete_on_the_first_frame() {
    for (speed, completes) in [(f32::MIN_POSITIVE, false), (f32::MAX, true)] {
        let mut app = app();
        let window = app.spawn(app.root(), window());
        let panel = app.spawn_with(
            window,
            Painted(Paint::Quad(Quad::new(Color::BLACK))),
            (PaintAnimationSettings::custom(
                AnimationTime::Speed(speed),
                |t| t,
            ),),
        );
        app.tick();
        *app.component_mut::<Paint>(panel).unwrap() = Paint::Quad(Quad::new(Color::WHITE));
        app.tick();
        app.signal(Frame(window.id()));
        app.flush();
        assert_eq!(
            app.component::<PaintTransition>(panel)
                .unwrap()
                .is_running(),
            !completes
        );
        if completes {
            assert_eq!(
                app.component::<AnimatedPaint>(panel).unwrap().0,
                Paint::Quad(Quad::new(Color::WHITE))
            );
        }
    }
}
