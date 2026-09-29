//! The `widgets` crate: `Div`, `Text`, `Icon`, `Image` and `Input` — the
//! primitive widgets a spawn site names instead of hand-writing
//! `LayoutStyle` and `Paint` itself.
//!
//! # Model
//!
//! - The widgets write only `LayoutStyle`, `Measure` and `Paint` —
//!   columns `layout` and `paint` already register. `widgets` installs
//!   no `Module`, `Component` or `Resource` of its own. `Input` also
//!   registers a handler for the `InputEdit` events the `text-input`
//!   crate emits at it, which does nothing without that crate.
//! - Using any widget requires the caller to have installed
//!   `LayoutModule` and `PaintModule` and inserted an `Atlas` resource.
//! - Mutation after spawn happens through each widget's own `*Context`
//!   trait ([`DivContext`], [`TextContext`], [`IconContext`],
//!   [`ImageContext`]), plus the generic `layout::StyleContext` and
//!   `paint::PaintContext`, which apply to every widget.
//!
//! # Quick start
//!
//! ```
//! use app::prelude::*;
//! use atlas::prelude::*;
//! use layout::prelude::*;
//! use paint::prelude::*;
//! use widgets::prelude::*;
//!
//! let mut app = App::new();
//! app.add_module(LayoutModule).add_module(PaintModule);
//! app.insert_resource(Atlas::new());
//!
//! let root = app.spawn_with(
//!     app.root(),
//!     div().style(LayoutStyle::default().size(px(200.0), px(100.0))),
//!     (LayoutRoot(true),),
//! );
//!
//! let font = app
//!     .resource_mut::<Atlas>()
//!     .add_font(include_bytes!("../../atlas/tests/fixtures/Inter-Regular.ttf"))
//!     .unwrap();
//! let sprite = app
//!     .resource_mut::<Atlas>()
//!     .insert(
//!         Class::Icon,
//!         &Bitmap { width: 8, height: 8, format: Format::R8, pixels: vec![255; 64] },
//!     )
//!     .unwrap();
//!
//! let label = app.spawn(root, text(font, "hi"));
//! let glyph = app.spawn(root, icon(sprite));
//!
//! app.tick();
//! assert!(app.component::<Layout>(label).unwrap().rect.width() > 0.0);
//! assert!(app.component::<Layout>(glyph).unwrap().rect.width() > 0.0);
//! ```

mod div;
mod icon;
mod image;
mod input;
mod text;

pub use div::{Div, DivBuilder, DivContext, div};
pub use icon::{Icon, IconBuilder, IconContext, icon};
pub use image::{Image, ImageBuilder, ImageContext, image};
pub use input::{Input, InputBuilder, InputContext, InputEdit, input};
pub use text::{Text, TextBuilder, TextContext, text};

pub mod prelude {
    pub use crate::{
        Div, DivBuilder, DivContext, Icon, IconBuilder, IconContext, Image, ImageBuilder,
        ImageContext, Input, InputBuilder, InputContext, InputEdit, Text, TextBuilder, TextContext,
        div, icon, image, input, text,
    };
}
