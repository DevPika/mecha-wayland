mod event;

pub use event::PointerEvent;

use wayland::{ButtonState, PointerEvent as WlPointerEvent};

// Alternatively struct and into / conversions can be defined for a proper type MouseButton type
pub mod mouse_button {
    pub const LEFT: u32 = 272;
    pub const RIGHT: u32 = 273;
    pub const MIDDLE: u32 = 274;
    pub const BACK: u32 = 275;
    pub const FORWARD: u32 = 276;
}

#[derive(Debug, Default)]
pub struct PointerState {
    /// Last known surface-relative cursor X coordinate.
    pub x: f64,
    /// Last known surface-relative cursor Y coordinate.
    pub y: f64,
}

impl PointerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate one raw Wayland [`WlPointerEvent`] into a semantic [`PointerEvent`].
    pub(crate) fn process(&mut self, ev: &WlPointerEvent) -> Option<PointerEvent> {
        match ev {
            WlPointerEvent::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                self.x = *surface_x;
                self.y = *surface_y;
                Some(PointerEvent::Enter {
                    surface: *surface,
                    x: self.x,
                    y: self.y,
                })
            }

            WlPointerEvent::Leave { surface, .. } => Some(PointerEvent::Leave {
                surface: *surface,
                x: self.x,
                y: self.y,
            }),

            WlPointerEvent::Motion {
                time,
                surface_x,
                surface_y,
            } => {
                let dx = surface_x - self.x;
                let dy = surface_y - self.y;
                self.x = *surface_x;
                self.y = *surface_y;
                Some(PointerEvent::Move {
                    x: self.x,
                    y: self.y,
                    dx,
                    dy,
                    time: *time,
                })
            }

            WlPointerEvent::Button {
                button,
                state,
                time,
                ..
            } => match state {
                ButtonState::Pressed => Some(PointerEvent::ButtonPress {
                    button: *button,
                    x: self.x,
                    y: self.y,
                    time: *time,
                }),
                ButtonState::Released => Some(PointerEvent::ButtonRelease {
                    button: *button,
                    x: self.x,
                    y: self.y,
                    time: *time,
                }),
                _ => None,
            },

            WlPointerEvent::Axis { time, axis, value } => Some(PointerEvent::Scroll {
                axis: *axis,
                delta: *value,
                time: *time,
            }),

            WlPointerEvent::Frame => Some(PointerEvent::Frame),

            _ => None,
        }
    }
}
