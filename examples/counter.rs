//! A `+`/`-` pair that bump a label each click. Run under a Wayland
//! session: `cargo run --example counter`.

use mecha_wayland::prelude::*;

/// A clickable box with a text label. It does nothing on its own —
/// [`Counter`] wires the `Clicked` event on the handle `button` returns.
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

/// A label between a `-` and a `+`; each click moves the count by one.
struct Counter {
    count: i32,
}

impl Counter {
    fn step(&mut self, delta: i32) -> i32 {
        self.count += delta;
        self.count
    }
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
        *s.component_mut::<LayoutStyle>(me).unwrap() =
            LayoutStyle::default().column().center().gap(px(12.0));

        let label = s.spawn(me, text(b.font, "0").size(24));
        let input1 = s.spawn(me, input(b.font));
        let input2 = s.spawn(me, input(b.font));
        let row = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        let minus = s.spawn(row, button(b.font, "-"));
        let plus = s.spawn(row, button(b.font, "+"));

        s.on::<Clicked>(minus, move |ctx, _| {
            let count = ctx.me().step(-1);
            ctx.at(label).unwrap().set_text(count.to_string());
            ctx.at(input1).unwrap().set_text("minus");
            ctx.at(input2).unwrap().set_text("2minus");
        });

        s.on::<Clicked>(plus, move |ctx, _| {
            let count = ctx.me().step(1);
            ctx.at(label).unwrap().set_text(count.to_string());
            ctx.at(input1).unwrap().set_text("plus");
            ctx.at(input2).unwrap().set_text("2plus");
        });

        Counter { count: 0 }
    }
}

/// Spawns the window and the counter; stops the app on close.
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
                .title("counter")
                .clear(Color::rgb(0.12, 0.12, 0.14))
                .layout(LayoutStyle::default().center().size(px(240.0), px(160.0))),
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
        .add_module(RenderModule::default())
        .insert_resource(Atlas::new());
    app.add_module(RingModule::default())
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.counter".into(),
            budget: Budget::default(),
        })
        .add_module(TextInputModule);

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
