use utils::{Color, Point, Rect, Size};

use crate::commands::{
    Command, CommandQueueRegistry, opaque::OpaquePrim, translucent::TranslucentPrim,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DrawQuad {
    pub color: Color,
    pub border_color: Color,
    pub origin: Point,
    pub z: f32,
    pub size: Size,
    pub border_radius: f32,
    pub border_thickness: f32,
    /// Solid colour behind the quad, inferred by the UI render walk.
    pub background: Color,
    /// Author opt-out of the opaque fast path (default `true` at the UI).
    pub is_opaque: bool,
}

impl Command for DrawQuad {
    fn record(self, registry: &mut CommandQueueRegistry) {
        let fill_visible = self.color.a > 0.0;
        let border_visible = self.border_thickness > 0.0 && self.border_color.a > 0.0;
        if fill_visible || border_visible {
            registry.translucent.push(TranslucentPrim {
                origin: self.origin,
                z: self.z,
                size: self.size,
                color: self.color,
                border_color: self.border_color,
                border_radius: self.border_radius,
                border_thickness: self.border_thickness,
                region: Rect::ZERO,
                texture_id: None,
                seq: 0,
            });
        }

        // The flat interior goes opaque when it composites to a solid colour and
        // the author hasn't opted out. `over` collapses to `color` when the fill
        // is already opaque, so a natively-opaque quad behaves exactly as before.
        let interior = self.color.over(self.background);
        if self.is_opaque && interior.a >= 1.0 {
            let r = self.border_radius;
            let inner_w = self.size.width() - 2.0 * r;
            let inner_h = self.size.height() - 2.0 * r;
            if inner_w > 0.0 && inner_h > 0.0 {
                const Z_EPSILON: f32 = 1e-5;
                registry.opaque.push_rect(OpaquePrim {
                    origin: Point::new(self.origin.x() + r, self.origin.y() + r),
                    z: self.z + Z_EPSILON,
                    size: Size::new(inner_w, inner_h),
                    fg: interior,
                    bg: interior,
                    region: Rect::ZERO,
                    texture_id: None,
                });
            }
        }
    }
}
