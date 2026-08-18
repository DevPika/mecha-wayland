extern crate self as ui;

use assets::BakedFont;
use interactivity::InteractivityState;
use std::any::Any;
use taffy::{AvailableSpace, Layout, NodeId, Size, Style, TaffyTree};
use utils::{Color, Rect, Size as USize};

pub use utils::Point;

pub use ui_macro::register_events;
pub use ui_macro::widget;

pub mod widgets;

pub type WidgetTree = TaffyTree<Box<dyn Measure>>;

pub fn compute_layout(tree: &mut WidgetTree, node: NodeId, available_space: Size<AvailableSpace>) {
    tree.compute_layout_with_measure(
        node,
        available_space,
        |known_dims, avail, _node_id, ctx, _style| {
            ctx.map_or(Size::ZERO, |m| m.measure(known_dims, avail))
        },
    )
    .unwrap();
}

pub enum RenderCommand {
    DrawQuad {
        color: Color,
        border_color: Color,
        origin: Point,
        z: f32,
        size: USize,
        border_radius: f32,
        border_thickness: f32,
    },
    DrawText {
        font: &'static BakedFont,
        text: String,
        origin: Point,
        z: f32,
        color: Color,
    },
    DrawMonochromeSprite {
        atlas_id: assets::AtlasId,
        region: assets::SpriteRegion,
        origin: Point,
        z: f32,
        size: USize,
        color: Color,
    },
    RegisterHitArea {
        id: u64,
        rect: Rect,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Damage {
    #[default]
    None,
    Paint(Rect),
    Layout,
}

impl Damage {
    #[inline]
    pub fn paint(rect: Rect) -> Damage {
        if rect.is_empty() {
            Damage::None
        } else {
            Damage::Paint(rect)
        }
    }

    #[inline]
    pub fn union(self, other: Damage) -> Damage {
        match (self, other) {
            (Damage::Layout, _) | (_, Damage::Layout) => Damage::Layout,
            (Damage::None, d) | (d, Damage::None) => d,
            (Damage::Paint(a), Damage::Paint(b)) => Damage::paint(a.union(b)),
        }
    }
}

impl std::ops::BitOr for Damage {
    type Output = Damage;
    #[inline]
    fn bitor(self, rhs: Damage) -> Damage {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for Damage {
    #[inline]
    fn bitor_assign(&mut self, rhs: Damage) {
        *self = *self | rhs;
    }
}

/// `damage` runs **first**, while the widget still holds its *old* state and is
/// handed the *new* value, so it can return a tight bound. `change` runs
/// **second** and is `&mut self` only: it applies the value and fans out the
/// *model* consequences — it never touches taffy or the renderer.
pub trait OnChange<T> {
    /// Region dirtied by replacing the current state with `new`. Runs before
    /// [`OnChange::change`]; must not mutate.
    fn damage(&self, new: &T) -> Damage;

    /// Apply `new`, mutating fields and fanning the value out to the model.
    fn change(&mut self, new: T);
}

pub trait Measure {
    fn measure(
        &self,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
    ) -> Size<f32>;
}

pub struct EventCtx<'a> {
    interactivity: &'a InteractivityState,
    tree: &'a mut WidgetTree,
    buffer: &'a mut Vec<Box<dyn Any>>,
}

impl<'a> EventCtx<'a> {
    pub fn new(
        interactivity: &'a InteractivityState,
        tree: &'a mut WidgetTree,
        buffer: &'a mut Vec<Box<dyn Any>>,
    ) -> Self {
        Self {
            interactivity,
            tree,
            buffer,
        }
    }

    pub fn interactivity(&self) -> &'a InteractivityState {
        self.interactivity
    }

    pub fn tree(&mut self) -> &mut WidgetTree {
        self.tree
    }

    pub fn dispatch<T: app::Event>(&mut self, event: T) {
        self.buffer.push(Box::new(event));
    }
}

pub trait Render {
    fn render(&self, layout: &Layout, abs_pos: Point) -> Vec<RenderCommand>;
}

pub trait Widget: Render {
    fn node_id(&self) -> NodeId;
    fn style(&self) -> &Style;
    fn own_bounds(&self) -> Rect;
    fn build_tree(&mut self, tree: &mut WidgetTree) -> NodeId;
    fn render_node(
        &mut self,
        layout: &Layout,
        tree: &WidgetTree,
        offset: Point,
    ) -> Vec<RenderCommand>;
    fn on_event(&mut self, _ctx: &mut EventCtx) {}
    fn drain_damage(&mut self, _tree: &mut WidgetTree) -> Damage {
        Damage::None
    }
}

pub trait WidgetList {
    fn build_children(&mut self, tree: &mut WidgetTree) -> Vec<NodeId>;
    fn render_children(&mut self, tree: &WidgetTree, parent_abs: Point) -> Vec<RenderCommand>;
    fn bounds(&self) -> Rect {
        Rect::ZERO
    }
    fn on_event(&mut self, _ctx: &mut EventCtx) {}
    fn flush_damage(&mut self, _tree: &mut WidgetTree) -> Damage {
        Damage::None
    }
    fn touch_config(&self) -> Option<interactivity::touch::TouchConfig> {
        None
    }
    fn gesture_config(&self) -> Option<interactivity::gesture::GestureConfig> {
        None
    }
    fn wants_input(&self) -> bool {
        true
    }
}

impl WidgetList for () {
    fn build_children(&mut self, _: &mut WidgetTree) -> Vec<NodeId> {
        vec![]
    }
    fn render_children(&mut self, _: &WidgetTree, _: Point) -> Vec<RenderCommand> {
        vec![]
    }
}

impl<W: Widget> WidgetList for W {
    fn build_children(&mut self, tree: &mut WidgetTree) -> Vec<NodeId> {
        vec![self.build_tree(tree)]
    }

    fn render_children(&mut self, tree: &WidgetTree, parent_abs: Point) -> Vec<RenderCommand> {
        let layout = tree.layout(self.node_id()).unwrap();
        self.render_node(layout, tree, parent_abs)
    }

    fn bounds(&self) -> Rect {
        <W as Widget>::own_bounds(self)
    }

    fn on_event(&mut self, ctx: &mut EventCtx) {
        <W as Widget>::on_event(self, ctx)
    }

    fn flush_damage(&mut self, tree: &mut WidgetTree) -> Damage {
        <W as Widget>::drain_damage(self, tree)
    }
}

impl<A: WidgetList> WidgetList for (A,) {
    fn build_children(&mut self, tree: &mut WidgetTree) -> Vec<NodeId> {
        self.0.build_children(tree)
    }

    fn render_children(&mut self, tree: &WidgetTree, parent_abs: Point) -> Vec<RenderCommand> {
        self.0.render_children(tree, parent_abs)
    }

    fn bounds(&self) -> Rect {
        self.0.bounds()
    }

    fn on_event(&mut self, ctx: &mut EventCtx) {
        self.0.on_event(ctx)
    }

    fn flush_damage(&mut self, tree: &mut WidgetTree) -> Damage {
        self.0.flush_damage(tree)
    }
}

impl<A: WidgetList, B: WidgetList> WidgetList for (A, B) {
    fn build_children(&mut self, tree: &mut WidgetTree) -> Vec<NodeId> {
        let mut ids = self.0.build_children(tree);
        ids.extend(self.1.build_children(tree));
        ids
    }

    fn render_children(&mut self, tree: &WidgetTree, parent_abs: Point) -> Vec<RenderCommand> {
        let mut commands = self.0.render_children(tree, parent_abs);
        commands.extend(self.1.render_children(tree, parent_abs));
        commands
    }

    fn bounds(&self) -> Rect {
        self.0.bounds().union(self.1.bounds())
    }

    fn on_event(&mut self, ctx: &mut EventCtx) {
        self.0.on_event(ctx);
        self.1.on_event(ctx);
    }

    fn flush_damage(&mut self, tree: &mut WidgetTree) -> Damage {
        self.0.flush_damage(tree) | self.1.flush_damage(tree)
    }
}

impl<A: WidgetList, B: WidgetList, C: WidgetList> WidgetList for (A, B, C) {
    fn build_children(&mut self, tree: &mut WidgetTree) -> Vec<NodeId> {
        let mut ids = self.0.build_children(tree);
        ids.extend(self.1.build_children(tree));
        ids.extend(self.2.build_children(tree));
        ids
    }

    fn render_children(&mut self, tree: &WidgetTree, parent_abs: Point) -> Vec<RenderCommand> {
        let mut commands = self.0.render_children(tree, parent_abs);
        commands.extend(self.1.render_children(tree, parent_abs));
        commands.extend(self.2.render_children(tree, parent_abs));
        commands
    }

    fn bounds(&self) -> Rect {
        self.0
            .bounds()
            .union(self.1.bounds())
            .union(self.2.bounds())
    }

    fn on_event(&mut self, ctx: &mut EventCtx) {
        self.0.on_event(ctx);
        self.1.on_event(ctx);
        self.2.on_event(ctx);
    }

    fn flush_damage(&mut self, tree: &mut WidgetTree) -> Damage {
        self.0.flush_damage(tree) | self.1.flush_damage(tree) | self.2.flush_damage(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::{Damage, OnChange};
    use utils::{Color, Rect};

    // ── Rect::union ─────────────────────────────────────────────────────────

    #[test]
    fn rect_union_bounding_box_of_disjoint_rects() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, 30.0, 5.0, 5.0);
        // min origin (0,0), far corner max(10,25)=25 x max(10,35)=35.
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 25.0, 35.0));
    }

