use assets::{AtlasId, SpriteRegion};
use taffy::{AvailableSpace, Layout, Size, Style};
use utils::{Color, Point, Rect, Size as USize};

use crate::{Damage, Measure, OnChange, Render, RenderCommand};

/// A single monochrome sprite in an atlas: which atlas, and the region within
/// it. The `Icon` analogue of `Text`'s `&'static BakedFont`.
#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub atlas_id: AtlasId,
    pub region: SpriteRegion,
}

/// A monochrome sprite, tinted by `color`, natural-sized from its atlas region.
/// Mirrors [`Text`](super::Text): a measure widget whose content feeds taffy.
#[crate::widget(measure)]
#[derive(Clone)]
pub struct Icon {
    pub sprite: Option<Sprite>,
    pub color: Color,
}

impl Icon {
    pub fn new(style: Style) -> Self {
        Self {
            node_id: taffy::NodeId::new(u64::MAX),
            style,
            bounds: Rect::ZERO,
            pending_damage: Damage::None,
            is_opaque: true,
            sprite: None,
            color: Color::WHITE,
        }
    }

    pub fn placeholder() -> Self {
        Self::new(Style::default())
    }
}

impl OnChange<Option<Sprite>> for Icon {
    fn damage(&self, _new: &Option<Sprite>) -> Damage {
        Damage::Layout
    }
    fn change(&mut self, new: Option<Sprite>) {
        self.sprite = new;
    }
}

impl OnChange<Color> for Icon {
    fn damage(&self, _new: &Color) -> Damage {
        Damage::paint(self.bounds)
    }
    fn change(&mut self, new: Color) {
        self.color = new;
    }
}

impl Measure for Icon {
    fn measure(
        &self,
        _known_dimensions: Size<Option<f32>>,
        _available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        match self.sprite {
            None => Size::ZERO,
            Some(s) => Size {
                width: s.region.w,
                height: s.region.h,
            },
        }
    }
}

impl Render for Icon {
    fn render(&self, layout: &Layout, abs_pos: Point) -> Vec<RenderCommand> {
        let Some(sprite) = self.sprite else {
            return vec![];
        };
        // `z`, `background`, and `is_opaque` are stamped by the render walk.
        vec![RenderCommand::DrawMonochromeSprite {
            atlas_id: sprite.atlas_id,
            region: sprite.region,
            origin: abs_pos,
            z: 0.0,
            size: USize::new(layout.size.width, layout.size.height),
            color: self.color,
            background: Color::TRANSPARENT,
            is_opaque: true,
        }]
    }
}
