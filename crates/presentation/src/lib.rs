#![forbid(unsafe_code)]
//! The window system integration: what puts a `Window` on the compositor.
//!
//! # Model
//!
//! - Every window gets a `wl_surface` and a shell role at its `Spawned`:
//!   an xdg toplevel by default, or a layer surface when the spawn bundle
//!   carries [`Role::Layer`]. The role is read once.
//! - The shell's configure is acked and settled: the shell's size where
//!   it gave one, else the window's `Layout`, else [`DEFAULT_SIZE`]; the
//!   window hears `Resized`. `preferred_buffer_scale` becomes
//!   `ScaleFactorChanged`; `close` and `closed` become `CloseRequested`,
//!   which this module only reports.
//! - The frame loop: `FrameRequested` marks the window wanting; when it
//!   is configured, no callback is outstanding and a slot is free, this
//!   module signals `Frame`. Its own `Frame` system runs last of all: it
//!   asks `Scenes::queue` for the free slot's age, draws the queue into
//!   the slot's dmabuf through `gles`, attaches, damages each rect of the
//!   queue's scissor, asks for the callback and commits. An empty scissor
//!   commits nothing. The callback's `done` and a buffer's `release` each
//!   try again.
//! - A configure settles the size and, when the device size changed,
//!   replaces the two slots, and always kicks. It must: `window` lets one
//!   frame request stand until a `Frame` answers it, so a configure that
//!   declined would strand the request the window's own content raised
//!   before the compositor ever configured it, and nothing would ever
//!   draw. A `Frame` whose queue is still the old size draws nothing and
//!   wants again; the layout the `Resized` causes then opens the next
//!   cycle, which draws at the new size. A preferred scale change
//!   replaces the slots and kicks.
//! - The atlas is uploaded on `OnChanged<Atlas>`, which the core queues
//!   at `PostTick` ahead of any frame request the same tick raises.
//! - Removal destroys the role objects, the buffers and targets, and the
//!   surface.
//!
//! Installs last of everything, after `RenderModule` and `WaylandModule`,
//! which must have bound `WlCompositor`, `ZwpLinuxDmabufV1`, `XdgWmBase`
//! and `WlSeat`. Opens the GPU at install; no GPU is a panic.
//!
//! # Quick start
//!
//! ```no_run
//! use app::prelude::*;
//! use atlas::Atlas;
//! use gles::Budget;
//! use layout::prelude::*;
//! use paint::prelude::*;
//! use presentation::prelude::*;
//! use render::prelude::*;
//! use wayland::fake::Fake;
//! use wayland::prelude::*;
//! use window::prelude::*;
//!
//! let globals = [
//!     ("wl_compositor", 6),
//!     ("zwp_linux_dmabuf_v1", 3),
//!     ("xdg_wm_base", 7),
//!     ("wl_seat", 9),
//! ];
//! let mut f = Fake::new(&globals, |m| {
//!     m.bind::<WlCompositor>()
//!         .bind::<ZwpLinuxDmabufV1>()
//!         .bind::<XdgWmBase>()
//!         .bind::<WlSeat>()
//! });
//! f.app
//!     .add_module(LayoutModule)
//!     .add_module(PaintModule)
//!     .add_module(WindowModule)
//!     .add_module(RenderModule::default());
//! f.app.insert_resource(Atlas::new());
//! f.app.add_module(PresentationModule { app_id: "example".into(), budget: Budget::default() });
//! let root = f.app.root();
//! let win = f.app.spawn(root, window().title("hi")).id();
//! f.app.tick();
//! f.turn();
//! // The compositor configures the toplevel (id 10) and its xdg surface (id 9).
//! f.send(10, 0, |w| { w.int(320); w.int(200); w.array(&[]); });
//! f.send(9, 0, |w| w.uint(1));
//! f.turn();
//! assert!(f.app.resource::<Surfaces>().is_configured(win));
//! assert_eq!(f.app.component::<LayoutStyle>(win).unwrap().width, px(320.0));
//! ```

use std::collections::HashMap;

