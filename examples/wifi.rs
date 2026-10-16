//! Wi-Fi status and an on/off toggle through NetworkManager on the
//! system bus. Run under a Wayland session with NetworkManager:
//! `cargo run --example wifi`. `nmcli radio wifi off` in a terminal
//! updates the window live.
//!
//! The shape to copy for a real service module: `WifiModule` owns the
//! D-Bus side and keeps the `Wifi` resource current; the UI only reads
//! `Wifi` (through `WifiChanged` at its node) and asks for changes with
//! the `ToggleWifi` signal.

use mecha_wayland::mechanix_widgets::prelude::{
    ButtonContextExt, FontBook, TextContextExt, button, text,
};
use mecha_wayland::prelude::*;
use theme::{ColorRole, TextVariant};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WifiState {
    Loading,
    Unavailable,
    On,
    Off,
    /// The radio is off at a hardware switch; software cannot turn it on.
    HardwareOff,
}

/// What the UI shows. Written only by `WifiModule`'s systems.
struct Wifi {
    state: WifiState,
    /// `WirelessEnabled`, last seen.
    enabled: bool,
    /// `WirelessHardwareEnabled`, last seen.
    hardware: bool,
    /// The outstanding `GetAll`.
    load: Option<CallId>,
    /// The outstanding `Set`.
    pending: Option<CallId>,
    /// Why the last toggle failed (a polkit refusal, say).
    error: Option<String>,
    props: MatchId,
    owner: MatchId,
}
impl Resource for Wifi {}

impl Wifi {
    fn settle(&mut self) {
        self.state = if !self.hardware {
            WifiState::HardwareOff
        } else if self.enabled {
            WifiState::On
        } else {
            WifiState::Off
        };
    }
}

/// Ask NetworkManager to flip Wi-Fi.
struct ToggleWifi;
impl Signal for ToggleWifi {}

/// The panel's cue to redraw from `Wifi`.
struct WifiChanged;
impl Event for WifiChanged {}

/// Watches NetworkManager and owns `Wifi`. Installs after `DBusModule`.
struct WifiModule;

impl Module for WifiModule {
    fn install(self, app: &mut App) {
        let (props, owner, load) = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();
            let props = bus.watch_properties(NM, NM_PATH, NM);
            let owner = bus.add_match(
                MatchRule::signal()
                    .sender("org.freedesktop.DBus")
                    .interface("org.freedesktop.DBus")
                    .member("NameOwnerChanged")
                    .arg(0, NM),
            );
            let load = bus.object(NM, NM_PATH).get_all(NM);
            (props, owner, load)
        };
        app.insert_resource(Wifi {
            state: WifiState::Loading,
            enabled: false,
            hardware: true,
            load: Some(load),
            pending: None,
            error: None,
            props,
            owner,
        });
        app.system(on_reply)
            .system(on_signal)
            .system(on_toggle)
            .system(on_wifi_changed);
    }
}

fn reload(app: &mut App) {
    let load = app
        .resource_mut::<DBus>()
        .system()
        .object(NM, NM_PATH)
        .get_all(NM);
    let mut w = app.resource_mut::<Wifi>();
    w.load = Some(load);
    w.state = WifiState::Loading;
}

fn on_reply(app: &mut App, e: &MethodReply) {
    let (load, pending) = {
        let w = app.resource::<Wifi>();
        (w.load, w.pending)
    };
    if Some(e.call) == load {
        let mut w = app.resource_mut::<Wifi>();
        w.load = None;
        match e.result.as_ref().map(|m| m.body::<(PropMap,)>()) {
            Ok(Ok((props,))) => {
                w.enabled = props
                    .get_as::<bool>("WirelessEnabled")
                    .and_then(Result::ok)
                    .unwrap_or(false);
                w.hardware = props
                    .get_as::<bool>("WirelessHardwareEnabled")
                    .and_then(Result::ok)
                    .unwrap_or(true);
                w.settle();
            }
            _ => w.state = WifiState::Unavailable,
        }
    } else if Some(e.call) == pending {
        let mut w = app.resource_mut::<Wifi>();
        w.pending = None;
        // Success needs no write: NetworkManager's PropertiesChanged follows.
        w.error = match &e.result {
            Ok(_) => None,
            Err(CallError::Remote { message, .. }) => Some(message.clone()),
            Err(other) => Some(format!("{other:?}")),
        };
    }
}

