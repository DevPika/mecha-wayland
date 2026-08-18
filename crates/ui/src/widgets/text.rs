use assets::BakedFont;
use taffy::{AvailableSpace, Layout, Size, Style};
use utils::{Color, Point, Rect};

use crate::{Damage, Measure, OnChange, Render, RenderCommand};

#[crate::widget(measure)]
#[derive(Clone)]
pub struct Text {
    pub font: Option<&'static BakedFont>,
    pub text: String,
    pub color: Color,
    pub z: f32,
}

impl Text {
    pub fn new(style: Style) -> Self {
        Self {
            node_id: taffy::NodeId::new(u64::MAX),
            style,
            bounds: Rect::ZERO,
            pending_damage: Damage::None,
            font: None,
            text: String::new(),
            color: Color::WHITE,
            z: 0.0,
        }
    }

    pub fn placeholder() -> Self {
        Self::new(Style::default())
    }
}

impl OnChange<String> for Text {
    fn damage(&self, _new: &String) -> Damage {
        Damage::Layout
    }
    fn change(&mut self, new: String) {
        self.text = new;
    }
}

impl OnChange<Option<&'static BakedFont>> for Text {
    fn damage(&self, _new: &Option<&'static BakedFont>) -> Damage {
        Damage::Layout
    }
    fn change(&mut self, new: Option<&'static BakedFont>) {
        self.font = new;
    }
}

impl Measure for Text {
    fn measure(
        &self,
        _known_dimensions: Size<Option<f32>>,
        _available_space: Size<AvailableSpace>,
    ) -> Size<f32> {
        match self.font {
            None => Size::ZERO,
            Some(font) => Size {
                width: font.measure_width(&self.text),
                height: font.line_height,
            },
        }
    }
}

impl Render for Text {
    fn render(&self, _layout: &Layout, abs_pos: Point) -> Vec<RenderCommand> {
        let Some(font) = self.font else { return vec![] };
        let origin = Point::new(abs_pos.x(), abs_pos.y() + font.ascent);
        vec![RenderCommand::DrawText {
            font,
            text: self.text.clone(),
            origin,
            z: self.z,
            color: self.color,
        }]
    }
}
