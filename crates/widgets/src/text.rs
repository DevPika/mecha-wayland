//! `Text`: one line of glyphs, shaped once at build and re-shaped by its
//! `Context` setters. Single-line only.

use app::{Build, Context, Handle, Spawner, Widget};
use atlas::{Atlas, FontId};
use geometry::{Color, Point, Size};
use layout::{LayoutStyle, Measure};
use paint::{MonochromeSprite, Paint, PaintContext};

pub struct Text {
    font: FontId,
    px: u16,
    color: Color,
    string: String,
}

impl Text {
    pub fn text(&self) -> &str {
        &self.string
    }
    pub fn font(&self) -> FontId {
        self.font
    }
    pub fn size(&self) -> u16 {
        self.px
    }
    pub fn color(&self) -> Color {
        self.color
    }
}

pub fn text(font: FontId, s: impl Into<String>) -> TextBuilder {
    TextBuilder {
        font,
        string: s.into(),
        style: LayoutStyle::default(),
        px: 16,
        color: Color::WHITE,
    }
}

pub struct TextBuilder {
    font: FontId,
    string: String,
    style: LayoutStyle,
    px: u16,
    color: Color,
}

impl TextBuilder {
    pub fn style(mut self, style: LayoutStyle) -> Self {
        self.style = style;
        self
    }
    pub fn size(mut self, px: u16) -> Self {
        self.px = px;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Build for TextBuilder {
    type Widget = Text;
}

impl Widget for Text {
    type Builder = TextBuilder;
    fn build(b: TextBuilder, me: Handle<Text>, s: &mut Spawner<'_, Text>) -> Text {
        *s.component_mut::<LayoutStyle>(me).unwrap() = b.style;
        let (sprites, size) = {
            let mut atlas = s.resource_mut::<Atlas>();
            shape(&mut atlas, b.font, b.px, &b.string, b.color)
        };
        *s.component_mut::<Paint>(me).unwrap() = Paint::Monochrome(sprites);
        *s.component_mut::<Measure>(me).unwrap() = Measure::fixed(size);
        Text {
            font: b.font,
            px: b.px,
            color: b.color,
            string: b.string,
        }
    }
}

/// The byte offset of the glyph boundary closest to `x` pixels into
/// `text`: where a click at `x` lands the cursor. `0` for an empty
/// string or `x` at or before the first glyph's midpoint; `text.len()`
/// for `x` past the end. Advances match `shape`'s (no kerning), so the
/// boundary lines up with a drawn glyph's edge.
pub fn hit_position(atlas: &mut Atlas, font: FontId, px: u16, text: &str, x: f32) -> usize {
    let mut pen = 0.0f32;
    let mut best = 0usize;
    let mut best_d = x.abs(); // distance from the boundary at pen = 0
    for (i, ch) in text.char_indices() {
        let Some((f, id)) = atlas.lookup(&[font], ch) else {
            continue;
        };
        pen += atlas.glyph(f, id, px).advance;
        let off = i + ch.len_utf8();
        let d = (pen - x).abs();
        if d < best_d {
            best_d = d;
            best = off;
        }
    }
    best
}

/// The pen advance of `text` in `font` at `px`: how far the pen moves
/// left to right, which is the width a `Text` of it reports. A character
/// missing from the font's cmap contributes no width, and one present
/// but with no ink (such as a space) still advances the pen. Sums the
/// same advances `shape` does, without building sprites.
pub fn measure(atlas: &mut Atlas, font: FontId, px: u16, text: &str) -> f32 {
    let mut pen = 0.0f32;
    let mut last: Option<u16> = None;
    for ch in text.chars() {
        let Some((f, id)) = atlas.lookup(&[font], ch) else {
            continue;
        };
        if let Some(left) = last {
            pen += atlas.kern(font, left, id, px);
        }
        pen += atlas.glyph(f, id, px).advance;
        last = Some(id);
    }
    pen
}

/// One line of `text` in `font` at `px`, tinted `color`: a sprite per
/// glyph with ink, pen-advanced left to right, baselined by the font's
/// ascent, and the line's natural size (`pen` wide, `ascent - descent +
/// gap` tall). A character missing from the font's cmap is skipped
/// entirely — it contributes no width and no sprite, as if it were never
/// in the string. A character present in the cmap but with no ink (such
/// as a space) still advances the pen; only the sprite is skipped.
fn shape(
    atlas: &mut Atlas,
    font: FontId,
    px: u16,
    text: &str,
    color: Color,
) -> (Vec<MonochromeSprite>, Size) {
    let line = atlas.line(font, px);
    let baseline = line.ascent;
    let mut pen = 0.0f32;
    let mut sprites = Vec::new();
    for ch in text.chars() {
        let Some((f, id)) = atlas.lookup(&[font], ch) else {
            continue;
        };
        let g = atlas.glyph(f, id, px);
        if g.tile.bounds.width() > 0.0 {
            sprites.push(MonochromeSprite::new(
                g.tile,
                Point::new(pen + g.left, baseline - g.top),
                Size::new(g.tile.bounds.width(), g.tile.bounds.height()),
                color,
            ));
        }
        pen += g.advance;
    }
    let height = line.ascent - line.descent + line.gap;
    (sprites, Size::new(pen, height))
}

pub trait TextContext {
    fn set_text(&mut self, text: impl Into<String>);
    fn set_size(&mut self, px: u16);
    fn set_color(&mut self, color: Color);
}

impl TextContext for Context<'_, Text> {
    fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        let (font, px, color) = {
            let w = self.me();
            (w.font, w.px, w.color)
        };
        let (sprites, size) = {
            let mut atlas = self.resource_mut::<Atlas>();
            shape(&mut atlas, font, px, &text, color)
        };
        self.set_paint(Paint::Monochrome(sprites));
        *self.component_mut::<Measure>().unwrap() = Measure::fixed(size);
        self.me().string = text;
    }