    #[test]
    fn rect_union_empty_is_identity() {
        let r = Rect::new(40.0, 40.0, 10.0, 10.0);
        // ZERO at the origin must NOT smear the bound toward (0,0).
        assert_eq!(r.union(Rect::ZERO), r);
        assert_eq!(Rect::ZERO.union(r), r);
        // A zero-*width* sliver is empty too.
        let sliver = Rect::new(0.0, 0.0, 0.0, 200.0);
        assert_eq!(r.union(sliver), r);
        assert_eq!(sliver.union(r), r);
    }

    #[test]
    fn rect_union_is_commutative() {
        let a = Rect::new(1.0, 2.0, 3.0, 4.0);
        let b = Rect::new(-5.0, 0.0, 2.0, 20.0);
        assert_eq!(a.union(b), b.union(a));
    }

    #[test]
    fn rect_union_contained_rect_is_absorbed() {
        let outer = Rect::new(0.0, 0.0, 100.0, 100.0);
        let inner = Rect::new(10.0, 10.0, 5.0, 5.0);
        assert_eq!(outer.union(inner), outer);
        assert_eq!(inner.union(outer), outer);
    }

    // ── Damage::union laws ──────────────────────────────────────────────────

    fn sample_paint() -> Damage {
        Damage::Paint(Rect::new(10.0, 10.0, 5.0, 5.0))
    }

