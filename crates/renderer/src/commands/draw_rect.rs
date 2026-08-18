use utils::{Color, Point, Rect, Size};

use crate::commands::{Command, CommandQueueRegistry, opaque::OpaquePrim};

/// A solid, fully-opaque axis-aligned fill. Always drawn in the opaque pass,
/// where it writes depth (its whole point — feeding early-z). A *translucent*
/// flat fill is a radius-0 [`super::DrawQuad`], not a `DrawRect`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DrawRect {
    pub color: Color,  // r, g, b, a
    pub origin: Point, // x, y in pixels
    pub z: f32,        // depth
    pub size: Size,    // width, height in pixels
}

impl Command for DrawRect {
    fn record(self, registry: &mut CommandQueueRegistry) {
        registry.opaque.push_rect(OpaquePrim {
            origin: self.origin,
            z: self.z,
            size: self.size,
            fg: self.color,
            bg: self.color,
            region: Rect::ZERO,
            texture_id: None,
        });
    }
}
