use taffy::NodeId;
use utils::{Color, Point, Rect, Size as USize};

use crate::{Damage, EventCtx, OnChange, RenderCommand, WidgetList, WidgetTree};

pub struct DebugDamage<Inner> {
    inner: Inner,
    frame: usize,
}

impl<Inner> DebugDamage<Inner> {
    pub fn new(inner: Inner) -> Self {
        Self { inner, frame: 0 }
    }
}

const PALETTE: [Color; 6] = [
    Color::from_rgba8(230, 60, 60, 0.25),
    Color::from_rgba8(70, 200, 90, 0.25),
    Color::from_rgba8(60, 120, 230, 0.25),
    Color::from_rgba8(230, 190, 60, 0.25),
    Color::from_rgba8(200, 70, 210, 0.25),
    Color::from_rgba8(60, 200, 210, 0.25),
];

const TINT_Z: f32 = 1.0;
const COVER: f32 = 16384.0;

impl<Inner: WidgetList> WidgetList for DebugDamage<Inner> {
    fn build_children(&mut self, tree: &mut WidgetTree) -> Vec<NodeId> {
        self.inner.build_children(tree)
    }

    fn render_children(&mut self, tree: &WidgetTree, parent_abs: Point) -> Vec<RenderCommand> {
        let mut commands = self.inner.render_children(tree, parent_abs);

        let color = PALETTE[self.frame % PALETTE.len()];
        self.frame = self.frame.wrapping_add(1);

        commands.push(RenderCommand::DrawQuad {
            color,
            border_color: Color::TRANSPARENT,
            origin: Point::ZERO,
            z: TINT_Z,
            size: USize::new(COVER, COVER),
            border_radius: 0.0,
            border_thickness: 0.0,
        });
        commands
    }

    fn bounds(&self) -> Rect {
        self.inner.bounds()
    }

    fn on_event(&mut self, ctx: &mut EventCtx) {
        self.inner.on_event(ctx);
    }

    fn flush_damage(&mut self, tree: &mut WidgetTree) -> Damage {
        self.inner.flush_damage(tree)
    }

    fn touch_config(&self) -> Option<interactivity::touch::TouchConfig> {
        self.inner.touch_config()
    }

    fn gesture_config(&self) -> Option<interactivity::gesture::GestureConfig> {
        self.inner.gesture_config()
    }

    fn wants_input(&self) -> bool {
        self.inner.wants_input()
    }
}

impl<Inner, U> OnChange<U> for DebugDamage<Inner>
where
    Inner: OnChange<U>,
{
    fn damage(&self, new: &U) -> Damage {
        self.inner.damage(new)
    }

    fn change(&mut self, new: U) {
        self.inner.change(new);
    }
}
