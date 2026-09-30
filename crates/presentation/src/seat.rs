//! `wl_seat`'s pointer and touch, reduced to `interactivity::ContactInput`.
//! `Seat` is presentation's own state for this: the bound `wl_seat`, the
//! `wl_pointer`/`wl_touch` objects once requested, and (from later in
//! this file) the small bit of focus/position bookkeeping a reducer
//! needs that `interactivity::Contacts` cannot supply in time.
//! `App::signal` queues onto a list `App::flush` drains later, so a
//! `ContactInput` just sent has not reached `Contacts` yet when the next
//! protocol event for the same contact arrives.

use std::collections::HashMap;

use app::prelude::*;
use geometry::Point;
use interactivity::{
    ContactId, ContactInput, ContactPhase, KeyCode, KeyState, KeyboardInput, Modifiers,
};
use wayland::prelude::*;

use crate::Surfaces;

pub(crate) struct Seat {
    seat: WlSeat,
    pointer: Option<WlPointer>,
    touch: Option<WlTouch>,
    keyboard: Option<WlKeyboard>,
    /// The modifier set from the latest `wl_keyboard.modifiers`, folded
    /// into every `KeyboardInput` this seat signals. The two events
    /// arrive on separate `wl_keyboard` events and are joined here, at
    /// the reducer.
    modifiers: Modifiers,
    /// The window the pointer last entered, and its position there.
    /// `None` between a `Leave` and the next `Enter`.
    pointer_focus: Option<(NodeId, Point)>,
    /// Every touch point currently down: `wl_touch`'s id to the window
    /// it started on and its last known position.
    touches: HashMap<i32, (NodeId, Point)>,
}
impl Resource for Seat {}

impl Seat {
    pub(crate) fn new(seat: WlSeat) -> Seat {
        Seat {
            seat,
            pointer: None,
            touch: None,
            keyboard: None,
            modifiers: Modifiers::default(),
            pointer_focus: None,
            touches: HashMap::new(),
        }
    }
}

/// The seat's capabilities changed (or were reported for the first
/// time): request a pointer/touch object the first time each bit is
/// seen. Never released — capability loss at runtime is out of scope,
/// per the design doc.
pub(crate) fn on_seat(app: &mut App, e: &WlSeatEvent) {
    let WlSeatEvent::Capabilities { capabilities, .. } = e else {
        return;
    };
    let (mut seat, mut wl) = app.query::<(ResMut<Seat>, ResMut<Wayland>)>();
    let s = seat.seat;
    if capabilities.contains(WlSeatCapability::POINTER) && seat.pointer.is_none() {
        seat.pointer = Some(s.get_pointer(&mut wl));
    }
    if capabilities.contains(WlSeatCapability::TOUCH) && seat.touch.is_none() {
        seat.touch = Some(s.get_touch(&mut wl));
    }
    if capabilities.contains(WlSeatCapability::KEYBOARD) && seat.keyboard.is_none() {
        seat.keyboard = Some(s.get_keyboard(&mut wl));
    }
}

/// `wl_pointer` reduced to `ContactInput`. `Button` and `Leave` carry no
/// position of their own — the protocol's own words for `Button` are
/// "the location of the click is given by the last motion or enter
/// event", which is exactly `pointer_focus`, always fresh by
/// construction since every `Motion`/`Enter` updates it first.
pub(crate) fn on_pointer(app: &mut App, e: &WlPointerEvent) {
    match e {
        WlPointerEvent::Enter {
            surface,
            surface_x,
            surface_y,
            ..
        } => {
            let Some(window) = app.resource::<Surfaces>().window_of(surface.id()) else {
                app.resource_mut::<Seat>().pointer_focus = None;
                return;
            };
            let position = Point::new(*surface_x, *surface_y);
            app.resource_mut::<Seat>().pointer_focus = Some((window, position));
            app.signal(ContactInput {
                window,
                contact: ContactId::Mouse,
                phase: ContactPhase::Moved,
                position,
            });
        }
        WlPointerEvent::Motion {
            surface_x,
            surface_y,
            ..
        } => {
            let Some((window, _)) = app.resource::<Seat>().pointer_focus else {
                return;
            };
            let position = Point::new(*surface_x, *surface_y);
            app.resource_mut::<Seat>().pointer_focus = Some((window, position));
            app.signal(ContactInput {
                window,
                contact: ContactId::Mouse,
                phase: ContactPhase::Moved,
                position,
            });
        }
        WlPointerEvent::Button { state, .. } => {
            let Some((window, position)) = app.resource::<Seat>().pointer_focus else {
                return;
            };
            let phase = match state {
                WlPointerButtonState::Pressed => ContactPhase::Pressed,
                WlPointerButtonState::Released => ContactPhase::Released,
            };
            app.signal(ContactInput {
                window,
                contact: ContactId::Mouse,
                phase,
                position,
            });
        }
        WlPointerEvent::Leave { .. } => {
            let Some((window, position)) = app.resource_mut::<Seat>().pointer_focus.take() else {
                return;
            };
            app.signal(ContactInput {
                window,
                contact: ContactId::Mouse,
                phase: ContactPhase::Cancelled,
                position,
            });
        }
        _ => {}
    }
}

