use std::any::Any;

use app::prelude::*;
use app::{Poll, PrePoll, Start};
use io_ring::Ring;
use wayland::{
    ExtSessionLockManagerV1, ExtSessionLockSurfaceV1, ExtSessionLockSurfaceV1Event,
    ExtSessionLockV1, ExtSessionLockV1Event, Handle, Interface, WlOutput, WlPointerButtonState,
    WlPointerEvent, WlRegistryEvent,
};
use window_manager::prelude::*;
use window_manager::{Color, Surface};

const PANEL_GREEN: Color = Color::from_rgb8(36, 92, 56); // xdg window: "lock"
const LOCK_RED: Color = Color::from_rgb8(120, 34, 40); // lock surface: "unlock"
const BTN_LEFT: u32 = 0x110;

struct SessionLockSurface {
    lock: Handle<ExtSessionLockV1>,
    lock_surface: Handle<ExtSessionLockSurfaceV1>,
    locked: bool,
}

impl Surface for SessionLockSurface {
    fn ack_configure(&self, serial: u32) {
        self.lock_surface.ack_configure(serial);
    }

    fn destroy(&mut self) {
        if self.locked {
            self.lock.unlock_and_destroy();
        } else {
            self.lock.destroy();
        }
        self.lock_surface.destroy();
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(State)]
struct LockScreen {
    ring: Ring,
    wm: WindowManager,
    #[lens(skip)]
    control: WindowHandle<()>,
    #[lens(skip)]
    manager: Option<Handle<ExtSessionLockManagerV1>>,
    #[lens(skip)]
    output: Option<Handle<WlOutput>>,
    #[lens(skip)]
    lock_window: Option<WindowHandle<()>>,
}

fn on_registry(s: &mut LockScreen, ev: &WlRegistryEvent) {
    let WlRegistryEvent::Global {
        sender,
        name,
        interface,
        version,
    } = ev
    else {
        return;
    };
    match interface.as_str() {
        ExtSessionLockManagerV1::NAME => s.manager = Some(sender.bind(*name, *version)),
        WlOutput::NAME if s.output.is_none() => s.output = Some(sender.bind(*name, *version)),
        _ => {}
    }
}

fn on_pointer(s: &mut LockScreen, ev: &WlPointerEvent) {
    let WlPointerEvent::Button {
        state: WlPointerButtonState::Pressed,
        button,
        ..
    } = ev
    else {
        return;
    };
    if *button != BTN_LEFT {
        return;
    }
    let Some(target) = s.wm.current_pointer_window() else {
        return;
    };

    match s.lock_window {
        None if target == s.control.id() => lock(s),
        Some(handle) if target == handle.id() => {
            s.wm.destroy(handle.id());
            s.lock_window = None;
        }
        _ => {}
    }
}

fn lock(s: &mut LockScreen) {
    let (Some(manager), Some(output)) = (s.manager.clone(), s.output.clone()) else {
        return;
    };

    let surface = s.wm.create_surface();
    let lock = manager.lock();
    let lock_surface = lock.get_lock_surface(&surface, &output);

    let handle = s.wm.spawn_window_with(
        0,
        0,
        LOCK_RED,
        (),
        surface,
        Box::new(SessionLockSurface {
            lock,
            lock_surface,
            locked: false,
        }),
    );
    s.lock_window = Some(handle);
}

fn on_configure(s: &mut LockScreen, ev: &ExtSessionLockSurfaceV1Event) {
    let ExtSessionLockSurfaceV1Event::Configure {
        serial,
        width,
        height,
        ..
    } = ev;
    if let Some(handle) = s.lock_window {
        s.wm.configure(handle.id(), *serial, *width, *height);
    }
}

fn on_lock(s: &mut LockScreen, ev: &ExtSessionLockV1Event) {
    match ev {
        ExtSessionLockV1Event::Locked { .. } => {
            if let Some(handle) = s.lock_window {
                if let Some(role) = s.wm.surface_mut::<SessionLockSurface>(handle.id()) {
                    role.locked = true;
                }
            }
        }
        ExtSessionLockV1Event::Finished { .. } => {
            if let Some(handle) = s.lock_window.take() {
                s.wm.destroy(handle.id());
            }
        }
    }
}

fn main() {
    let ring = Ring::default();
    let mut wm = WindowManager::new(ring.proxy());

    let control = wm.spawn_window(
        WindowSettings {
            width: 400,
            height: 300,
            clear_color: PANEL_GREEN,
            kind: WindowKind::Xdg {
                title: "lock control — click to lock".into(),
            },
            touch_config: None,
            gesture_config: None,
        },
        (),
    );

    let state = LockScreen {
        ring,
        wm,
        control,
        manager: None,
        output: None,
        lock_window: None,
    };

    let mut app = App::new(state)
        .mount(io_ring::module())
        .mount(window_manager::module())
        .mount(
            Module::new()
                .on(on_registry)
                .on(on_pointer)
                .on(on_configure)
                .on(on_lock),
        );

    app.dispatch(&Start);
    loop {
        app.dispatch(&PrePoll);
        app.dispatch(&Poll);
    }
}
