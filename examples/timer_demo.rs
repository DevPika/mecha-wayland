//! A seconds counter driven by an interval, and a button whose timeout
//! fires two seconds after the *last* tap. Run under a Wayland session:
//! `cargo run --example timer_demo`. Every fire is also logged to stderr
//! with the time since start, so the cadence can be read off a device.

use std::time::{Duration, Instant};

use mecha_wayland::prelude::*;

/// A clickable box with a text label. It does nothing on its own:
/// [`Demo`] wires the `Clicked` event on the handle `button` returns.
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
        *s.component_mut::<LayoutStyle>(me).unwrap() =
            LayoutStyle::default().center().padding_all(px(12.0));
        *s.component_mut::<Paint>(me).unwrap() =
            Paint::Quad(Quad::new(Color::rgb(0.25, 0.5, 0.9)).radius(6.0));
        s.spawn(me, text(b.font, b.label).size(18));
        Button
    }
}

/// Seconds since start, and a status line for the debounced timeout.
struct Demo {
    seconds: u32,
}

struct DemoBuilder {
    font: FontId,
    start: Instant,
}

impl Build for DemoBuilder {
    type Widget = Demo;
}

impl Widget for Demo {
    type Builder = DemoBuilder;
    fn build(b: DemoBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() =
            LayoutStyle::default().column().center().gap(px(12.0));

        let clock = s.spawn(me, text(b.font, "0 s").size(24));
        let restart = s.spawn(me, button(b.font, "Restart 2 s"));
        let status = s.spawn(me, text(b.font, "idle").size(18));
        let start = b.start;

        s.set_interval(Duration::from_secs(1), move |ctx, _| {
            ctx.me().seconds += 1;
            let n = ctx.me().seconds;
            ctx.at(clock).unwrap().set_text(format!("{n} s"));
            eprintln!("[timer_demo] tick {n} at {}ms", start.elapsed().as_millis());
        });

        let fire = s.add_timeout(Duration::from_secs(2), move |ctx, _| {
            ctx.at(status).unwrap().set_text("fired");
            eprintln!(
                "[timer_demo] timeout fired at {}ms",
                start.elapsed().as_millis()
            );
        });

        s.on::<Clicked>(restart, move |ctx, _| {
            ctx.start_timer(fire);
            ctx.at(status).unwrap().set_text("waiting…");
            eprintln!("[timer_demo] restart at {}ms", start.elapsed().as_millis());
        });

        Demo { seconds: 0 }
    }
}

/// Spawns the window and the demo; stops the app on close.
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
                .title("timer_demo")
                .clear(Color::rgb(0.12, 0.12, 0.14))
                .layout(LayoutStyle::default().center().size(px(300.0), px(200.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));
        s.spawn(
            win,
            DemoBuilder {
                font: b.font,
                start: Instant::now(),
            },
        );
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
        .add_module(RenderModule::default())
        .insert_resource(Atlas::new());
    app.add_module(RingModule::default())
        .add_module(TimerModule)
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.timer_demo".into(),
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