use app::prelude::*;
use atlas::Atlas;
use geometry::{Rect, Size};
use gles::{Budget, Device};
use layout::prelude::*;
use render::Scenes;
use wayland::prelude::*;
use window::prelude::*;

mod seat;
mod slots;

use seat::{Seat, on_keyboard, on_pointer, on_seat, on_touch};
use slots::Slots;

pub use wayland::{
    ZwlrLayerShellV1Layer as Layer, ZwlrLayerSurfaceV1Anchor as Anchor,
    ZwlrLayerSurfaceV1KeyboardInteractivity as KeyboardInteractivity,
};

pub mod prelude {
    pub use crate::{
        Anchor, BUFFERS, DEFAULT_SIZE, KeyboardInteractivity, Layer, LayerRole, PresentationModule,
        Role, Surfaces,
    };
}

/// The size of a toplevel the compositor leaves to us when the window
/// has no content to size it.
pub const DEFAULT_SIZE: Size = Size::new(640.0, 480.0);

/// How many slots a window holds. `RenderModule::buffers` must be at
/// least this, which its default is, for a free slot's scissor to be
/// exact; `PresentationModule::install` asserts it.
pub const BUFFERS: usize = 2;

/// `DRM_FORMAT_MOD_INVALID`: a compositor's way of saying "implicit",
/// which GBM cannot be asked for.
const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// What a window is to the shell. Read once, at the window's `Spawned`;
/// set it through the spawn bundle. Rewriting it later changes nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Role {
    #[default]
    Toplevel,
    Layer(LayerRole),
}
impl Component for Role {}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerRole {
    pub layer: Layer,
    pub anchor: Anchor,
    pub exclusive_zone: i32,
    pub namespace: String,
    pub keyboard_interactivity: KeyboardInteractivity,
}

enum Shell {
    Toplevel {
        xdg: XdgSurface,
        toplevel: XdgToplevel,
    },
    Layer(ZwlrLayerSurfaceV1),
}

struct Entry {
    surface: WlSurface,
    shell: Shell,
    /// The toplevel's last proposal; zero means ours to choose.
    proposed: (i32, i32),
    /// The settled logical size.
    size: Size,
    scale: i32,
    scale_sent: i32,
    configured: bool,
    callback: Option<WlCallback>,
    wanting: bool,
    slots: Option<Slots>,
    frames: u64,
}

/// Presentation's own state: one entry per window it put on screen, and
/// the owner of every object it created.
pub struct Surfaces {
    entries: HashMap<NodeId, Entry>,
    owner: HashMap<ObjectId, NodeId>,
    layer_shell: Option<ZwlrLayerShellV1>,
    app_id: String,
    device: Device,
    dmabuf: ZwpLinuxDmabufV1,
    modifiers: Vec<u64>,
}
impl Resource for Surfaces {}

impl Surfaces {
    pub fn surface_of(&self, window: NodeId) -> Option<WlSurface> {
        self.entries.get(&window).map(|e| e.surface)
    }

    /// Re-send `set_size` on `window`'s `zwlr_layer_surface_v1`, requesting a
    /// new width/height. `0` defers to the compositor on that axis (e.g. a
    /// width of `0` with left+right anchored stretches full-width). The
    /// compositor answers with a `Configure`; `settle` then reports `Resized`
    /// and the window's layout updates to the new size. No-op for a toplevel
    /// or an unknown window.
    ///
    /// This is the dynamic-resize path that [`on_spawned`](#) only opens at
    /// surface creation: call it whenever the window's desired size changes
    /// after the surface exists (e.g. a keyboard that collapses to a toggle
    /// bar when no text input is focused).
    pub fn request_layer_size(
        &mut self,
        window: NodeId,
        width: u32,
        height: u32,
        wl: &mut Wayland,
    ) {
        let Some(entry) = self.entries.get(&window) else {
            return;
        };
        let Shell::Layer(ls) = &entry.shell else {
            return;
        };
        let ls = *ls;
        let surface = entry.surface;
        ls.set_size(wl, width, height);
        surface.commit(wl);
    }

    pub fn is_configured(&self, window: NodeId) -> bool {
        self.entries.get(&window).is_some_and(|e| e.configured)
    }

