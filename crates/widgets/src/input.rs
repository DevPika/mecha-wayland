use app::{Build, Widget};
use atlas::FontId;

use crate::{div, text};

pub struct Input;

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
        s.spawn(div, text(b.font, "hello").size(24));
        Input
    }
}
