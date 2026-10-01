use std::time::Duration;

use animation::{
    AnimatedPaint, AnimationModule, AnimationSettings, AnimationTime, PaintTransition,
};
use app::prelude::*;
use geometry::Color;
use layout::LayoutModule;
use paint::{Paint, PaintModule, Quad};
use window::{FrameRequested, WindowModule, window};

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
