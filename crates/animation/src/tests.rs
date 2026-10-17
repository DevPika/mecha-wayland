use super::*;
use crate::{AnimationModule, AnimationTime, Easing, LayoutAnimationSettings};
use ::paint::prelude::*; // crate root. Why is this even here
use geometry::{Color, Size};
use layout::prelude::*;
use std::time::Duration;
use window::{FrameRequested, WindowModule, window};

struct MoveTo(f32);
impl Event for MoveTo {}

struct Leaf;
impl Build for Leaf {
    type Widget = Self;
}
impl Widget for Leaf {
    type Builder = Self;
    fn build(b: Self, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        s.on::<MoveTo>(me, |ctx, event| {
            ctx.component_mut::<LayoutStyle>().unwrap().inset.left = px(event.0);
        });
        b
    }
}

#[derive(Default)]
struct Requests(Vec<NodeId>);
impl Resource for Requests {}

fn app() -> App {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(AnimationModule)
        .init_resource::<Requests>()
        .system(|app, f: &FrameRequested| app.resource_mut::<Requests>().0.push(f.0));
    app
}

fn animated(ms: u64, easing: fn(f32) -> f32) -> LayoutAnimationSettings {
    LayoutAnimationSettings::custom(AnimationTime::Duration(Duration::from_millis(ms)), easing)
}

fn scene(app: &mut App) -> (NodeId, Handle<Leaf>) {
    let window = app.spawn(
        app.root(),
        window().layout(LayoutStyle::default().size(px(400.0), px(400.0))),
    );
    let panel = app.spawn_with(
        window,
        Leaf,
        (
            animated(1000, |t| t * t),
            LayoutStyle::default()
                .column()
                .absolute()
                .inset(Insets::new(px(0.0), auto(), auto(), px(0.0)))
                .size(px(100.0), px(100.0)),
            Paint::Quad(Quad::new(Color::WHITE)),
        ),
    );
    app.tick();
    app.signal(Frame(window.id()));
    app.flush();
    app.resource_mut::<Requests>().0.clear();
    (window.id(), panel)
}

fn start(app: &App, node: impl Into<NodeId>) -> Instant {
    app.component::<Transition>(node)
        .unwrap()
        .running
        .as_ref()
        .unwrap()
        .started
}

#[test]
fn animation_registers_displayed_layout() {
    let mut app = App::new();
    app.add_module(LayoutModule).add_module(WindowModule);
    app.add_module(AnimationModule);
    assert_eq!(
        app.component::<Layout>(app.root()),
        Some(&Layout::default())
    );
}

#[test]
fn first_resolution_snaps_even_when_the_target_does_not_change_from_zero() {
    let mut app = app();
    let window = app.spawn(app.root(), window());
    let panel = app.spawn_with(
        window,
        Leaf,
        (
            animated(1000, |t| t),
            LayoutStyle::default().size(px(0.0), px(0.0)),
        ),
    );
    app.tick();
    assert_eq!(
        *app.component::<ComputedLayout>(panel).unwrap(),
        ComputedLayout::default()
    );
    assert!(app.component::<Transition>(panel).unwrap().resolved);
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );

    app.component_mut::<LayoutStyle>(panel).unwrap().width = px(100.0);
    app.tick();
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.width(), 0.0);
    assert_eq!(
        app.component::<ComputedLayout>(panel).unwrap().rect.width(),
        100.0
    );
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_some()
    );
}

#[test]
fn marking_a_root_later_initializes_only_its_resolved_subtree() {
    let mut app = app();
    let stray = app.spawn(app.root(), Leaf);
    let root = app
        .spawn_with(
            app.root(),
            Leaf,
            (LayoutStyle::default().size(px(200.0), px(100.0)),),
        )
        .id();
    let panel = app.spawn_with(
        root,
        Leaf,
        (
            animated(1000, |t| t),
            LayoutStyle::default().size(px(40.0), px(30.0)),
        ),
    );
    app.tick();
    assert!(!app.component::<Transition>(root).unwrap().resolved);
    assert!(!app.component::<Transition>(panel).unwrap().resolved);
    assert!(!app.component::<Transition>(stray).unwrap().resolved);
    assert_eq!(*app.component::<Layout>(panel).unwrap(), Layout::default());

    app.component_mut::<LayoutRoot>(root).unwrap().0 = true;
    app.tick();
    for id in [root, panel.id()] {
        assert!(app.component::<Transition>(id).unwrap().resolved);
        assert_eq!(
            *app.component::<Layout>(id).unwrap(),
            Layout::from(*app.component::<ComputedLayout>(id).unwrap())
        );
        assert!(app.component::<Transition>(id).unwrap().running.is_none());
    }
    assert!(!app.component::<Transition>(stray).unwrap().resolved);

    app.component_mut::<LayoutStyle>(panel).unwrap().width = px(80.0);
    app.tick();
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );
    // The node has no window, so subsequent changes copy instead of animating.
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.width(), 80.0);
}