    #[test]
    fn damage_none_is_identity() {
        for x in [Damage::None, sample_paint(), Damage::Layout] {
            assert_eq!(Damage::None.union(x), x);
            assert_eq!(x.union(Damage::None), x);
        }
    }

    #[test]
    fn damage_union_is_commutative() {
        let variants = [Damage::None, sample_paint(), Damage::Layout];
        for a in variants {
            for b in variants {
                assert_eq!(a.union(b), b.union(a));
            }
        }
    }

    #[test]
    fn damage_paint_union_paint_is_rect_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, 20.0, 5.0, 5.0);
        assert_eq!(
            Damage::Paint(a).union(Damage::Paint(b)),
            Damage::Paint(a.union(b)),
        );
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 25.0, 25.0));
    }

    #[test]
    fn damage_layout_dominates() {
        for x in [Damage::None, sample_paint(), Damage::Layout] {
            assert_eq!(Damage::Layout.union(x), Damage::Layout);
            assert_eq!(x.union(Damage::Layout), Damage::Layout);
        }
    }

    #[test]
    fn damage_zero_area_collapses_to_none() {
        assert_eq!(Damage::paint(Rect::ZERO), Damage::None);
        assert_eq!(Damage::paint(Rect::new(5.0, 5.0, 0.0, 10.0)), Damage::None);
        // Empty Paint unioned with a real paint contributes nothing.
        let p = sample_paint();
        assert_eq!(Damage::Paint(Rect::ZERO).union(p), p);
    }

    #[test]
    fn damage_union_is_idempotent() {
        for x in [Damage::None, sample_paint(), Damage::Layout] {
            assert_eq!(x.union(x), x);
        }
    }

    #[test]
    fn damage_bitor_matches_union() {
        let a = sample_paint();
        let b = Damage::Paint(Rect::new(50.0, 50.0, 2.0, 2.0));
        assert_eq!(a | b, a.union(b));
        let mut acc = Damage::None;
        acc |= a;
        acc |= b;
        assert_eq!(acc, a.union(b));
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct FakeStyle(f32);

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Fill(f32);

    struct TestWidget {
        rect: Rect,
        color: Color,
        layout: FakeStyle,
        fill: Fill,
    }

    // Paint-class: a colour swap repaints the widget's own box, never moves it.
    impl OnChange<Color> for TestWidget {
        fn damage(&self, _new: &Color) -> Damage {
            Damage::paint(self.rect)
        }
        fn change(&mut self, new: Color) {
            self.color = new;
        }
    }

    // Layout-class: a style change may move geometry, so it dominates.
    impl OnChange<FakeStyle> for TestWidget {
        fn damage(&self, _new: &FakeStyle) -> Damage {
            Damage::Layout
        }
        fn change(&mut self, new: FakeStyle) {
            self.layout = new;
        }
    }

    // Paint-class, but uses old-and-new: damage only the horizontal strip
    // between the old fill and the new fill within the widget's rect.
    impl OnChange<Fill> for TestWidget {
        fn damage(&self, new: &Fill) -> Damage {
            let old_w = self.rect.width() * self.fill.0;
            let new_w = self.rect.width() * new.0;
            let lo = old_w.min(new_w);
            let hi = old_w.max(new_w);
            Damage::paint(Rect::new(
                self.rect.x() + lo,
                self.rect.y(),
                hi - lo,
                self.rect.height(),
            ))
        }
        fn change(&mut self, new: Fill) {
            self.fill = new;
        }
    }

    fn widget() -> TestWidget {
        TestWidget {
            rect: Rect::new(100.0, 200.0, 40.0, 10.0),
            color: Color::BLACK,
            layout: FakeStyle(1.0),
            fill: Fill(0.5),
        }
    }

    #[test]
    fn color_change_is_paint_of_own_rect_and_mutates() {
        let mut w = widget();
        let new = Color::WHITE;
        assert_eq!(w.damage(&new), Damage::Paint(w.rect));
        w.change(new);
        assert_eq!(w.color, Color::WHITE);
    }

    #[test]
    fn style_change_is_layout_and_mutates() {
        let mut w = widget();
        let new = FakeStyle(2.0);
        assert_eq!(w.damage(&new), Damage::Layout);
        w.change(new);
        assert_eq!(w.layout, FakeStyle(2.0));
    }

    #[test]
    fn fill_change_damages_only_the_strip_between_and_mutates() {
        let mut w = widget(); // rect x=100 w=40, fill 0.5 -> old_w = 20
        let new = Fill(0.75); // new_w = 30 -> strip x=120 width=10
        assert_eq!(
            w.damage(&new),
            Damage::Paint(Rect::new(120.0, 200.0, 10.0, 10.0)),
        );
        w.change(new);
        assert_eq!(w.fill, Fill(0.75));
    }

    #[test]
    fn fill_no_op_change_is_none() {
        let w = widget(); // fill 0.5
        // Same value: strip is zero-width, so `Damage::paint` collapses to None.
        assert_eq!(w.damage(&Fill(0.5)), Damage::None);
    }
}
