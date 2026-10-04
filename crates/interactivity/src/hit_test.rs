//! The hit test: every node under a window whose `ComputedLayout` contains a
//! position, in dispatch order.

use app::{App, NodeId};
use geometry::Point;
use layout::ComputedLayout;

use crate::contacts::HitSet;

/// Every node under `window` (`window` included) whose `ComputedLayout.rect`
/// contains `position`. A preorder walk collects the matches ancestor
/// first, then the list is reversed: deepest/frontmost first, `window`
/// last. That lines up with `render`'s rule that a later preorder index
/// paints nearer, without this crate ever reading `render`'s `Scene`.
///
/// A node whose `ComputedLayout` was never written by `LayoutModule` is
/// `Rect::default()`, which is zero-sized and so never matches; no
/// separate filter is needed for it.
pub(crate) fn hit_test(app: &App, window: NodeId, position: Point) -> HitSet {
    let contains = |id: NodeId| {
        app.component::<ComputedLayout>(id)
            .is_some_and(|l| l.rect.contains(position))
    };
    let mut matches = HitSet::new();
    if contains(window) {
        matches.push(window);
    }
    for id in app.tree().descendants(window) {
        if contains(id) {
            matches.push(id);
        }
    }
    matches.reverse();
    matches
}

#[cfg(test)]
mod tests {
    use app::prelude::*;
    use geometry::Insets;
    use layout::prelude::*;
    use window::prelude::*;

    use super::*;

    struct Leaf;
    impl Build for Leaf {
        type Widget = Leaf;
    }
    impl Widget for Leaf {
        type Builder = Leaf;
        fn build(b: Leaf, _: Handle<Self>, _: &mut Spawner<'_, Self>) -> Self {
            b
        }
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_module(LayoutModule).add_module(WindowModule);
        app
    }

    /// An absolutely positioned box at `(x, y)`, `w` by `h`, relative to
    /// its containing block.
    fn at(x: f32, y: f32, w: f32, h: f32) -> LayoutStyle {
        LayoutStyle::default()
            .absolute()
            .inset(Insets::new(px(y), auto(), auto(), px(x)))
            .size(px(w), px(h))
    }

    /// A 200x200 window; a 120x120 card absolutely positioned at (20, 20);
    /// a 40x40 button, the card's one flow child, so it sits at the
    /// card's content-box origin — window-relative rect (20, 20, 40, 40).
    fn scene() -> (App, NodeId, NodeId, NodeId) {
        let mut app = app();
        let win = app.spawn(
            app.root(),
            window().layout(LayoutStyle::default().size(px(200.0), px(200.0))),
        );
        let card = app.spawn_with(win, Leaf, (at(20.0, 20.0, 120.0, 120.0),));
        let button = app.spawn_with(
            card,
            Leaf,
            (LayoutStyle::default().size(px(40.0), px(40.0)),),
        );
        app.tick();
        (app, win.id(), card.id(), button.id())
    }

    #[test]
    fn matches_every_nesting_level_deepest_first_window_last() {
        let (app, win, card, button) = scene();
        assert_eq!(
            hit_test(&app, win, Point::new(30.0, 30.0)).into_vec(),
            vec![button, card, win]
        );
    }

    #[test]
    fn a_point_inside_the_card_but_outside_the_button_skips_the_button() {
        let (app, win, card, _) = scene();
        assert_eq!(
            hit_test(&app, win, Point::new(100.0, 100.0)).into_vec(),
            vec![card, win]
        );
    }

    #[test]
    fn a_point_outside_the_card_matches_only_the_window() {
        let (app, win, ..) = scene();
        assert_eq!(
            hit_test(&app, win, Point::new(150.0, 150.0)).into_vec(),
            vec![win]
        );
    }

    #[test]
    fn the_half_open_rule_holds_at_a_nested_edge() {
        let (app, win, card, button) = scene();
        // the button's rect is (20, 20, 40, 40); its right/bottom edge is 60.
        assert_eq!(
            hit_test(&app, win, Point::new(59.9, 59.9)).into_vec(),
            vec![button, card, win]
        );
        assert_eq!(
            hit_test(&app, win, Point::new(60.0, 59.9)).into_vec(),
            vec![card, win],
            "the right edge is excluded"
        );
        assert_eq!(
            hit_test(&app, win, Point::new(20.0, 20.0)).into_vec(),
            vec![button, card, win],
            "the top-left corner is included"
        );
    }

    #[test]
    fn a_point_outside_the_window_matches_nothing() {
        let (app, win, ..) = scene();
        assert!(hit_test(&app, win, Point::new(250.0, 250.0)).is_empty());
    }
}