#[test]
fn first_resolved_layout_notifies_consumers_in_the_same_tick() {
    #[derive(Default)]
    struct Changes(Vec<Vec<NodeId>>);
    impl Resource for Changes {}

    let mut app = app();
    app.init_resource::<Changes>()
        .system(|app, event: &Emitted<OnChanged<Layout>>| {
            app.resource_mut::<Changes>().0.push(event.targets.to_vec());
        });
    let window = app.spawn(
        app.root(),
        window().layout(LayoutStyle::default().size(px(200.0), px(100.0))),
    );
    let panel = app.spawn_with(
        window,
        Leaf,
        (LayoutStyle::default().size(px(20.0), px(10.0)),),
    );
    app.tick();
    let changes = &app.resource::<Changes>().0;
    assert_eq!(changes, &[vec![window.id(), panel.id()]]);
    app.resource_mut::<Changes>().0.clear();
    app.tick();
    assert!(app.resource::<Changes>().0.is_empty());
}

#[test]
fn changing_a_window_from_column_to_row_retargets_its_children() {
    let mut app = app();
    let window = app.spawn(
        app.root(),
        window().layout(LayoutStyle::default().column().size(px(480.0), px(320.0))),
    );
    let first = app.spawn_with(
        window,
        Leaf,
        (LayoutStyle::default().size(px(440.0), px(200.0)),),
    );
    let second = app.spawn_with(
        window,
        Leaf,
        (
            animated(1500, |t| t * t * t),
            LayoutStyle::default().size(px(440.0), px(200.0)),
        ),
    );
    let third = app.spawn_with(
        window,
        Leaf,
        (
            animated(500, |t| t),
            LayoutStyle::default()
                .inset(Insets::new(px(410.0), auto(), auto(), px(0.0)))
                .size(px(200.0), px(180.0)),
        ),
    );
    app.tick();
    app.signal(Frame(window.id()));
    app.flush();
    app.resource_mut::<Requests>().0.clear();

    let before = *app.component::<Layout>(second).unwrap();
    *app.component_mut::<LayoutStyle>(window).unwrap() =
        LayoutStyle::default().row().size(px(480.0), px(320.0));
    app.component_mut::<LayoutStyle>(second).unwrap().width = px(220.0);
    app.tick();

    assert_eq!(
        app.component::<ComputedLayout>(window).unwrap().rect.size,
        Size::new(480.0, 320.0)
    );
    assert_eq!(
        app.component::<Layout>(window).unwrap().rect.size,
        Size::new(480.0, 320.0)
    );
    assert_eq!(*app.component::<Layout>(second).unwrap(), before);
    assert!(
        app.component::<Transition>(second)
            .unwrap()
            .running
            .is_some()
    );
    assert!(
        app.component::<Transition>(third)
            .unwrap()
            .running
            .is_some()
    );
    assert_eq!(app.resource::<Requests>().0, [window.id()]);
    assert_eq!(
        app.component::<Layout>(first).unwrap().rect,
        app.component::<ComputedLayout>(first).unwrap().rect,
        "a nonanimated child should copy its target"
    );
}

