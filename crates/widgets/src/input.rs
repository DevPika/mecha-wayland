use app::{Build, Context, Handle, Widget};
use atlas::FontId;

use crate::{Text, TextContext, div, text};

pub struct Input {
    content: Handle<Text>,
}

pub fn input(font: FontId) -> InputBuilder {
    InputBuilder { font }
}

pub struct InputBuilder {
    font: FontId,
}

impl Build for InputBuilder {
    type Widget = Input;
}

impl Widget for Input {
    type Builder = InputBuilder;

    fn build(
        b: Self::Builder,
        me: app::prelude::Handle<Self>,
        s: &mut app::prelude::Spawner<'_, Self>,
    ) -> Self {
        let div = s.spawn(me, div());
        let content = s.spawn(div, text(b.font, "hello").size(24));
        Input { content }
    }
}

pub trait InputContext {
    fn set_input(&mut self, text: impl Into<String>);
}

impl InputContext for Context<'_, Input> {
    fn set_input(&mut self, text: impl Into<String>) {
        let label = self.me().content;
        self.at(label).unwrap().set_text(text);
    }
}
