//! `Image`: one full-colour sprite the caller already inserted into the
//! atlas.

use app::{Build, Context, Handle, Spawner, Widget};
use atlas::{Atlas, AtlasTile, Rescaled, SpriteId};
use geometry::Corners;
use layout::{LayoutStyle, Measure};
use paint::{Paint, PaintContext, PolychromeSprite};

pub struct Image {
    sprite: SpriteId,
    tile: AtlasTile,
    radii: Corners<f32>,
    opacity: f32,
    grayscale: bool,
}

impl Image {
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
    pub fn grayscale(&self) -> bool {
        self.grayscale
    }
}

pub fn image(sprite: SpriteId) -> ImageBuilder {
    ImageBuilder {
        sprite,
        style: LayoutStyle::default(),
        radii: Corners::all(0.0),
        opacity: 1.0,
        grayscale: false,
    }
}

pub struct ImageBuilder {
    sprite: SpriteId,
    style: LayoutStyle,
    radii: Corners<f32>,
    opacity: f32,
    grayscale: bool,
}

impl ImageBuilder {
    pub fn style(mut self, style: LayoutStyle) -> Self {
        self.style = style;
        self
    }
    pub fn radius(mut self, radius: f32) -> Self {
        self.radii = Corners::all(radius);
        self
    }
    pub fn radii(mut self, radii: Corners<f32>) -> Self {
        self.radii = radii;
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
    pub fn grayscale(mut self, grayscale: bool) -> Self {
        self.grayscale = grayscale;
        self
    }
}

impl Build for ImageBuilder {
    type Widget = Image;
}

impl Widget for Image {
    type Builder = ImageBuilder;
    fn build(b: ImageBuilder, me: Handle<Image>, s: &mut Spawner<'_, Image>) -> Image {
        *s.component_mut::<LayoutStyle>(me).unwrap() = b.style;
        let sprite = s.resource::<Atlas>().sprite(b.sprite);
        *s.component_mut::<Paint>(me).unwrap() = Paint::Polychrome(
            PolychromeSprite::new(sprite.tile)
                .radii(b.radii)
                .opacity(b.opacity)
                .grayscale(b.grayscale),
        );
        *s.component_mut::<Measure>(me).unwrap() = Measure::fixed(sprite.size);
        s.on::<Rescaled>(me, |ctx: &mut Context<'_, Image>, _| {
            let id = ctx.me().sprite;
            let tile = ctx.resource::<Atlas>().sprite(id).tile;
            let (radii, opacity, grayscale) = {
                let w = ctx.me();
                (w.radii, w.opacity, w.grayscale)
            };
            ctx.set_paint(Paint::Polychrome(
                PolychromeSprite::new(tile)
                    .radii(radii)
                    .opacity(opacity)
                    .grayscale(grayscale),
            ));
            ctx.me().tile = tile;
        });
        Image {
            sprite: b.sprite,
            tile: sprite.tile,
            radii: b.radii,
            opacity: b.opacity,
            grayscale: b.grayscale,
        }
    }
}

pub trait ImageContext {
    /// Swaps to a different already-inserted sprite; rewrites `Measure`
    /// to the new sprite's size, keeps radii, opacity and grayscale.
    fn set_sprite(&mut self, sprite: SpriteId);
    fn set_opacity(&mut self, opacity: f32);
    fn set_grayscale(&mut self, grayscale: bool);
    fn set_radius(&mut self, radius: f32);
    fn set_radii(&mut self, radii: Corners<f32>);
}

impl ImageContext for Context<'_, Image> {
    fn set_sprite(&mut self, id: SpriteId) {
        let sprite = self.resource::<Atlas>().sprite(id);
        if self.me().tile == sprite.tile {
            return;
        }
        let (radii, opacity, grayscale) = {
            let w = self.me();
            (w.radii, w.opacity, w.grayscale)
        };
        self.set_paint(Paint::Polychrome(
            PolychromeSprite::new(sprite.tile)
                .radii(radii)
                .opacity(opacity)
                .grayscale(grayscale),
        ));
        *self.component_mut::<Measure>().unwrap() = Measure::fixed(sprite.size);
        let w = self.me();
        w.sprite = id;
        w.tile = sprite.tile;
    }

    fn set_opacity(&mut self, opacity: f32) {
        if self.me().opacity == opacity {
            return;
        }
        let (tile, radii, grayscale) = {
            let w = self.me();
            (w.tile, w.radii, w.grayscale)
        };
        self.set_paint(Paint::Polychrome(
            PolychromeSprite::new(tile)
                .radii(radii)
                .opacity(opacity)
                .grayscale(grayscale),
        ));
        self.me().opacity = opacity;
    }

    fn set_grayscale(&mut self, grayscale: bool) {
        if self.me().grayscale == grayscale {
            return;
        }
        let (tile, radii, opacity) = {
            let w = self.me();
            (w.tile, w.radii, w.opacity)
        };
        self.set_paint(Paint::Polychrome(
            PolychromeSprite::new(tile)
                .radii(radii)
                .opacity(opacity)
                .grayscale(grayscale),
        ));
        self.me().grayscale = grayscale;
    }

    fn set_radius(&mut self, radius: f32) {
        self.set_radii(Corners::all(radius));
    }

    fn set_radii(&mut self, radii: Corners<f32>) {
        if self.me().radii == radii {
            return;
        }
        let (tile, opacity, grayscale) = {
            let w = self.me();
            (w.tile, w.opacity, w.grayscale)
        };
        self.set_paint(Paint::Polychrome(
            PolychromeSprite::new(tile)
                .radii(radii)
                .opacity(opacity)
                .grayscale(grayscale),
        ));
        self.me().radii = radii;
    }
}

#[cfg(test)]
mod tests {
    use layout::LayoutStyle;

    use super::*;

    #[test]
    fn builder_verbs_set_the_right_fields() {
        let b = image(SpriteId(0)).radius(3.0).opacity(0.5).grayscale(true);
        assert_eq!(b.sprite, SpriteId(0));
        assert_eq!(b.radii, Corners::all(3.0));
        assert_eq!(b.opacity, 0.5);
        assert!(b.grayscale);
    }

    #[test]
    fn radii_and_style_verbs_replace_their_whole_value() {
        let b = image(SpriteId(1)).radii(Corners::new(1.0, 2.0, 3.0, 4.0));
        assert_eq!(b.radii, Corners::new(1.0, 2.0, 3.0, 4.0));

        let style = LayoutStyle::default().column();
        let b = image(SpriteId(2)).style(style.clone());
        assert_eq!(b.style, style);
    }
}
