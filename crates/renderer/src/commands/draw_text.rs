use assets::BakedFont;
use utils::{Color, Point, Rect, Size};

use crate::{
    commands::{Command, CommandQueueRegistry, DrawMonochromeSprite},
    texture::TextureId,
};

#[derive(Clone)]
pub struct DrawText {
    pub font: &'static BakedFont,
    pub texture_id: TextureId,
    pub text: String,
    pub origin: Point,
    pub z: f32,
    pub color: Color,
    pub background: Color,
    /// Author opt-out of the opaque fast path; forwarded to each glyph sprite.
    pub is_opaque: bool,
}

impl Command for DrawText {
    fn record(self, registry: &mut CommandQueueRegistry) {
        let mut pen_x = self.origin.x();
        let baseline_y = self.origin.y();

        for ch in self.text.chars() {
            let byte = ch as u32;
            if byte < 32 || byte > 126 {
                continue;
            }
            let glyph = &self.font.glyphs[(byte - 32) as usize];
            if glyph.w > 0.0 && glyph.h > 0.0 {
                DrawMonochromeSprite {
                    texture_id: self.texture_id,
                    region: Rect::xywh(glyph.x, glyph.y, glyph.w, glyph.h),
                    origin: Point::new(
                        pen_x + glyph.bearing_x,
                        baseline_y - glyph.bearing_y - glyph.h,
                    ),
                    z: self.z,
                    size: Size::new(glyph.w, glyph.h),
                    color: self.color,
                    background: self.background,
                    is_opaque: self.is_opaque,
                }
                .record(registry);
            }
            pen_x += glyph.advance;
        }
    }
}
