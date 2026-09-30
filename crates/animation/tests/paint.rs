use animation::{AnimatedPaint, AnimationModule};
use app::prelude::*;
use geometry::Color;
use layout::LayoutModule;
use paint::{Paint, PaintModule, Quad};
use window::WindowModule;

struct Leaf;
struct Painted(Paint);
impl Build for Painted {
    type Widget = Leaf;
}
impl Widget for Leaf {
    type Builder = Painted;
    fn build(b: Painted, me: Handle<Self>, spawner: &mut Spawner<'_, Self>) -> Self {
        *spawner.component_mut::<Paint>(me).unwrap() = b.0;
        Leaf
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(AnimationModule);
    app
}

#[test]
fn spawned_nodes_copy_their_final_paint_after_build() {
    let mut app = app();
    let quad = Paint::Quad(Quad::new(Color::WHITE));
    let first = app.spawn_with(app.root(), Painted(quad.clone()), (Paint::None,));
    let run = Paint::Monochrome(vec![]);
    let second = app.spawn_with(app.root(), Painted(run.clone()), (Paint::None,));
    let third = app.spawn(app.root(), Painted(Paint::None));
    app.flush();

    assert_eq!(
        app.component::<AnimatedPaint>(first),
        Some(&AnimatedPaint(quad))
    );
    assert_eq!(
        app.component::<AnimatedPaint>(second),
        Some(&AnimatedPaint(run))
    );
    assert_eq!(
        app.component::<AnimatedPaint>(third),
        Some(&AnimatedPaint(Paint::None))
    );
}

#[test]
fn removed_nodes_do_not_leak_displayed_paint_to_reused_slots() {
    let mut app = app();
    let first = app.spawn(app.root(), Painted(Paint::Quad(Quad::new(Color::WHITE))));
    app.flush();
    app.remove(first);
    let replacement = app.spawn(app.root(), Painted(Paint::None));
    assert_eq!(replacement.id().slot(), first.id().slot());
    app.flush();
    assert_eq!(
        app.component::<AnimatedPaint>(replacement),
        Some(&AnimatedPaint(Paint::None))
    );
}
