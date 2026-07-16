use std::collections::HashMap;

use app::{RegisteredModule, prelude::*};
use wayland::{
    Handle, ObjectId, WaylandProxy, WlCompositorRequest, WlPointer, WlPointerEvent,
    WlPointerRequest, WlSeatCapability, WlSeatRequest,
};

use crate::{Compositor, protocols::wl_surface::SurfaceData};

// ── State ───────────────────────────────────────────────────────────────────────

#[derive(State)]
pub struct WlPointerState {
    /// Host compositor's wl_pointer (receives events from the host).
    pub pointer: Option<Handle<WlPointer>>,

    /// wl_pointer resources created by internal clients.
    pub client_pointers: Vec<Handle<WlPointer>>,

    /// The client surface (if any) that currently has pointer focus.
    pub focused_surface: Option<ObjectId>,

    /// Current host pointer position in compositor-window coordinates.
    pub host_x: i32,
    #[lens(skip)]
    pub host_y: i32,

    /// Maps a client surface's ObjectId to the WaylandProxy of the client
    /// that owns it, so we can route events to the correct wl_pointer.
    pub surface_clients: HashMap<ObjectId, WaylandProxy>,

    /// Monotonically increasing serial for enter/leave/button events we generate.
    #[lens(skip)]
    serial: u32,
}

impl WlPointerState {
    pub fn new() -> Self {
        Self {
            pointer: None,
            client_pointers: Vec::new(),
            focused_surface: None,
            host_x: 0,
            host_y: 0,
            surface_clients: HashMap::new(),
            serial: 1,
        }
    }

    pub fn retain_alive(&mut self) {
        self.client_pointers.retain(|p| p.is_alive());
    }

    pub fn on_capability_removed(&mut self) {
        self.client_pointers.clear();
        self.focused_surface = None;
        self.surface_clients.clear();
    }

    fn next_serial(&mut self) -> u32 {
        let s = self.serial;
        self.serial = self.serial.wrapping_add(1);
        s
    }
}

// ── Client-pointer lookup ───────────────────────────────────────────────────────

/// Send a pointer event to every *alive* client pointer that belongs to the
/// same connection as `proxy`.  Returns the number of pointers that were sent to.
fn send_to_client_ptrs(
    pointers: &[Handle<WlPointer>],
    proxy: &WaylandProxy,
    f: impl Fn(&Handle<WlPointer>),
) -> usize {
    let mut n = 0;
    for p in pointers {
        if p.is_alive() && p.proxy.is_same_connection(proxy) {
            f(p);
            n += 1;
        }
    }
    n
}

// ── Module ──────────────────────────────────────────────────────────────────────

