use utils::{Color, Point, Rect, Size};

use crate::{
    commands::{Command, CommandQueueRegistry, opaque::OpaquePrim, translucent::TranslucentPrim},
    texture::TextureId,
};

#[derive(Clone)]
pub struct DrawMonochromeSprite {
    pub texture_id: TextureId,
    pub region: Rect,
    pub origin: Point,
    pub z: f32,
    pub size: Size,
    pub color: Color,
    pub background: Color,
    /// Author opt-out of the opaque fast path (default `true` at the UI). The
    /// path is taken only when this holds *and* `background` is opaque.
    pub is_opaque: bool,
}

impl Command for DrawMonochromeSprite {
    fn record(self, registry: &mut CommandQueueRegistry) {
        if self.is_opaque && self.background.a >= 1.0 {
            registry.opaque.push_sprite(OpaquePrim {
                origin: self.origin,
                z: self.z,
                size: self.size,
                fg: self.color,
                bg: self.background,
                region: self.region,
                texture_id: Some(self.texture_id),
            });
        } else if self.color.a > 0.0 {
            // A fully-transparent tint blends nothing — skip it entirely.
            registry.translucent.push(TranslucentPrim {
                origin: self.origin,
                z: self.z,
                size: self.size,
                color: self.color,
                // No border for sprites; equal border colour keeps the SDF
                // border mix a no-op (border_thickness is 0 anyway).
                border_color: self.color,
                border_radius: 0.0,
                border_thickness: 0.0,
                region: self.region,
                texture_id: Some(self.texture_id),
                seq: 0,
            });
        }
    }
}
