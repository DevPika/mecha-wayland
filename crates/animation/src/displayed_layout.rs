//! The renderer-facing geometry, distinct from Taffy's resolved target.

use app::Component;
use geometry::{Insets, Rect};
use layout::ComputedLayout;

/// The displayed box in its layout root's coordinates. Initialized from
/// [`ComputedLayout`] on first resolution, then maintained by the animation
/// module. Renderers read this box; frame-time values may be fractional pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Layout {
    pub rect: Rect,
    pub padding: Insets<f32>,
    pub border: Insets<f32>,
}

impl Component for Layout {}

impl From<ComputedLayout> for Layout {
    fn from(value: ComputedLayout) -> Self {
        Self {
            rect: value.rect,
            padding: value.padding,
            border: value.border,
        }
    }
}

impl Layout {
    /// The rect inside padding and border, each dimension clamped at zero.
    pub fn content(&self) -> Rect {
        self.rect.inset(Insets::new(
            self.padding.top + self.border.top,
            self.padding.right + self.border.right,
            self.padding.bottom + self.border.bottom,
            self.padding.left + self.border.left,
        ))
    }
}