    /// The window that owns `surface`, if any. `seat.rs`'s pointer/touch
    /// reducer needs no other part of an entry.
    pub(crate) fn window_of(&self, surface: ObjectId) -> Option<NodeId> {
        self.owner.get(&surface).copied()
    }

    fn entry_of(&mut self, object: ObjectId) -> Option<(NodeId, &mut Entry)> {
        let w = *self.owner.get(&object)?;
        self.entries.get_mut(&w).map(|e| (w, e))
    }

    /// The pixels of the slot most recently drawn for `window`, as
    /// `(width, height, RGBA8 rows top-down)`. `None` before the first
    /// frame. For tests.
    pub fn last_frame(&mut self, window: NodeId) -> Option<(u32, u32, Vec<u8>)> {
        let e = self.entries.get(&window)?;
        let slots = e.slots.as_ref()?;
        let slot = slots
            .slots
            .iter()
            .filter(|s| s.drawn.is_some())
            .max_by_key(|s| s.drawn)?;
        Some((slots.width, slots.height, self.device.read(&slot.target)))
    }
}

/// Registers `Role`, inserts `Surfaces`, opens the GPU, binds the layer
/// shell if the compositor offers one. Installs last of everything.
///
/// # Panics
///
/// If `WlCompositor`, `ZwpLinuxDmabufV1`, `XdgWmBase` or `WlSeat` were not
/// bound by `WaylandModule`, if `wl_compositor` is below version 4 or
/// `zwp_linux_dmabuf_v1` below 3, or if the GPU does not open with a
/// GLES 3.0 context.
pub struct PresentationModule {
    pub app_id: String,
    /// Room on the GPU for the atlas, allocated once at install.
    pub budget: Budget,
}

impl Module for PresentationModule {
    fn install(self, app: &mut App) {
        let compositor = *app.resource::<WlCompositor>();
        let dmabuf = *app.resource::<ZwpLinuxDmabufV1>();
        let _ = app.resource::<XdgWmBase>();
        let seat = *app.resource::<WlSeat>();
        {
            let wl = app.resource::<Wayland>();
            assert!(
                wl.version(compositor) >= 4,
                "presentation: wl_compositor must be version 4 or above for damage_buffer"
            );
            assert!(
                wl.version(dmabuf) >= 3,
                "presentation: zwp_linux_dmabuf_v1 must be version 3 or above for modifier events"
            );
        }
        assert!(
            app.resource::<Scenes>().buffers() >= BUFFERS,
            "presentation: a window holds {BUFFERS} slots, so a slot can be {BUFFERS} frames old, but render keeps only {} frames of damage: raise RenderModule::buffers to {BUFFERS}",
            app.resource::<Scenes>().buffers()
        );
        let device = Device::open(self.budget);
        let layer_shell = app
            .resource::<Globals>()
            .find(ZwlrLayerShellV1::NAME)
            .cloned()
            .map(|g| {
                let (globals, mut wl) = app.query::<(Res<Globals>, ResMut<Wayland>)>();
                globals.bind::<ZwlrLayerShellV1>(&g, &mut wl)
            });
        app.register_component::<Role>().insert_resource(Surfaces {
            entries: HashMap::new(),
            owner: HashMap::new(),
            layer_shell,
            app_id: self.app_id,
            device,
            dmabuf,
            modifiers: Vec::new(),
        });
        app.insert_resource(Seat::new(seat));
        app.system(on_spawned)
            .system(on_wm_base)
            .system(on_dmabuf)
            .system(on_seat)
            .system(on_pointer)
            .system(on_touch)
            .system(on_keyboard)
            .system(on_xdg_surface)
            .system(on_toplevel)
            .system(on_layer_surface)
            .system(on_surface)
            .system(on_frame_requested)
            .system(on_atlas_changed)
            .system(on_callback)
            .system(on_buffer)
            .system(on_removed)
            .system(on_frame);
    }
}

