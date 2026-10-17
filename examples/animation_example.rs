//! A themed counter with animated quad colors and button clicks.
//! Run under a Wayland session: `cargo run --example animation_example`.
//!
//! Text colors still snap: only quad-to-quad paint changes are animated.

use std::time::{Duration, Instant};

use mecha_wayland::prelude::*;

const RESTING_PADDING: f32 = 12.0;
const PRESSED_PADDING: f32 = 16.0;
const PULSE_HOLD: Duration = Duration::from_millis(100);

/// The click handler starts the pulse; the next frame after its hold time
/// restores the button's target padding. Layout animation handles both legs.
#[derive(Default)]
struct Pulses(Vec<(NodeId, Instant)>);
impl Resource for Pulses {}

fn restore_pulses(app: &mut App, frame: &Frame) {
    let now = app.resource::<Time>().now();
    let due: Vec<NodeId> = app
        .resource::<Pulses>()
        .0
        .iter()
        .filter(|&&(id, until)| {
            now >= until
                && app
                    .component::<InWindow>(id)
                    .is_some_and(|membership| membership.0 == Some(frame.0))
        })
        .map(|&(id, _)| id)
        .collect();
    if due.is_empty() {
        return;
    }
    app.resource_mut::<Pulses>()
        .0
        .retain(|(id, _)| !due.contains(id));
    for id in due {
        if let Some(mut style) = app.component_mut::<LayoutStyle>(id) {
            style.padding = Insets::all(px(RESTING_PADDING));
        }
    }
}

struct Button;

fn button(font: FontId, label: impl Into<String>) -> ButtonBuilder {
    ButtonBuilder {
        font,
        label: label.into(),
    }
}

struct ButtonBuilder {
    font: FontId,
    label: String,
}

impl Build for ButtonBuilder {
    type Widget = Button;
}

impl Widget for Button {
    type Builder = ButtonBuilder;

    fn build(b: ButtonBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default()
            .center()
            .padding_all(px(RESTING_PADDING));
        *s.component_mut::<Paint>(me).unwrap() =
            Paint::Quad(Quad::new(s.color(ColorRole::PrimaryContainer)).radius(6.0));
        *s.component_mut::<LayoutAnimationSettings>(me).unwrap() = LayoutAnimationSettings::new(
            AnimationTime::Duration(Duration::from_millis(240)),
            Easing::EaseOutCubic,
        );
        *s.component_mut::<PaintAnimationSettings>(me).unwrap() = PaintAnimationSettings::new(
            AnimationTime::Duration(Duration::from_millis(300)),
            Easing::EaseInOutQuad,
        );
        let label = s.spawn(
            me,
            text(b.font, b.label)
                .color(s.color(ColorRole::OnPrimaryContainer))
                .size(18),
        );

        s.on::<Clicked>(me, |ctx, _| {
            let id = ctx.handle().id();
            ctx.set_padding(Insets::all(px(PRESSED_PADDING)));
            let until = Instant::now() + PULSE_HOLD;
            let mut pulses = ctx.resource_mut::<Pulses>();
            if let Some((_, deadline)) = pulses.0.iter_mut().find(|(node, _)| *node == id) {
                *deadline = until;
            } else {
                pulses.0.push((id, until));
            }
        });

        s.on_theme(me, move |ctx| {
            let bg = ctx.color(ColorRole::PrimaryContainer);
            let fg = ctx.color(ColorRole::OnPrimaryContainer);
            ctx.set_paint(Paint::Quad(Quad::new(bg).radius(6.0)));
            ctx.at(label).unwrap().set_color(fg);
        });

        Button
    }
}

struct Counter {
    count: i32,
}

fn counter(font: FontId) -> CounterBuilder {
    CounterBuilder { font }
}

struct CounterBuilder {
    font: FontId,
}

impl Build for CounterBuilder {
    type Widget = Counter;
}

impl Widget for Counter {
    type Builder = CounterBuilder;

