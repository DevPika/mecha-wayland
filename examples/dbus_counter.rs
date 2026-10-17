//! The counter, also on the session bus: `mecha.Counter` at
//! `/mecha/Counter`, interface `mecha.Counter`, with `Increment`,
//! `Decrement` and a read-only `Count`. Run under a Wayland session:
//! `cargo run --example dbus_counter`, then in a terminal
//!
//! ```text
//! busctl --user call mecha.Counter /mecha/Counter mecha.Counter Increment
//! busctl --user get-property mecha.Counter /mecha/Counter mecha.Counter Count
//! busctl --user monitor mecha.Counter
//! ```
//!
//! A call moves the window's count; a click shows up in `monitor` as
//! `PropertiesChanged`.

use mecha_wayland::mechanix_widgets::Text;
use mecha_wayland::mechanix_widgets::prelude::{FontBook, TextContextExt, button, text};
use mecha_wayland::prelude::*;
use theme::{ColorRole, TextVariant};

const PATH: &str = "/mecha/Counter";
const IFACE: &str = "mecha.Counter";

/// Where the counter lives on the bus.
struct Remote {
    export: ExportId,
}
impl Resource for Remote {}

/// Move the count: sent by the D-Bus system to the counter node.
struct Step(i32);
impl Event for Step {}

struct Counter {
    count: i32,
}

struct CounterBuilder;
impl Build for CounterBuilder {
    type Widget = Counter;
}

/// Apply a step: the label, then the exported `Count`.
fn apply(ctx: &mut Context<'_, Counter>, label: Handle<Text>, delta: i32) {
    ctx.me().count += delta;
    let count = ctx.me().count;
    ctx.at(label).unwrap().set_text(count.to_string());
    let export = ctx.resource::<Remote>().export;
    let _ =
        ctx.resource_mut::<DBus>()
            .session()
            .set_property_value(export, "Count", Value::I32(count));
}

impl Widget for Counter {
    type Builder = CounterBuilder;
    fn build(_: CounterBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
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
        s.on::<Clicked>(minus, move |ctx, _| apply(ctx, label, -1));
        s.on::<Clicked>(plus, move |ctx, _| apply(ctx, label, 1));
        s.on::<Step>(me, move |ctx, e| apply(ctx, label, e.0));
        Counter { count: 0 }
    }
}

/// `Increment` and `Decrement` from the bus: step every counter, answer.
fn on_call(app: &mut App, e: &MethodCallReceived) {
    if e.export != app.resource::<Remote>().export {
        return;
    }
    let delta = match e.call.member().as_str() {
        "Increment" => 1,
        "Decrement" => -1,
        _ => return,
    };
    let counters: Vec<NodeId> = app.widgets::<Counter>().map(|(id, _)| id).collect();
    app.emit(Step(delta), counters);
    app.resource_mut::<DBus>().reply(&e.call, &());
}

struct Shell;
struct ShellBuilder {
    root: NodeId,
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
                .title("dbus counter")
                .layout(LayoutStyle::default().center().size(px(300.0), px(200.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));
        s.spawn(win, CounterBuilder);
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

    app.add_module(RingModule::default())
        .add_module(DBusModule::new().session())
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.dbus_counter".into(),
            budget: Budget::default(),
        });

    let spec = InterfaceSpec::new(IFACE)
        .method("Increment", "", "")
        .method("Decrement", "", "")
        .property("Count", "i", Access::Read, Value::I32(0));
    let export = {
        let mut dbus = app.resource_mut::<DBus>();
        let mut bus = dbus.session();
        bus.request_name("mecha.Counter", NameFlags::DO_NOT_QUEUE);
        bus.export(PATH, spec).expect("export /mecha/Counter")
    };
    app.insert_resource(Remote { export });
    app.system(on_call);

    let root = app.root();
    app.spawn(root, ShellBuilder { root });
    app.run();
}