/// The compositor's layouts for XRGB8888, in the order advertised;
/// `format` events and the implicit modifier are ignored.
fn on_dmabuf(app: &mut App, e: &ZwpLinuxDmabufV1Event) {
    if let ZwpLinuxDmabufV1Event::Modifier {
        format,
        modifier_hi,
        modifier_lo,
        ..
    } = e
        && *format == gles::XRGB8888
    {
        let m = ((*modifier_hi as u64) << 32) | *modifier_lo as u64;
        if m != MOD_INVALID {
            app.resource_mut::<Surfaces>().modifiers.push(m);
        }
    }
}

fn px_or_zero(v: Val) -> u32 {
    match v {
        Val::Px(p) => p.max(0.0) as u32,
        _ => 0,
    }
}

/// A window (its `InWindow` is itself) gets a surface, its role objects
/// and the initial commit with no buffer.
fn on_spawned(app: &mut App, s: &Spawned) {
    let is_window = app
        .component::<InWindow>(s.id)
        .is_some_and(|w| w.0 == Some(s.id));
    if !is_window {
        return;
    }
    let role = app.component::<Role>(s.id).cloned().unwrap_or_default();
    let title = app
        .widget::<Window>(s.id)
        .map(|w| w.title.clone())
        .unwrap_or_default();
    let requested = {
        let style = app
            .component::<LayoutStyle>(s.id)
            .expect("a window has a style");
        (px_or_zero(style.width), px_or_zero(style.height))
    };
    let compositor = *app.resource::<WlCompositor>();
    let wm_base = *app.resource::<XdgWmBase>();

    let (mut surfaces, mut wl) = app.query::<(ResMut<Surfaces>, ResMut<Wayland>)>();
    let surface = compositor.create_surface(&mut wl);
    let shell = match role {
        Role::Toplevel => {
            let xdg = wm_base.get_xdg_surface(&mut wl, surface);
            let toplevel = xdg.get_toplevel(&mut wl);
            toplevel.set_title(&mut wl, &title);
            toplevel.set_app_id(&mut wl, &surfaces.app_id);
            surfaces.owner.insert(xdg.id(), s.id);
            surfaces.owner.insert(toplevel.id(), s.id);
            Shell::Toplevel { xdg, toplevel }
        }
        Role::Layer(l) => {
            let shell = surfaces.layer_shell.expect(
                "presentation: a Role::Layer window needs zwlr_layer_shell_v1, which the compositor does not offer",
            );
            let ls = shell.get_layer_surface(&mut wl, surface, None, l.layer, &l.namespace);
            ls.set_anchor(&mut wl, l.anchor);
            ls.set_exclusive_zone(&mut wl, l.exclusive_zone);
            ls.set_keyboard_interactivity(&mut wl, l.keyboard_interactivity);
            ls.set_size(&mut wl, requested.0, requested.1);
            surfaces.owner.insert(ls.id(), s.id);
            Shell::Layer(ls)
        }
    };
    surface.commit(&mut wl);
    surfaces.owner.insert(surface.id(), s.id);
    surfaces.entries.insert(
        s.id,
        Entry {
            surface,
            shell,
            proposed: (0, 0),
            size: Size::ZERO,
            scale: 1,
            scale_sent: 1,
            configured: false,
            callback: None,
            wanting: false,
            slots: None,
            frames: 0,
        },
    );
}

fn on_wm_base(app: &mut App, e: &XdgWmBaseEvent) {
    let XdgWmBaseEvent::Ping { wm_base, serial } = e;
    wm_base.pong(&mut app.resource_mut::<Wayland>(), *serial);
}

fn device_size(size: Size, scale: i32) -> (u32, u32) {
    let s = scale.max(1) as f32;
    (
        ((size.width * s).round() as u32).max(1),
        ((size.height * s).round() as u32).max(1),
    )
}

/// Forget both buffers' owner rows, then destroy the buffers and targets.
fn teardown_slots(
    slots: Slots,
    owner: &mut HashMap<ObjectId, NodeId>,
    device: &mut Device,
    wl: &mut Wayland,
) {
    for s in &slots.slots {
        owner.remove(&s.buffer.id());
    }
    slots.destroy(device, wl);
}

