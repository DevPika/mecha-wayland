use crate::{
    commands::{clear_color::ClearColorQueue, opaque::OpaqueQueue, translucent::TranslucentQueue},
    texture::TextureStore,
};

pub use utils::{Color, Point, Rect, Size};

mod clear_color;
pub use clear_color::ClearColor;

mod draw_quad;
pub use draw_quad::DrawQuad;

mod draw_rect;
pub use draw_rect::DrawRect;

mod draw_mono_sprite;
pub use draw_mono_sprite::DrawMonochromeSprite;

mod draw_text;
pub use draw_text::DrawText;

pub(crate) mod opaque;
pub(crate) mod translucent;

pub struct RenderContext<'a> {
    pub gl: &'a glow::Context,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub(crate) textures: &'a TextureStore,
}

pub trait Command {
    fn record(self, registry: &mut CommandQueueRegistry);
}

#[derive(Default)]
pub struct CommandQueueRegistry {
    pub(crate) clear: ClearColorQueue,
    pub(crate) opaque: OpaqueQueue,
    pub(crate) translucent: TranslucentQueue,
}

impl CommandQueueRegistry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init(&mut self, ctx: &RenderContext) {
        self.opaque.init(ctx);
        self.translucent.init(ctx);
    }

    pub fn record<C: Command>(&mut self, command: C) {
        command.record(self);
    }

    pub fn process_clear(&mut self, ctx: &RenderContext) {
        self.clear.process(ctx);
    }

    pub fn process_opaque(&mut self, ctx: &RenderContext) {
        self.opaque.process(ctx);
    }

    pub fn process_translucent(&mut self, ctx: &RenderContext) {
        self.translucent.process(ctx);
    }
}
