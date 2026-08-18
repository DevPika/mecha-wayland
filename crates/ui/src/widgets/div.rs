use taffy::{Layout, Style};
use utils::{Color, Point, Rect, Size as USize};

use crate::{Damage, OnChange, Render, RenderCommand, WidgetList};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderColor(pub Color);

#[crate::widget]
pub struct Div<T: WidgetList> {
    pub color: Color,
    pub border_color: BorderColor,
    pub border_radius: f32,
    pub border_thickness: f32,
    pub z: f32,
    #[widget(child)]
    pub children: T,
}

impl<T: WidgetList> Div<T> {
    pub fn new(style: Style, children: T) -> Self {
        Self {
            node_id: taffy::NodeId::new(u64::MAX),
            style,
            bounds: Rect::ZERO,
            pending_damage: Damage::None,
            color: Color::TRANSPARENT,
            border_color: BorderColor(Color::TRANSPARENT),
            border_radius: 0.0,
            border_thickness: 0.0,
            z: 0.0,
            children,
        }
    }
}

impl<T: WidgetList> OnChange<Color> for Div<T> {
    fn damage(&self, _new: &Color) -> Damage {
        Damage::paint(self.bounds)
    }
    fn change(&mut self, new: Color) {
        self.color = new;
    }
}

impl<T: WidgetList> OnChange<BorderColor> for Div<T> {
    fn damage(&self, _new: &BorderColor) -> Damage {
        Damage::paint(self.bounds)
    }
    fn change(&mut self, new: BorderColor) {
        self.border_color = new;
    }
}

impl<T: WidgetList> Render for Div<T> {
    fn render(&self, layout: &Layout, abs_pos: Point) -> Vec<RenderCommand> {
        vec![RenderCommand::DrawQuad {
            color: self.color,
            border_color: self.border_color.0,
            origin: abs_pos,
            z: self.z,
            size: USize::new(layout.size.width, layout.size.height),
            border_radius: self.border_radius,
            border_thickness: self.border_thickness,
        }]
    }
}