/// Drop the slots, if any, and make new ones at the entry's size and
/// scale, owned by `w`.
fn replace_slots(
    entry: &mut Entry,
    owner: &mut HashMap<ObjectId, NodeId>,
    device: &mut Device,
    dmabuf: ZwpLinuxDmabufV1,
    modifiers: &[u64],
    wl: &mut Wayland,
    w: NodeId,
) {
    if let Some(old) = entry.slots.take() {
        teardown_slots(old, owner, device, wl);
    }
    let (dw, dh) = device_size(entry.size, entry.scale);
    let slots = Slots::create(device, wl, dmabuf, modifiers, dw, dh);
    for s in &slots.slots {
        owner.insert(s.buffer.id(), w);
    }
    entry.slots = Some(slots);
}

/// The shell configured `w`: settle the size, make slots if the device
/// size changed, report `Resized`, and kick. The kick is unconditional
/// because `window` swallows every `RequestFrame` while one is pending
/// and only a `Frame` clears that bit: the request a window raises for
/// its own content arrives before the first configure, so a configure
/// that declined to kick would leave it pending forever. When the size
/// the queue was built at is not yet the slots', `on_frame` declines to
/// draw and wants again, and the relayout this `Resized` causes requests
/// the frame that fits.
fn settle(app: &mut App, w: NodeId, proposed: (i32, i32)) {
    let layout = app
        .component::<Layout>(w)
        .map(|l| l.rect.size)
        .unwrap_or(Size::ZERO);
    let pick = |p: i32, l: f32, d: f32| {
        if p > 0 {
            p as f32
        } else if l > 0.0 {
            l
        } else {
            d
        }
    };
    let size = Size::new(
        pick(proposed.0, layout.width, DEFAULT_SIZE.width),
        pick(proposed.1, layout.height, DEFAULT_SIZE.height),
    );
    {
        let (mut surfaces, mut wl) = app.query::<(ResMut<Surfaces>, ResMut<Wayland>)>();
        let s = &mut *surfaces;
        let Some(entry) = s.entries.get_mut(&w) else {
            return;
        };
        entry.size = size;
        let (dw, dh) = device_size(size, entry.scale);
        let stale = entry
            .slots
            .as_ref()
            .is_none_or(|b| (b.width, b.height) != (dw, dh));
        if stale {
            replace_slots(
                entry,
                &mut s.owner,
                &mut s.device,
                s.dmabuf,
                &s.modifiers,
                &mut wl,
                w,
            );
        }
        entry.configured = true;
        entry.wanting = true;
    }
    app.emit(Resized { size }, w);
    kick(app, w);
}

/// The one decision: configured, no callback outstanding, a slot free
/// and wanting gives `Frame(w)`. Otherwise the next configure, `done` or
/// `release` kicks again.
fn kick(app: &mut App, w: NodeId) {
    let ready = app.resource::<Surfaces>().entries.get(&w).is_some_and(|e| {
        e.configured
            && e.callback.is_none()
            && e.wanting
            && e.slots.as_ref().is_some_and(|b| b.free_slot().is_some())
    });
    if ready {
        app.resource_mut::<Surfaces>()
            .entries
            .get_mut(&w)
            .unwrap()
            .wanting = false;
        app.signal(Frame(w));
    }
}

fn on_toplevel(app: &mut App, e: &XdgToplevelEvent) {
    match e {
        XdgToplevelEvent::Configure {
            toplevel,
            width,
            height,
            ..
        } => {
            if let Some((_, entry)) = app.resource_mut::<Surfaces>().entry_of(toplevel.id()) {
                entry.proposed = (*width, *height);
            }
        }
        XdgToplevelEvent::Close { toplevel } => {
            if let Some(w) = app
                .resource::<Surfaces>()
                .owner
                .get(&toplevel.id())
                .copied()
            {
                app.emit(CloseRequested, w);
            }
        }
        _ => {}
    }
}

fn on_xdg_surface(app: &mut App, e: &XdgSurfaceEvent) {
    let XdgSurfaceEvent::Configure { surface, serial } = e;
    let found = {
        let (mut surfaces, mut wl) = app.query::<(ResMut<Surfaces>, ResMut<Wayland>)>();
        surfaces.entry_of(surface.id()).map(|(w, entry)| {
            surface.ack_configure(&mut wl, *serial);
            (w, entry.proposed)
        })
    };
    if let Some((w, proposed)) = found {
        settle(app, w, proposed);
    }
}

