use utils::{Color, Point, Rect, Size};

use crate::commands::{
    Command, CommandQueueRegistry, opaque::OpaquePrim, translucent::TranslucentPrim,
};

/// A rounded, optionally-bordered rectangle. Always drawn in the translucent
/// pass (its edges and corners are anti-aliased). If fully opaque, its
/// axis-aligned interior is *also* emitted as an opaque rect so it writes depth.
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

        if self.color.a >= 1.0 {
            let r = self.border_radius;
            let inner_w = self.size.width() - 2.0 * r;
            let inner_h = self.size.height() - 2.0 * r;
            if inner_w > 0.0 && inner_h > 0.0 {
                const Z_EPSILON: f32 = 1e-5;
                registry.opaque.push_rect(OpaquePrim {
                    origin: Point::new(self.origin.x() + r, self.origin.y() + r),
                    z: self.z + Z_EPSILON,
                    size: Size::new(inner_w, inner_h),
                    fg: self.color,
                    bg: self.color,
                    region: Rect::ZERO,
                    texture_id: None,
                });
            }
        }
    }
}