fn on_signal(app: &mut App, e: &SignalReceived) {
    let (props, owner) = {
        let w = app.resource::<Wifi>();
        (w.props, w.owner)
    };
    if e.matches.contains(&props) {
        let Ok((_, changed, _)) = PropertiesChanged::parse(&e.msg) else {
            return;
        };
        let enabled = changed
            .get_as::<bool>("WirelessEnabled")
            .and_then(Result::ok);
        let hardware = changed
            .get_as::<bool>("WirelessHardwareEnabled")
            .and_then(Result::ok);
        if enabled.is_none() && hardware.is_none() {
            return;
        }
        let mut w = app.resource_mut::<Wifi>();
        if let Some(v) = enabled {
            w.enabled = v;
        }
        if let Some(v) = hardware {
            w.hardware = v;
        }
        if w.load.is_none() {
            w.settle();
        }
    } else if e.matches.contains(&owner) {
        let Ok((_, _, new_owner)) = e.msg.body::<(String, String, String)>() else {
            return;
        };
        if new_owner.is_empty() {
            let mut w = app.resource_mut::<Wifi>();
            w.state = WifiState::Unavailable;
            w.pending = None;
        } else {
            reload(app);
        }
    }
}

fn on_toggle(app: &mut App, _: &ToggleWifi) {
    let (state, busy) = {
        let w = app.resource::<Wifi>();
        (w.state, w.pending.is_some())
    };
    let target = match state {
        WifiState::On => false,
        WifiState::Off => true,
        _ => return,
    };
    if busy {
        return;
    }
    let call = app.resource_mut::<DBus>().system().send(
        MethodCall::new(NM, NM_PATH, "org.freedesktop.DBus.Properties", "Set")
            .body(&(NM, "WirelessEnabled", Value::Bool(target)))
            .allow_interactive_auth(),
    );
    let mut w = app.resource_mut::<Wifi>();
    w.pending = Some(call);
    w.error = None;
}

fn on_wifi_changed(app: &mut App, _: &OnChanged<Wifi>) {
    let panels: Vec<NodeId> = app.widgets::<WifiPanel>().map(|(id, _)| id).collect();
    app.emit(WifiChanged, panels);
}

fn status_text(state: WifiState) -> &'static str {
    match state {
        WifiState::Loading => "Wi-Fi: …",
        WifiState::Unavailable => "NetworkManager unavailable",
        WifiState::On => "Wi-Fi: On",
        WifiState::Off => "Wi-Fi: Off",
        WifiState::HardwareOff => "Wi-Fi: Off (hardware switch)",
    }
}

/// A status line, an on/off button, and an error line.
struct WifiPanel;
struct WifiPanelBuilder;
impl Build for WifiPanelBuilder {
    type Widget = WifiPanel;
}

impl Widget for WifiPanel {
    type Builder = WifiPanelBuilder;
    fn build(_: WifiPanelBuilder, me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        *s.component_mut::<LayoutStyle>(me).unwrap() = LayoutStyle::default()
            .column()
            .center()
            .fill()
            .gap(px(12.0));
        *s.component_mut::<Paint>(me).unwrap() =
            Paint::Quad(Quad::new(s.color(ColorRole::Surface)));
        let status = s.spawn(
            me,
            text(status_text(WifiState::Loading))
                .variant(TextVariant::TitleLarge)
                .color(ColorRole::OnSurface),
        );
        let toggle = s.spawn(me, button().label("…").disabled(true));
        let error = s.spawn(me, text("").color(ColorRole::Error));

        s.on::<Clicked>(toggle, |ctx, _| ctx.signal(ToggleWifi));
        s.on::<WifiChanged>(me, move |ctx, _| {
            let (state, pending, err) = {
                let w = ctx.resource::<Wifi>();
                (w.state, w.pending.is_some(), w.error.clone())
            };
            ctx.at(status).unwrap().set_text(status_text(state));
            {
                let mut b = ctx.at(toggle).unwrap();
                let label = match (pending, state) {
                    (true, _) => "…",
                    (false, WifiState::On) => "Turn off",
                    (false, WifiState::Off) => "Turn on",
                    _ => "…",
                };
                b.set_label(label);
                b.set_disabled(pending || !matches!(state, WifiState::On | WifiState::Off));
            }
            ctx.at(error).unwrap().set_text(err.unwrap_or_default());
        });
        WifiPanel
    }
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
                .title("wifi")
                .layout(LayoutStyle::default().center().size(px(360.0), px(220.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));
        s.spawn(win, WifiPanelBuilder);
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
        .add_module(DBusModule::new().system())
        .add_module(WifiModule)
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.wifi".into(),
            budget: Budget::default(),
        });

    let root = app.root();
    app.spawn(root, ShellBuilder { root });
    app.run();
}