#[test]
fn a_new_node_resolves_independently_of_a_reused_slot() {
    let mut app = app();
    let (window, old) = scene(&mut app);
    app.emit(MoveTo(100.0), old);
    app.tick();
    assert!(app.component::<Transition>(old).unwrap().running.is_some());
    app.remove(old);
    let replacement = app.spawn_with(
        window,
        Leaf,
        (
            animated(1000, |t| t),
            LayoutStyle::default().size(px(20.0), px(30.0)),
        ),
    );
    assert_eq!(old.id().slot(), replacement.id().slot());
    assert!(!app.component::<Transition>(replacement).unwrap().resolved);
    app.tick();
    assert!(app.component::<Transition>(replacement).unwrap().resolved);
    assert!(
        app.component::<Transition>(replacement)
            .unwrap()
            .running
            .is_none()
    );
    assert_eq!(
        app.component::<Layout>(replacement).unwrap().rect.width(),
        20.0
    );
}

#[test]
fn displayed_layout_has_the_same_fields_and_content_box_as_the_target() {
    let target = ComputedLayout {
        rect: Rect::new(10.0, 20.0, 100.0, 50.0),
        padding: Insets::new(1.0, 2.0, 3.0, 4.0),
        border: Insets::all(1.0),
    };
    let displayed = Layout::from(target);
    assert_eq!(displayed.rect, target.rect);
    assert_eq!(displayed.padding, target.padding);
    assert_eq!(displayed.border, target.border);
    assert_eq!(displayed.content(), target.content());
}

#[test]
fn first_resolution_snaps_then_an_event_starts_a_duration_transition() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    let initial = *app.component::<Layout>(panel).unwrap();
    assert_eq!(
        initial,
        Layout::from(*app.component::<ComputedLayout>(panel).unwrap())
    );
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );

    app.emit(MoveTo(100.0), panel);
    app.tick();
    assert_eq!(
        app.component::<ComputedLayout>(panel).unwrap().rect.x(),
        100.0
    );
    assert_eq!(*app.component::<Layout>(panel).unwrap(), initial);
    assert_eq!(app.resource::<Requests>().0, [window]);
    let started = start(&app, panel);

    // A second layout pass must not reset a running transition.
    app.tick();
    assert_eq!(start(&app, panel), started);
    advance(&mut app, window, started + Duration::from_millis(500));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 25.0);
    advance(&mut app, window, started + Duration::from_secs(1));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 100.0);
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );

    // Close pending demand after the final displayed value.
    app.signal(Frame(window));
    app.flush();
    app.resource_mut::<Requests>().0.clear();
    app.tick();
    assert!(app.resource::<Requests>().0.is_empty());
    app.signal(Frame(window));
    app.flush();
    app.resource_mut::<Requests>().0.clear();
    app.tick();
    assert!(
        app.resource::<Requests>().0.is_empty(),
        "completion stops requesting frames"
    );
}

#[test]
fn nonanimated_nodes_copy_and_request_a_frame_in_the_same_tick() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::default();
    app.emit(MoveTo(100.0), panel);
    app.tick();
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 100.0);
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );
    assert_eq!(app.resource::<Requests>().0, [window]);

    app.signal(Frame(window));
    app.flush();
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 100.0);
    app.resource_mut::<Requests>().0.clear();
    app.tick();
    assert!(app.resource::<Requests>().0.is_empty());
}

#[test]
fn nonanimated_layout_roots_outside_windows_are_synchronized_too() {
    let mut app = app();
    let root = app.root();
    app.component_mut::<LayoutRoot>(root).unwrap().0 = true;
    *app.component_mut::<LayoutStyle>(root).unwrap() =
        LayoutStyle::default().size(px(200.0), px(100.0));
    app.tick();
    app.component_mut::<LayoutStyle>(root).unwrap().width = px(300.0);
    app.tick();
    assert_eq!(app.component::<Layout>(root).unwrap().rect.width(), 300.0);
    assert!(app.resource::<Requests>().0.is_empty());
}