/// `wl_touch` reduced to `ContactInput`. A `Down` is two signals in
/// order — `Moved` then `Pressed`, both at the same position —
/// `interactivity` depends on exactly that ordering (see
/// `ContactPhase::Moved`'s doc). `Cancel` carries no `id` at all: the
/// protocol says it "applies to all touch points currently active on
/// this client", so every tracked touch is cancelled at once.
pub(crate) fn on_touch(app: &mut App, e: &WlTouchEvent) {
    match e {
        WlTouchEvent::Down {
            surface, id, x, y, ..
        } => {
            let Some(window) = app.resource::<Surfaces>().window_of(surface.id()) else {
                return;
            };
            let position = Point::new(*x, *y);
            app.resource_mut::<Seat>()
                .touches
                .insert(*id, (window, position));
            app.signal(ContactInput {
                window,
                contact: ContactId::Touch(*id as u32),
                phase: ContactPhase::Moved,
                position,
            });
            app.signal(ContactInput {
                window,
                contact: ContactId::Touch(*id as u32),
                phase: ContactPhase::Pressed,
                position,
            });
        }
        WlTouchEvent::Motion { id, x, y, .. } => {
            let Some((window, _)) = app.resource::<Seat>().touches.get(id).copied() else {
                return;
            };
            let position = Point::new(*x, *y);
            app.resource_mut::<Seat>()
                .touches
                .insert(*id, (window, position));
            app.signal(ContactInput {
                window,
                contact: ContactId::Touch(*id as u32),
                phase: ContactPhase::Moved,
                position,
            });
        }
        WlTouchEvent::Up { id, .. } => {
            let Some((window, position)) = app.resource_mut::<Seat>().touches.remove(id) else {
                return;
            };
            app.signal(ContactInput {
                window,
                contact: ContactId::Touch(*id as u32),
                phase: ContactPhase::Released,
                position,
            });
        }
        WlTouchEvent::Cancel { .. } => {
            let touches = std::mem::take(&mut app.resource_mut::<Seat>().touches);
            for (id, (window, position)) in touches {
                app.signal(ContactInput {
                    window,
                    contact: ContactId::Touch(id as u32),
                    phase: ContactPhase::Cancelled,
                    position,
                });
            }
        }
        _ => {}
    }
}

/// `wl_keyboard` reduced to `interactivity::KeyboardInput`. The only
/// event that becomes a signal is `Key`; `Modifiers` is folded in from
/// the latest report stored on [`Seat`]. `Keymap`, `Enter`, `Leave` and
/// `RepeatInfo` are read and dropped: this matches raw evdev keycodes,
/// so the keymap fd is not memory-mapped, and keyboard focus is the
/// `interactivity` crate's concern, surfaced as `KeyboardFocus`.
pub(crate) fn on_keyboard(app: &mut App, e: &WlKeyboardEvent) {
    match e {
        WlKeyboardEvent::Key { key, state, .. } => {
            let modifiers = app.resource::<Seat>().modifiers;
            let state = match state {
                WlKeyboardKeyState::Pressed => KeyState::Pressed,
                WlKeyboardKeyState::Released => KeyState::Released,
                WlKeyboardKeyState::Repeated => KeyState::Repeated,
            };
            app.signal(KeyboardInput {
                key: KeyCode(*key),
                state,
                modifiers,
            });
        }
        WlKeyboardEvent::Modifiers {
            mods_depressed,
            mods_latched,
            mods_locked,
            ..
        } => {
            app.resource_mut::<Seat>().modifiers =
                Modifiers::from_bits(*mods_depressed | *mods_latched | *mods_locked);
        }
        // `Keymap` (an fd to mmap), `Enter`/`Leave` (compositor surface
        // focus) and `RepeatInfo` have nowhere to go.
        _ => {}
    }
}
