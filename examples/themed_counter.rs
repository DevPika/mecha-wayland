//! A counter that demonstrates theme-aware colors and runtime theme switching.
//! Run under a Wayland session: `cargo run --example themed_counter`.

use atlas::SpriteId;
use mecha_wayland::mechanix_widgets::prelude::{
    ButtonVariant, FontBook, TextContextExt, button, text,
};
use mecha_wayland::prelude::*;
use theme::{ColorRole, TextVariant};

const RESET_ICON_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/></svg>"#;

struct Counter {
    count: i32,
}

fn counter(reset_icon: SpriteId) -> CounterBuilder {
    CounterBuilder { reset_icon }
}

struct CounterBuilder {
    reset_icon: SpriteId,
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

        let label = s.spawn(
            me,
            text("0")
                .variant(TextVariant::TitleLarge)
                .color(ColorRole::OnSurface),
        );

        let row = s.spawn(
            me,
            div().style(LayoutStyle::default().row().center().gap(px(8.0))),
        );
        let minus = s.spawn(row, button().label("-"));

        let plus = s.spawn(row, button().label("+"));

        let reset = s.spawn(row, button().label("Reset").icon(b.reset_icon));

        let toggle = s.spawn(
            me,
            button()
                .variant(ButtonVariant::Outlined)
                .label("Toggle Theme"),
        );

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

        s.on::<Clicked>(reset, move |ctx, _| {
            ctx.me().count = 0;
            ctx.at(label).unwrap().set_text("0");
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
            ctx.set_paint(Paint::Quad(Quad::new(bg)));
        });

        Counter { count: 0 }
    }
}

struct Shell;
struct ShellBuilder {
    root: NodeId,
    reset_icon: SpriteId,
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
                .title("themed counter")
                .layout(LayoutStyle::default().center().size(px(300.0), px(200.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));
        s.spawn(win, counter(b.reset_icon));
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

    app.add_module(MechanixTheme::dark());

    let font = app
        .resource_mut::<Atlas>()
        .add_font(include_bytes!(
            "../crates/atlas/tests/fixtures/Inter-Regular.ttf"
        ))
        .expect("Inter loads");
    app.insert_resource(FontBook::new(font));

    let reset_icon = app
        .resource_mut::<Atlas>()
        .insert_svg(RESET_ICON_SVG, 24)
        .expect("insert reset icon");

    app.add_module(RingModule::default())
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.themed_counter".into(),
            budget: Budget::default(),
        });

    let root = app.root();
    app.spawn(root, ShellBuilder { root, reset_icon });
    app.run();
}