#[test]
fn children_inherit_and_explicit_settings_override() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    let inherited = app.spawn_with(
        panel,
        Leaf,
        (LayoutStyle::default().size(px(10.0), px(10.0)),),
    );
    let overridden = app.spawn_with(
        panel,
        Leaf,
        (
            LayoutStyle::default().size(px(10.0), px(10.0)),
            animated(1000, |t| t),
        ),
    );
    app.tick();
    assert!(
        app.component::<Transition>(inherited)
            .unwrap()
            .running
            .is_none()
    );
    app.emit(MoveTo(100.0), panel);
    app.tick();
    let started = start(&app, panel);
    assert_eq!(start(&app, inherited), started);
    advance(&mut app, window, started + Duration::from_millis(500));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 25.0);
    assert_eq!(app.component::<Layout>(inherited).unwrap().rect.x(), 25.0);
    assert_eq!(app.component::<Layout>(overridden).unwrap().rect.x(), 50.0);
    assert!(
        app.component::<LayoutAnimationSettings>(inherited)
            .unwrap()
            .0
            .is_none(),
        "inheritance does not overwrite explicit settings"
    );
}

#[test]
fn zero_duration_disables_inheritance_and_syncs_without_relayout() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    app.emit(MoveTo(100.0), panel);
    app.tick();
    let started = start(&app, panel);
    advance(&mut app, window, started + Duration::from_millis(500));
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() = animated(0, |t| t);
    app.tick();
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 100.0);
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );

    let child = app.spawn_with(
        panel,
        Leaf,
        (LayoutStyle::default().size(px(10.0), px(10.0)),),
    );
    app.tick();
    app.emit(MoveTo(200.0), panel);
    app.tick();
    assert_eq!(app.component::<Layout>(child).unwrap().rect.x(), 200.0);
    assert!(
        app.component::<Transition>(child)
            .unwrap()
            .running
            .is_none()
    );
}

#[test]
fn new_settings_and_style_in_the_same_tick_do_not_snap_the_target() {
    let mut app = app();
    let (_, panel) = scene(&mut app);
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::default();
    app.tick();
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() = animated(1000, |t| t);
    app.emit(MoveTo(100.0), panel);
    app.tick();
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 0.0);
    assert_eq!(
        app.component::<ComputedLayout>(panel).unwrap().rect.x(),
        100.0
    );
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_some()
    );
}

#[test]
fn frame_updates_only_its_window_and_removed_slots_do_not_inherit_transitions() {
    let mut app = app();
    let (a, first) = scene(&mut app);
    let (_, second) = scene(&mut app);
    app.emit(MoveTo(100.0), [first.id(), second.id()]);
    app.tick();
    let started = start(&app, first);
    advance(&mut app, a, started + Duration::from_millis(500));
    assert_eq!(app.component::<Layout>(first).unwrap().rect.x(), 25.0);
    assert_eq!(app.component::<Layout>(second).unwrap().rect.x(), 0.0);
    app.remove(first);
    let replacement = app.spawn(a, Leaf);
    assert_eq!(replacement.id().slot(), first.id().slot());
    assert!(
        app.component::<Transition>(replacement)
            .unwrap()
            .running
            .is_none()
    );
    app.tick();
}

#[test]
fn window_roots_snap_and_frame_animation_reaches_the_target() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    *app.component_mut::<LayoutAnimationSettings>(window)
        .unwrap() = animated(1000, |t| t);
    app.component_mut::<LayoutStyle>(window).unwrap().width = px(500.0);
    app.emit(MoveTo(100.0), panel);
    app.tick();
    assert_eq!(app.component::<Layout>(window).unwrap().rect.width(), 500.0);
    assert!(
        app.component::<Transition>(window)
            .unwrap()
            .running
            .is_none()
    );
    // Backdate instead of sleeping to exercise the real Frame systems.
    app.component_mut::<Transition>(panel)
        .unwrap()
        .running
        .as_mut()
        .unwrap()
        .started = Instant::now() - Duration::from_secs(2);
    app.signal(Frame(window));
    app.flush();
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 100.0);
}

#[test]
fn interpolation_includes_size_padding_and_border() {
    let from = Layout::default();
    let target = Layout {
        rect: Rect::new(20.0, 40.0, 80.0, 100.0),
        padding: Insets::new(2.0, 4.0, 6.0, 8.0),
        border: Insets::new(10.0, 12.0, 14.0, 16.0),
    };
    assert_eq!(
        from.interpolate(target, 0.5),
        Layout {
            rect: Rect::new(10.0, 20.0, 40.0, 50.0),
            padding: Insets::new(1.0, 2.0, 3.0, 4.0),
            border: Insets::new(5.0, 6.0, 7.0, 8.0),
        }
    );
}

