//! The `mecha-wayland` facade: one crate to depend on, one prelude to import.
//! Each member crate's prelude is folded into [`prelude`] as it is rewritten
//! against the new `app` core.

pub mod prelude {
    pub use app::prelude::*;
    pub use atlas::prelude::*;
    pub use geometry::prelude::*;
    pub use gles::prelude::*;
    pub use interactivity::prelude::*;
    pub use layout::prelude::*;
    pub use paint::prelude::*;
    pub use presentation::prelude::*;
    pub use render::prelude::*;
    pub use ring::prelude::*;
    pub use text_input::prelude::*;
    pub use theme::prelude::*;
    pub use wayland::prelude::*;
    pub use widgets::prelude::*;
    pub use window::prelude::*;
}
