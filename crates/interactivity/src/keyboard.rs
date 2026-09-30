//! Keyboard input, protocol-agnostic and event-driven.
//!
//! Mirrors the contact path: `presentation` (or a test) reduces a
//! `wl_keyboard` event to one [`KeyboardInput`] signal, and this crate's
//! system turns it into a [`KeyPress`](crate::KeyPress), [`KeyRepeat`](crate::KeyRepeat)
//! or [`KeyRelease`](crate::KeyRelease) event at the node that holds the
//! keyboard focus — the same shape [`ContactInput`](crate::ContactInput)
//! takes to [`Press`](crate::Press)/[`Release`](crate::Release). No
//! polling, no per-frame deltas to clear: each report becomes one event
//! the focused node's handler runs.
//!
//! Focus is a [`KeyboardFocus`] resource: whatever owns it sets it with
//! [`KeyboardFocus::focus`] when a node gains the keyboard focus and
//! clears it with [`KeyboardFocus::blur`] when it loses it. A report
//! with no focus is dropped — a keyboard with nowhere to go is a no-op.

use app::{NodeId, Resource, Signal};

/// A key, as `wl_keyboard.key` carries it: a linux evdev keycode. The
/// protocol's own words are "clients must add 8 to the key event keycode"
/// to get the xkb keycode; the value here is the raw evdev one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u32);

/// The linux evdev code for Backspace. A control key like this arrives
/// as a `wl_keyboard.key` event, not as a text-input edit, so consumers
/// match it directly rather than going through `zwp_text_input_v3`.
pub const KEY_BACKSPACE: KeyCode = KeyCode(14);

/// One key's transition this report. Mirrors `wl_keyboard.key`'s state
/// without naming the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pressed,
    Released,
    Repeated,
}

/// The modifier set in effect. The bit layout passed to [`Modifiers::from_bits`]
/// matches `wl_keyboard`'s combined modifier group (depressed | latched |
/// locked): 0x01 shift, 0x02 caps_lock, 0x04 ctrl, 0x08 alt, 0x10
/// num_lock, 0x20 scroll_lock, 0x40 logo (Super).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub logo: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
}

impl Modifiers {
    /// From the combined modifier bits a `wl_keyboard.modifiers` event
    /// carries. Named `from_bits`, not `from_wayland`, so a reducer in
    /// `presentation` can call it without this crate depending on
    /// `wayland`.
    pub fn from_bits(bits: u32) -> Self {
        Self {
            shift: bits & 0x01 != 0,
            caps_lock: bits & 0x02 != 0,
            ctrl: bits & 0x04 != 0,
            alt: bits & 0x08 != 0,
            num_lock: bits & 0x10 != 0,
            scroll_lock: bits & 0x20 != 0,
            logo: bits & 0x40 != 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// One report of keyboard input: `presentation`'s reduction of a
/// `wl_keyboard.key` event, with the latest modifiers folded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardInput {
    pub key: KeyCode,
    pub state: KeyState,
    pub modifiers: Modifiers,
}
impl Signal for KeyboardInput {}

/// Which node keyboard events dispatch to, if any. Set by whatever owns
/// keyboard focus when a node gains or loses it; this crate's system
/// reads it to route [`KeyPress`](crate::KeyPress)/
/// [`KeyRepeat`](crate::KeyRepeat)/[`KeyRelease`](crate::KeyRelease).
#[derive(Debug, Default)]
pub struct KeyboardFocus(Option<NodeId>);
impl Resource for KeyboardFocus {}

impl KeyboardFocus {
    pub fn new() -> Self {
        Self::default()
    }

    /// The node keyboard events are dispatching to, or `None`.
    pub fn focused(&self) -> Option<NodeId> {
        self.0
    }

    /// Route keyboard events to `node`.
    pub fn focus(&mut self, node: NodeId) {
        self.0 = Some(node);
    }

    /// Stop routing keyboard events anywhere.
    pub fn blur(&mut self) {
        self.0 = None;
    }
}