#[test]
fn a_bezier_curve_eases_displayed_layout_on_frame() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    let easing = Easing::cubic_bezier(0.42, 0.0, 1.0, 1.0);
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::new(AnimationTime::Duration(Duration::from_secs(1)), easing);
    app.emit(MoveTo(100.0), panel);
    app.tick();
    let started = start(&app, panel);
    advance(&mut app, window, started + Duration::from_millis(500));

    let shown = app.component::<Layout>(panel).unwrap().rect.x();
    assert!((shown - 100.0 * easing.resolve(0.5)).abs() < 1e-5);
    assert!(shown < 50.0);
    advance(&mut app, window, started + Duration::from_secs(1));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 100.0);
}

#[test]
fn speed_derives_duration_from_distance_and_retains_easing() {
    for (destination, seconds) in [(50.0, 0.5), (200.0, 2.0), (-100.0, 1.0)] {
        for easing in [|t| t, |t| t * t, |t| t * 2.0] as [fn(f32) -> f32; 3] {
            let mut app = app();
            let (window, panel) = scene(&mut app);
            *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
                LayoutAnimationSettings::custom(AnimationTime::Speed(100.0), easing);
            app.emit(MoveTo(destination), panel);
            app.tick();
            let duration = Duration::from_secs_f64(seconds);
            let running = app
                .component::<Transition>(panel)
                .unwrap()
                .running
                .as_ref()
                .unwrap();
            assert_eq!(running.duration, duration);
            let started = running.started;
            assert_eq!(app.resource::<Requests>().0, [window]);

            advance(&mut app, window, started + duration / 2);
            assert_eq!(
                app.component::<Layout>(panel).unwrap().rect.x(),
                destination * easing(0.5)
            );
            advance(&mut app, window, started + duration);
            assert_eq!(
                app.component::<Layout>(panel).unwrap().rect.x(),
                destination
            );
            assert!(
                app.component::<Transition>(panel)
                    .unwrap()
                    .running
                    .is_none()
            );
        }
    }
}

#[test]
fn speed_distance_accounts_for_every_layout_field() {
    for field in 0..12 {
        let mut app = app();
        let (window, panel) = scene(&mut app);
        *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
            LayoutAnimationSettings::custom(AnimationTime::Speed(100.0), |t| t);
        let from = *app.component::<Layout>(panel).unwrap();
        let mut target = *app.component::<ComputedLayout>(panel).unwrap();
        let fields = [
            &mut target.rect.origin.x,
            &mut target.rect.origin.y,
            &mut target.rect.size.width,
            &mut target.rect.size.height,
            &mut target.padding.top,
            &mut target.padding.right,
            &mut target.padding.bottom,
            &mut target.padding.left,
            &mut target.border.top,
            &mut target.border.right,
            &mut target.border.bottom,
            &mut target.border.left,
        ];
        *fields.into_iter().nth(field).unwrap() += 50.0;
        *app.component_mut::<ComputedLayout>(panel).unwrap() = target;
        app.signal(LayoutDone { roots: vec![] });
        app.flush();

        let running = app
            .component::<Transition>(panel)
            .unwrap()
            .running
            .as_ref()
            .unwrap();
        assert_eq!(
            running.duration,
            Duration::from_millis(500),
            "field {field}"
        );
        let started = running.started;
        advance(&mut app, window, started + Duration::from_millis(250));
        assert_eq!(
            *app.component::<Layout>(panel).unwrap(),
            from.interpolate(Layout::from(target), 0.5),
            "field {field}"
        );
    }
}

#[test]
fn speed_uses_one_duration_for_all_fields_not_diagonal_distance() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::custom(AnimationTime::Speed(100.0), |t| t);
    let from = *app.component::<Layout>(panel).unwrap();
    let target = ComputedLayout {
        rect: Rect::new(100.0, 100.0, 200.0, 150.0),
        padding: Insets::all(20.0),
        border: Insets::all(10.0),
    };
    *app.component_mut::<ComputedLayout>(panel).unwrap() = target;
    app.signal(LayoutDone { roots: vec![] });
    app.flush();
    let running = app
        .component::<Transition>(panel)
        .unwrap()
        .running
        .as_ref()
        .unwrap();
    assert_eq!(running.duration, Duration::from_secs(1));
    let started = running.started;
    advance(&mut app, window, started + Duration::from_millis(500));
    assert_eq!(
        *app.component::<Layout>(panel).unwrap(),
        from.interpolate(Layout::from(target), 0.5)
    );
    advance(&mut app, window, started + Duration::from_secs(1));
    assert_eq!(
        *app.component::<Layout>(panel).unwrap(),
        Layout::from(target)
    );
}