pub fn module<S>() -> impl RegisteredModule<Compositor, S> {
    Module::<Compositor, _, _>::new()
        // ── Track surface → client mapping ──────────────────────────────────────
        .on(|compositor: &mut Compositor, ev: &WlCompositorRequest| {
            if let WlCompositorRequest::CreateSurface { id, sender } = ev {
                if let Some(sid) = id.object_id() {
                    compositor
                        .seat
                        .pointer_state
                        .surface_clients
                        .insert(sid, sender.proxy.clone());
                    println!("tracked surface {:?} for client", sid);
                }
            }
            hlist![]
        })
        // ── wl_seat.get_pointer ──────────────────────────────────────────────
        .on(|compositor: &mut Compositor, ev: &WlSeatRequest| {
            if let WlSeatRequest::GetPointer { id, .. } = ev {
                if let Some(caps) = compositor.seat.capability
                    && caps.contains(WlSeatCapability::Pointer)
                {
                    compositor
                        .seat
                        .pointer_state
                        .client_pointers
                        .push(id.clone());
                    println!("seat pointer: {:?}", id.object_id().expect("live pointer"));
                } else {
                    // TODO Send WlSeatError – through WlDisplay
                }
            }
        })
        // ── Events from the host compositor ──────────────────────────────────
        .on(|compositor: &mut Compositor, ev: &WlPointerEvent| {
            compositor.seat.pointer_state.retain_alive();

            match ev {
                // ── Enter ────────────────────────────────────────────────────
                WlPointerEvent::Enter {
                    sender: _,
                    serial: _,
                    surface: _,
                    surface_x,
                    surface_y,
                } => {
                    let wx = *surface_x;
                    let wy = *surface_y;
                    let new_serial;

                    // Phase 1 – immutable: hit-test surfaces.
                    let hit = compositor
                        .surfaces
                        .hit_test(&compositor.shm.buffers, wx, wy);

                    // Phase 2 – mutable: update pointer state.
                    {
                        let state = &mut compositor.seat.pointer_state;
                        state.host_x = wx;
                        state.host_y = wy;
                        new_serial = state.next_serial();

                        if let Some(hs) = &hit {
                            // Leave old surface if focus changed.
                            let old_id = state.focused_surface;
                            if let Some(old_id) = old_id {
                                if old_id != hs.id {
                                    if let Some(old_sd) = compositor.surfaces.surfaces.get(&old_id)
                                    {
                                        for p in &state.client_pointers {
                                            if p.is_alive()
                                                && p.proxy.is_same_connection(&old_sd.handle.proxy)
                                            {
                                                p.leave(new_serial, &old_sd.handle);
                                            }
                                        }
                                    }
                                }
                            }

                            // Enter the new surface.
                            let n = send_to_client_ptrs(
                                &state.client_pointers,
                                &hs.handle.proxy,
                                |p| p.enter(new_serial, &hs.handle, hs.local_x, hs.local_y),
                            );
                            state.focused_surface = Some(hs.id);

                            println!(
                                "ptr enter surface {:?} @ ({},{}), sent to {} ptr(s)",
                                hs.id, hs.local_x, hs.local_y, n
                            );
                        } else {
                            // No surface under the pointer – clear focus.
                            state_clear_focus(state, &compositor.surfaces.surfaces, new_serial);
                        }
                    }
                }

                // ── Leave ────────────────────────────────────────────────────
                WlPointerEvent::Leave {
                    sender: _,
                    serial: _,
                    surface: _,
                } => {
                    let ps = &mut compositor.seat.pointer_state;
                    let serial = ps.next_serial();
                    state_clear_focus(ps, &compositor.surfaces.surfaces, serial);
                    println!("ptr leave – focus cleared (serial {})", serial);
                }

                // ── Motion ───────────────────────────────────────────────────
                WlPointerEvent::Motion {
                    sender: _,
                    time,
                    surface_x,
                    surface_y,
                } => {
                    let wx = *surface_x;
                    let wy = *surface_y;

                    // Update stored position and check focus.
                    {
                        let state = &mut compositor.seat.pointer_state;
                        state.host_x = wx;
                        state.host_y = wy;
                    }

                    // Phase 1 – immutable: check if still on the same surface.
                    let focused_id = compositor.seat.pointer_state.focused_surface;
                    let still_focused = focused_id
                        .and_then(|fid| {
                            compositor.surfaces.surfaces.get(&fid).map(|sd| {
                                let (bw, bh) = sd
                                    .current
                                    .buffer
                                    .and_then(|bid| compositor.shm.buffers.get(&bid))
                                    .map(|b| (b.width, b.height))
                                    .unwrap_or((0, 0));
                                let region = sd.current.input_region.as_deref();
                                match region {
                                    None => wx >= 0 && wx < bw && wy >= 0 && wy < bh,
                                    Some(r) => r.contains(wx, wy),
                                }
                            })
                        })
                        .unwrap_or(false);

                    if still_focused {
                        // Still on the same surface – send motion.
                        let state = &mut compositor.seat.pointer_state;
                        if let Some(fid) = state.focused_surface {
                            if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                                send_to_client_ptrs(
                                    &state.client_pointers,
                                    &sd.handle.proxy,
                                    |p| p.motion(*time, wx, wy),
                                );
                            }
                        }
                        return;
                    }

                    // Surface switch or no surface – leave old, enter new.
                    let state = &mut compositor.seat.pointer_state;
                    let serial = state.next_serial();
                    state_clear_focus(state, &compositor.surfaces.surfaces, serial);

                    if let Some(hs) = compositor
                        .surfaces
                        .hit_test(&compositor.shm.buffers, wx, wy)
                    {
                        send_to_client_ptrs(&state.client_pointers, &hs.handle.proxy, |p| {
                            p.enter(serial, &hs.handle, hs.local_x, hs.local_y)
                        });
                        state.focused_surface = Some(hs.id);
                    }
                }

                // ── Button ───────────────────────────────────────────────────
                WlPointerEvent::Button {
                    sender: _,
                    serial: _,
                    time,
                    button,
                    state: btn_state,
                } => {
                    let ps = &mut compositor.seat.pointer_state;
                    if let Some(fid) = ps.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            let s = ps.next_serial();
                            send_to_client_ptrs(&ps.client_pointers, &sd.handle.proxy, |p| {
                                p.button(s, *time, *button, *btn_state)
                            });
                            println!(
                                "ptr button {:?} state {:?} – forwarded to surface {:?}",
                                button, btn_state, fid
                            );
                        }
                    }
                }

                // ── Axis ─────────────────────────────────────────────────────
                WlPointerEvent::Axis {
                    sender: _,
                    time,
                    axis,
                    value,
                } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.axis(*time, *axis, *value)
                            });
                        }
                    }
                }

                // ── Frame ────────────────────────────────────────────────────
                WlPointerEvent::Frame { .. } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.frame()
                            });
                        }
                    }
                }

                // ── AxisSource ───────────────────────────────────────────────
                WlPointerEvent::AxisSource {
                    sender: _,
                    axis_source,
                } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.axis_source(*axis_source)
                            });
                        }
                    }
                }

                // ── AxisStop ─────────────────────────────────────────────────
                WlPointerEvent::AxisStop {
                    sender: _,
                    time,
                    axis,
                } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.axis_stop(*time, *axis)
                            });
                        }
                    }
                }

                // ── AxisDiscrete ─────────────────────────────────────────────
                WlPointerEvent::AxisDiscrete {
                    sender: _,
                    axis,
                    discrete,
                } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.axis_discrete(*axis, *discrete)
                            });
                        }
                    }
                }

                // ── AxisValue120 ─────────────────────────────────────────────
                WlPointerEvent::AxisValue120 {
                    sender: _,
                    axis,
                    value120,
                } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.axis_value120(*axis, *value120)
                            });
                        }
                    }
                }

                // ── AxisRelativeDirection ────────────────────────────────────
                WlPointerEvent::AxisRelativeDirection {
                    sender: _,
                    axis,
                    direction,
                } => {
                    let state = &mut compositor.seat.pointer_state;
                    if let Some(fid) = state.focused_surface {
                        if let Some(sd) = compositor.surfaces.surfaces.get(&fid) {
                            send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                                p.axis_relative_direction(*axis, *direction)
                            });
                        }
                    }
                }
            }
        })
        // ── Requests from internal clients ───────────────────────────────────
        .on(|compositor: &mut Compositor, ev: &WlPointerRequest| {
            match ev {
                WlPointerRequest::SetCursor {
                    sender: _,
                    serial,
                    surface,
                    hotspot_x,
                    hotspot_y,
                } => {
                    // Forward cursor changes to the host compositor.
                    if let Some(host_ptr) = &compositor.seat.pointer_state.pointer {
                        host_ptr.set_cursor(*serial, surface.as_ref(), *hotspot_x, *hotspot_y);
                    }
                }
                WlPointerRequest::Release { sender } => {
                    compositor
                        .seat
                        .pointer_state
                        .client_pointers
                        .retain(|p| p.object_id() != sender.object_id());
                }
            }
        })
}

// ── Helpers ──────────────────────────────────────────────────────────────────────

/// Clear pointer focus: send `leave` to the currently focused surface's client(s)
/// and reset `state.focused_surface`.
fn state_clear_focus(
    state: &mut WlPointerState,
    surfaces: &HashMap<ObjectId, SurfaceData>,
    serial: u32,
) {
    if let Some(fid) = state.focused_surface.take() {
        if let Some(sd) = surfaces.get(&fid) {
            let n = send_to_client_ptrs(&state.client_pointers, &sd.handle.proxy, |p| {
                p.leave(serial, &sd.handle)
            });
            println!(
                "ptr leave surface {:?} (serial {}), sent to {} ptr(s)",
                fid, serial, n
            );
        }
    }
}