    fn set_size(&mut self, px: u16) {
        let (font, color, string) = {
            let w = self.me();
            (w.font, w.color, w.string.clone())
        };
        let (sprites, size) = {
            let mut atlas = self.resource_mut::<Atlas>();
            shape(&mut atlas, font, px, &string, color)
        };
        self.set_paint(Paint::Monochrome(sprites));
        *self.component_mut::<Measure>().unwrap() = Measure::fixed(size);
        self.me().px = px;
    }

    fn set_color(&mut self, color: Color) {
        let mut paint = self.paint().clone();
        if let Paint::Monochrome(sprites) = &mut paint {
            for sprite in sprites.iter_mut() {
                sprite.color = color;
            }
        }
        self.set_paint(paint);
        self.me().color = color;
    }
}

#[cfg(test)]
mod tests {
    use atlas::Atlas;
    use geometry::Color;

    use super::*;

    const INTER: &[u8] = include_bytes!("../../atlas/tests/fixtures/Inter-Regular.ttf");

    fn inter() -> (Atlas, FontId) {
        let mut atlas = Atlas::new();
        let font = atlas.add_font(INTER).unwrap();
        (atlas, font)
    }

    #[test]
    fn shape_places_one_sprite_per_glyph_with_ink() {
        let (mut atlas, font) = inter();
        let (sprites, size) = shape(&mut atlas, font, 14, "ab", Color::WHITE);
        assert_eq!(sprites.len(), 2);
        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
        assert_eq!(sprites[0].color, Color::WHITE);
    }

    #[test]
    fn shape_skips_a_space_but_still_advances_the_pen() {
        let (mut atlas, font) = inter();
        let (with_space, wide) = shape(&mut atlas, font, 14, "a b", Color::WHITE);
        let (without_space, narrow) = shape(&mut atlas, font, 14, "ab", Color::WHITE);
        assert_eq!(with_space.len(), 2, "the space has no ink");
        assert_eq!(without_space.len(), 2);
        assert!(
            wide.width > narrow.width,
            "the space's advance still counts"
        );
    }

    #[test]
    fn shape_of_empty_text_is_a_zero_width_run_at_the_line_height() {
        let (mut atlas, font) = inter();
        let (sprites, size) = shape(&mut atlas, font, 14, "", Color::WHITE);
        assert!(sprites.is_empty());
        assert_eq!(size.width, 0.0);
        assert!(
            size.height > 0.0,
            "the line height does not depend on content"
        );
    }

    #[test]
    fn shape_skips_a_character_missing_from_the_cmap_and_contributes_no_width() {
        let (mut atlas, font) = inter();
        // U+4E2D ('中') is not in Inter-Regular's cmap.
        assert!(
            atlas.lookup(&[font], '\u{4e2d}').is_none(),
            "test assumes Inter-Regular has no glyph for U+4E2D"
        );
        let (with_missing, wider) = shape(&mut atlas, font, 14, "a\u{4e2d}b", Color::WHITE);
        let (_, narrower) = shape(&mut atlas, font, 14, "ab", Color::WHITE);
        assert_eq!(
            with_missing.len(),
            2,
            "the missing character paints no sprite"
        );
        assert_eq!(
            wider.width, narrower.width,
            "a character missing from the cmap contributes zero width, unlike a space"
        );
    }

    #[test]
    fn builder_verbs_set_the_right_fields() {
        let (_, font) = inter();
        let b = text(font, "hi").size(24).color(Color::BLACK);
        assert_eq!(b.string, "hi");
        assert_eq!(b.px, 24);
        assert_eq!(b.color, Color::BLACK);
        assert_eq!(b.font, font);
    }
}
