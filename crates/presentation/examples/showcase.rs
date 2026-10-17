//! One toplevel with an opaque panel, a rounded translucent card over it,
//! a bordered box, and a word in Inter, drawn by the GLES backend. Run
//! under a Wayland session: `cargo run -p presentation --example showcase`.

use animation::AnimationModule;
use app::prelude::*;
use atlas::prelude::*;
use geometry::{Color, Point, Size};
use gles::Budget;
use interactivity::prelude::*;
use layout::prelude::*;
use paint::prelude::*;
use presentation::prelude::*;
use render::prelude::*;
use ring::prelude::*;
use wayland::prelude::*;
use window::prelude::*;

struct Leaf;
impl Build for Leaf {
    type Widget = Leaf;
}
impl Widget for Leaf {
    type Builder = Leaf;
    fn build(b: Leaf, _me: Handle<Self>, _s: &mut Spawner<'_, Self>) -> Self {
        b
    }
}

/// Spawns the window and its content; stops the app on close.
struct Shell;
struct ShellBuilder {
    root: NodeId,
}
impl Build for ShellBuilder {
    type Widget = Shell;
}
impl Widget for Shell {
    type Builder = ShellBuilder;
    fn build(b: ShellBuilder, _me: Handle<Self>, s: &mut Spawner<'_, Self>) -> Self {
        let win = s.spawn(
            b.root,
            window()
                .title("showcase")
                .clear(Color::rgb(0.12, 0.12, 0.14))
                .layout(LayoutStyle::default().column().size(px(480.0), px(320.0))),
        );
        s.on::<CloseRequested>(win, |ctx, _| ctx.signal(Stop));

        // An opaque panel.
        let panel = s.spawn_with(
            win,
            Leaf,
            (
                LayoutStyle::default().column().size(px(440.0), px(200.0)),
                Paint::Quad(Quad::new(Color::rgb(0.2, 0.45, 0.8)).radius(12.0)),
            ),
        );
        s.on::<Press>(panel, |_, e| {
            eprintln!("panel: Press {:?} at {:?}", e.contact, e.position)
        });
        s.on::<Release>(panel, |_, e| {
            eprintln!("panel: Release {:?} at {:?}", e.contact, e.position)
        });
        // A translucent card inside it, blended.
        s.spawn_with(
            panel,
            Leaf,
            (
                LayoutStyle::default().size(px(200.0), px(120.0)),
                Paint::Quad(Quad::new(Color::rgba(1.0, 1.0, 1.0, 0.35)).radius(16.0)),
            ),
        );
        // A bordered box with uneven sides.
        s.spawn_with(
            win,
            Leaf,
            (
                LayoutStyle::default().size(px(160.0), px(60.0)),
                Paint::Quad(
                    Quad::new(Color::rgb(0.95, 0.8, 0.3))
                        .radius(8.0)
                        .border_widths(geometry::Insets::new(2.0, 8.0, 4.0, 2.0))
                        .border_color(Color::rgb(0.6, 0.2, 0.1)),
                ),
            ),
        );
        // A word in Inter at 32 px, glyphs placed by hand.
        let sprites = {
            let mut atlas = s.resource_mut::<Atlas>();
            let inter = atlas
                .add_font(include_bytes!(
                    "../../atlas/tests/fixtures/Inter-Regular.ttf"
                ))
                .expect("Inter loads");
            let mut pen = 0.0f32;
            let baseline = 30.0f32;
            let mut out = Vec::new();
            for ch in "mecha".chars() {
                let Some((font, id)) = atlas.lookup(&[inter], ch) else {
                    continue;
                };
                let g = atlas.glyph(font, id, 32);
                if g.tile.bounds.width() > 0.0 {
                    out.push(MonochromeSprite::new(
                        g.tile,
                        Point::new(pen + g.left, baseline - g.top),
                        Size::new(g.tile.bounds.width(), g.tile.bounds.height()),
                        Color::WHITE,
                    ));
                }
                pen += g.advance;
            }
            out
        };
        s.spawn_with(
            win,
            Leaf,
            (
                LayoutStyle::default().size(px(200.0), px(40.0)),
                Paint::Monochrome(sprites),
            ),
        );
        Shell
    }
}

fn main() {
    let mut app = App::new();
    app.add_module(LayoutModule)
        .add_module(PaintModule)
        .add_module(WindowModule)
        .add_module(InteractivityModule)
        .add_module(AnimationModule)
        .add_module(RenderModule::default())
        .insert_resource(Atlas::new());
    app.add_module(RingModule::default())
        .add_module(
            WaylandModule::new()
                .bind::<WlCompositor>()
                .bind::<ZwpLinuxDmabufV1>()
                .bind::<XdgWmBase>()
                .bind::<WlSeat>(),
        )
        .add_module(PresentationModule {
            app_id: "mecha.showcase".into(),
            budget: Budget::default(),
        });
    let root = app.root();
    app.spawn(root, ShellBuilder { root });
    app.run();
}