#[test]
fn speed_retargeting_uses_displayed_layout_and_new_settings() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::custom(AnimationTime::Speed(100.0), |t| t);
    app.emit(MoveTo(200.0), panel);
    app.tick();
    let started = start(&app, panel);
    advance(&mut app, window, started + Duration::from_millis(500));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 50.0);

    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::custom(AnimationTime::Speed(200.0), |t| t * t);
    app.tick();
    let running = app
        .component::<Transition>(panel)
        .unwrap()
        .running
        .as_ref()
        .unwrap();
    assert_eq!(running.started, started);
    assert_eq!(running.duration, Duration::from_secs(2));
    assert_eq!(running.easing.resolve(0.5), 0.5);

    app.emit(MoveTo(-50.0), panel);
    app.tick();
    let running = app
        .component::<Transition>(panel)
        .unwrap()
        .running
        .as_ref()
        .unwrap();
    assert_eq!(running.from.rect.x(), 50.0);
    assert_eq!(running.duration, Duration::from_millis(500));
    let started = running.started;
    advance(&mut app, window, started + Duration::from_millis(250));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 25.0);
    advance(&mut app, window, started + Duration::from_millis(500));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), -50.0);
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_none()
    );
}

#[test]
fn speed_is_inherited_and_zero_duration_still_overrides_it() {
    let mut app = app();
    let (window, panel) = scene(&mut app);
    *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
        LayoutAnimationSettings::custom(AnimationTime::Speed(100.0), |t| t);
    let child = app.spawn_with(
        panel,
        Leaf,
        (LayoutStyle::default().size(px(10.0), px(10.0)),),
    );
    app.tick();
    assert!(
        app.component::<Transition>(child)
            .unwrap()
            .running
            .is_none()
    );
    app.emit(MoveTo(200.0), panel);
    app.tick();
    let started = start(&app, panel);
    assert_eq!(start(&app, child), started);
    advance(&mut app, window, started + Duration::from_millis(500));
    assert_eq!(app.component::<Layout>(panel).unwrap().rect.x(), 50.0);
    assert_eq!(app.component::<Layout>(child).unwrap().rect.x(), 50.0);

    *app.component_mut::<LayoutAnimationSettings>(child).unwrap() = animated(0, |t| t);
    app.tick();
    assert_eq!(app.component::<Layout>(child).unwrap().rect.x(), 200.0);
    assert!(
        app.component::<Transition>(child)
            .unwrap()
            .running
            .is_none()
    );
    assert!(
        app.component::<Transition>(panel)
            .unwrap()
            .running
            .is_some()
    );
}

#[test]
fn extreme_positive_speeds_do_not_overflow_or_divide_by_zero() {
    for (speed, duration) in [
        (f32::MIN_POSITIVE, Duration::MAX),
        (f32::MAX, Duration::ZERO),
    ] {
        let mut app = app();
        let (window, panel) = scene(&mut app);
        *app.component_mut::<LayoutAnimationSettings>(panel).unwrap() =
            LayoutAnimationSettings::custom(AnimationTime::Speed(speed), |t| t);
        app.emit(MoveTo(100.0), panel);
        app.tick();
        let running = app
            .component::<Transition>(panel)
            .unwrap()
            .running
            .as_ref()
            .unwrap();
        assert_eq!(running.duration, duration);
        let started = running.started;
        advance(&mut app, window, started);
        assert_eq!(
            app.component::<Layout>(panel).unwrap().rect.x(),
            if duration.is_zero() { 100.0 } else { 0.0 }
        );
        assert_eq!(
            app.component::<Transition>(panel)
                .unwrap()
                .running
                .is_none(),
            duration.is_zero()
        );
    }
}