fn on_layer_surface(app: &mut App, e: &ZwlrLayerSurfaceV1Event) {
    match e {
        ZwlrLayerSurfaceV1Event::Configure {
            layer_surface,
            serial,
            width,
            height,
        } => {
            let found = {
                let (mut surfaces, mut wl) = app.query::<(ResMut<Surfaces>, ResMut<Wayland>)>();
                surfaces.entry_of(layer_surface.id()).map(|(w, _)| {
                    layer_surface.ack_configure(&mut wl, *serial);
                    w
                })
            };
            if let Some(w) = found {
                settle(app, w, (*width as i32, *height as i32));
            }
        }
        ZwlrLayerSurfaceV1Event::Closed { layer_surface } => {
            if let Some(w) = app
                .resource::<Surfaces>()
                .owner
                .get(&layer_surface.id())
                .copied()
            {
                app.emit(CloseRequested, w);
            }
        }
    }
}

fn on_frame_requested(app: &mut App, r: &FrameRequested) {
    if let Some(entry) = app.resource_mut::<Surfaces>().entries.get_mut(&r.0) {
        entry.wanting = true;
    }
    kick(app, r.0);
}

/// The atlas changed this tick: upload before any frame the same tick's
/// changes request. `OnChanged<Atlas>` is queued by the core at
/// `PostTick`, ahead of the `RequestFrame` that render's own `OnChanged`
/// handlers raise, so a page always has its texture before a command
/// samples it.
fn on_atlas_changed(app: &mut App, _: &OnChanged<Atlas>) {
    let (mut surfaces, atlas) = app.query::<(ResMut<Surfaces>, Res<Atlas>)>();
    surfaces.device.upload(&atlas);
}

/// The drawer and the commit: the last `Frame` system to run.
fn on_frame(app: &mut App, f: &Frame) {
    let w = f.0;
    let (mut surfaces, mut wl, mut scenes) =
        app.query::<(ResMut<Surfaces>, ResMut<Wayland>, ResMut<Scenes>)>();
    let s = &mut *surfaces;
    let Some(entry) = s.entries.get_mut(&w) else {
        return;
    };
    if !entry.configured {
        return;
    }
    if entry.callback.is_some() {
        entry.wanting = true;
        return;
    }
    let Some(slots) = entry.slots.as_mut() else {
        return;
    };
    let Some(i) = slots.free_slot() else {
        entry.wanting = true;
        return;
    };
    let age = slots.slots[i]
        .drawn
        .map(|d| (entry.frames - d) as usize)
        .unwrap_or(0);
    let Some(queue) = scenes.queue(w, age) else {
        return;
    };
    if queue.scissor.is_empty() {
        return;
    }
    // Two roundings of the same size must agree: the slots' came from
    // `device_size`, which rounds the logical size times the scale, and
    // the queue's is what `render` walked at. Round here too — truncating
    // would make a fractional device size differ from the slots' forever,
    // and every frame would decline and want again.
    let want = (
        queue.size.width.round() as u32,
        queue.size.height.round() as u32,
    );
    if want != (slots.width, slots.height) {
        // A frame between a configure and the layout that follows it:
        // the layout's change requests the frame that fits.
        entry.wanting = true;
        return;
    }
    let slot = &mut slots.slots[i];
    s.device.draw(&slot.target, queue);
    if entry.scale != entry.scale_sent {
        entry.surface.set_buffer_scale(&mut wl, entry.scale);
        entry.scale_sent = entry.scale;
    }
    entry.surface.attach(&mut wl, Some(slot.buffer), 0, 0);
    for &r in &queue.scissor {
        let (x, y, dw, dh) = outward(r);
        entry.surface.damage_buffer(&mut wl, x, y, dw, dh);
    }
    let callback = entry.surface.frame(&mut wl);
    s.owner.insert(callback.id(), w);
    entry.callback = Some(callback);
    entry.surface.commit(&mut wl);
    slot.held = true;
    slot.drawn = Some(entry.frames);
    entry.frames += 1;
}