    fn build(b: CounterBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default()
            .column()
            .center()
            .fill()
            .gap(px(12.0));
        *s.component_mut::<Paint>(me).unwrap() =
            Paint::Quad(Quad::new(s.color(ColorRole::Surface)));
        *s.component_mut::<PaintAnimationSettings>(me).unwrap() = PaintAnimationSettings::new(
            AnimationTime::Duration(Duration::from_millis(300)),
            Easing::EaseInOutQuad,
        );

        let label = s.spawn(
            me,
            text(b.font, "0")
                .color(s.color(ColorRole::OnSurface))
                .size(24),
        );
        let row = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        let minus = s.spawn(row, button(b.font, "-"));
        let plus = s.spawn(row, button(b.font, "+"));
        let toggle = s.spawn(me, button(b.font, "Toggle Theme"));

        s.on::<Clicked>(minus, move |ctx, _| {
            ctx.me().count -= 1;
            let val = ctx.me().count;
            ctx.at(label).unwrap().set_text(val.to_string());
        });

        s.on::<Clicked>(plus, move |ctx, _| {
            ctx.me().count += 1;
            let val = ctx.me().count;
            ctx.at(label).unwrap().set_text(val.to_string());
        });

        s.on::<Clicked>(toggle, move |ctx, _| {
            let next = match ctx.theme().mode() {
                ThemeMode::Dark => MechanixTheme::light(),
                ThemeMode::Light => MechanixTheme::dark(),
            };
            ctx.set_theme(next);
        });

        s.on_theme(me, move |ctx| {
            let bg = ctx.color(ColorRole::Surface);
            let fg = ctx.color(ColorRole::OnSurface);
            ctx.set_paint(Paint::Quad(Quad::new(bg)));
            ctx.at(label).unwrap().set_color(fg);
        });

        Counter { count: 0 }
    }
}

struct Shell;
struct ShellBuilder {
    root: NodeId,
    font: FontId,
}
impl Build for ShellBuilder {
    type Widget = Shell;
}
impl Widget for Shell {
    type Builder = ShellBuilder;
    fn build(b: ShellBuilder, _me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let win = s.spawn(
            b.root,
            window()
                .title("animated themed counter")
                .layout(LayoutStyle::default().center().size(px(240.0), px(180.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));
        s.spawn(win, counter(b.font));
        Shell
    }
}

fn main() {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(InteractivityModule)
        .add_module(AnimationModule)
        .init_resource::<Pulses>()
        .system(restore_pulses)
        .add_module(RenderModule::default())
        .insert_resource(Atlas::new());

    app.add_module(MechanixTheme::dark());

    app.add_module(RingModule::default())
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.animated_counter".into(),
            budget: Budget::default(),
        });

    let font = app
        .resource_mut::<Atlas>()
        .add_font(include_bytes!(
            "../crates/atlas/tests/fixtures/Inter-Regular.ttf"
        ))
        .expect("Inter loads");

    let root = app.root();
    app.spawn(root, ShellBuilder { root, font });
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_click_pulses_and_theme_colors_animate() {
        let mut app = App::new();
        app.add_module(LayoutModule)
            .add_module(PaintModule)
            .add_module(WindowModule)
            .add_module(AnimationModule)
            .init_resource::<Pulses>()
            .system(restore_pulses)
            .insert_resource(Atlas::new());
        app.add_module(MechanixTheme::dark());
        let font = app
            .resource_mut::<Atlas>()
            .add_font(include_bytes!(
                "../crates/atlas/tests/fixtures/Inter-Regular.ttf"
            ))
            .unwrap();
        let window = app.spawn(
            app.root(),
            window().layout(LayoutStyle::default().size(px(240.0), px(180.0))),
        );
        let button = app.spawn(window, button(font, "+"));
        app.tick();

        app.emit(
            Clicked {
                contact: ContactId::Mouse,
                position: Point::ZERO,
            },
            button,
        );
        app.flush();
        assert_eq!(
            app.component::<LayoutStyle>(button).unwrap().padding,
            Insets::all(px(PRESSED_PADDING))
        );
        app.tick();
        assert!(
            app.resource::<Pulses>()
                .0
                .iter()
                .any(|(id, _)| *id == button.id())
        );

        // Backdate the hold, not the animation: the next frame restores the
        // layout target, whose displayed value can still interpolate.
        app.resource_mut::<Pulses>().0[0].1 = Instant::now() - Duration::from_secs(1);
        app.signal(Frame(window.id()));
        app.flush();
        assert!(app.resource::<Pulses>().0.is_empty());
        assert_eq!(
            app.component::<LayoutStyle>(button).unwrap().padding,
            Insets::all(px(RESTING_PADDING))
        );

        let original = app.component::<Paint>(button).unwrap().clone();
        app.set_theme(MechanixTheme::light());
        app.flush();
        app.tick();
        assert_ne!(*app.component::<Paint>(button).unwrap(), original);
        assert_eq!(app.component::<AnimatedPaint>(button).unwrap().0, original);
        assert!(
            app.component::<PaintTransition>(button)
                .unwrap()
                .is_running()
        );
    }
}