/// A device-pixel rect rounded outward: `(x, y, width, height)` as
/// `damage_buffer` takes them.
fn outward(r: Rect) -> (i32, i32, i32, i32) {
    let x0 = r.x().floor() as i32;
    let y0 = r.y().floor() as i32;
    let x1 = r.right().ceil() as i32;
    let y1 = r.bottom().ceil() as i32;
    (x0, y0, x1 - x0, y1 - y0)
}

fn on_callback(app: &mut App, e: &WlCallbackEvent) {
    let WlCallbackEvent::Done { callback, .. } = e;
    let w = {
        let mut surfaces = app.resource_mut::<Surfaces>();
        let Some(w) = surfaces.owner.remove(&callback.id()) else {
            return;
        };
        if let Some(entry) = surfaces.entries.get_mut(&w)
            && entry.callback == Some(*callback)
        {
            entry.callback = None;
        }
        w
    };
    kick(app, w);
}

fn on_buffer(app: &mut App, e: &WlBufferEvent) {
    let WlBufferEvent::Release { buffer } = e;
    let w = {
        let mut surfaces = app.resource_mut::<Surfaces>();
        let Some((w, entry)) = surfaces.entry_of(buffer.id()) else {
            return;
        };
        if let Some(b) = entry.slots.as_mut()
            && let Some(slot) = b.slots.iter_mut().find(|s| s.buffer == *buffer)
        {
            slot.held = false;
        }
        w
    };
    kick(app, w);
}

/// A new preferred scale: report it, and if configured rescale the
/// slots and want a frame. The same scale again does nothing.
fn on_surface(app: &mut App, e: &WlSurfaceEvent) {
    let WlSurfaceEvent::PreferredBufferScale { surface, factor } = e else {
        return;
    };
    let changed = {
        let (mut surfaces, mut wl) = app.query::<(ResMut<Surfaces>, ResMut<Wayland>)>();
        let s = &mut *surfaces;
        let Some(w) = s.owner.get(&surface.id()).copied() else {
            return;
        };
        let entry = s.entries.get_mut(&w).expect("owned objects have entries");
        if entry.scale == *factor {
            None
        } else {
            entry.scale = *factor;
            if entry.configured {
                replace_slots(
                    entry,
                    &mut s.owner,
                    &mut s.device,
                    s.dmabuf,
                    &s.modifiers,
                    &mut wl,
                    w,
                );
                entry.wanting = true;
            }
            Some(w)
        }
    };
    if let Some(w) = changed {
        app.emit(
            ScaleFactorChanged {
                scale: *factor as f32,
            },
            w,
        );
        kick(app, w);
    }
}

/// Every entry whose window is gone is torn down: the role objects, the
/// slots and their targets, then the surface. A late event for any of
/// them finds no owner and is skipped.
fn on_removed(app: &mut App, _: &Removed) {
    let gone: Vec<NodeId> = {
        let surfaces = app.resource::<Surfaces>();
        surfaces
            .entries
            .keys()
            .copied()
            .filter(|&w| !app.is_live(w))
            .collect()
    };
    if gone.is_empty() {
        return;
    }
    let (mut surfaces, mut wl) = app.query::<(ResMut<Surfaces>, ResMut<Wayland>)>();
    let s = &mut *surfaces;
    for w in gone {
        let entry = s.entries.remove(&w).unwrap();
        match entry.shell {
            Shell::Toplevel { xdg, toplevel } => {
                toplevel.destroy(&mut wl);
                xdg.destroy(&mut wl);
                s.owner.remove(&toplevel.id());
                s.owner.remove(&xdg.id());
            }
            Shell::Layer(ls) => {
                ls.destroy(&mut wl);
                s.owner.remove(&ls.id());
            }
        }
        if let Some(slots) = entry.slots {
            teardown_slots(slots, &mut s.owner, &mut s.device, &mut wl);
        }
        entry.surface.destroy(&mut wl);
        s.owner.remove(&entry.surface.id());
        if let Some(cb) = entry.callback {
            s.owner.remove(&cb.id());
        }
    }
}
