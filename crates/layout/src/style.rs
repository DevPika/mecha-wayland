//! `LayoutStyle`: the box a node presents to layout, in our own words.
//! Taffy's style traits are implemented on it here, converting per getter,
//! so no taffy type is a field and none appears in a builder.
//!
//! Taffy 0.10.1: `CoreStyle` is `src/style/mod.rs:79`, the flexbox traits
//! `src/style/flex.rs`, the block traits `src/style/block.rs`; every method
//! has a default, and only the ones we carry are overridden.

use app::Component;
use geometry::Insets;
use taffy::geometry::{Rect as TRect, Size as TSize};
use taffy::style::{
    AlignContent, AlignItems, BlockContainerStyle, BlockItemStyle, BoxGenerationMode, CoreStyle,
    Dimension, FlexDirection, FlexWrap, FlexboxContainerStyle, FlexboxItemStyle, LengthPercentage,
    LengthPercentageAuto, Position as TPosition,
};

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// A length as layout understands it. Built with [`auto`], [`px`] and
/// [`percent`]; there is deliberately no conversion from a bare number.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Val {
    #[default]
    Auto,
    Px(f32),
    /// Of the parent, `0.0..=100.0`.
    Percent(f32),
}

pub const fn auto() -> Val {
    Val::Auto
}

pub const fn px(v: f32) -> Val {
    Val::Px(v)
}

/// `percent(50.0)` is half of the parent.
pub const fn percent(v: f32) -> Val {
    Val::Percent(v)
}

impl Val {
    pub(crate) fn dimension(self) -> Dimension {
        match self {
            Val::Auto => Dimension::auto(),
            Val::Px(v) => Dimension::length(v),
            Val::Percent(p) => Dimension::percent(p / 100.0),
        }
    }

    pub(crate) fn length_percentage_auto(self) -> LengthPercentageAuto {
        match self {
            Val::Auto => LengthPercentageAuto::auto(),
            Val::Px(v) => LengthPercentageAuto::length(v),
            Val::Percent(p) => LengthPercentageAuto::percent(p / 100.0),
        }
    }

    /// Where taffy has no `auto` (padding, border, gap), `Auto` is zero.
    pub(crate) fn length_percentage(self) -> LengthPercentage {
        match self {
            Val::Auto => LengthPercentage::length(0.0),
            Val::Px(v) => LengthPercentage::length(v),
            Val::Percent(p) => LengthPercentage::percent(p / 100.0),
        }
    }
}

fn insets_lpa(e: Insets<Val>) -> TRect<LengthPercentageAuto> {
    TRect {
        left: e.left.length_percentage_auto(),
        right: e.right.length_percentage_auto(),
        top: e.top.length_percentage_auto(),
        bottom: e.bottom.length_percentage_auto(),
    }
}

fn insets_lp(e: Insets<Val>) -> TRect<LengthPercentage> {
    TRect {
        left: e.left.length_percentage(),
        right: e.right.length_percentage(),
        top: e.top.length_percentage(),
        bottom: e.bottom.length_percentage(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    #[default]
    Flex,
    Block,
    /// The node and its subtree take no space and get a zero-size box.
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Row,
    Column,
    ColumnReverse,
    RowReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Relative,
    /// Taken out of the parent's flow and placed by `inset`; sized only by
    /// its own style.
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wrap {
    #[default]
    No,
    Yes,
}

impl Align {
    fn items(self) -> AlignItems {
        match self {
            Align::Start => AlignItems::Start,
            Align::Center => AlignItems::Center,
            Align::End => AlignItems::End,
            Align::Stretch => AlignItems::Stretch,
        }
    }
}

impl Justify {
    fn content(self) -> AlignContent {
        match self {
            Justify::Start => AlignContent::Start,
            Justify::Center => AlignContent::Center,
            Justify::End => AlignContent::End,
            Justify::SpaceBetween => AlignContent::SpaceBetween,
            Justify::SpaceAround => AlignContent::SpaceAround,
            Justify::SpaceEvenly => AlignContent::SpaceEvenly,
        }
    }
}

// ---------------------------------------------------------------------------
// LayoutStyle
// ---------------------------------------------------------------------------

/// The box a node presents to layout. Dense: every node has one. Written
/// by reading it, changing it with the verbs, and setting it back.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStyle {
    pub display: Display,
    pub direction: Direction,
    pub position: Position,
    pub justify: Justify,
    /// How wrapped lines are packed along the cross axis. `Start` packs
    /// them, so a wrapping row of fixed-size children leaves its free space
    /// at the end rather than spreading the lines across it.
    pub align_content: Justify,
    pub align_items: Align,
    /// `None` follows the parent's `align_items`.
    pub align_self: Option<Align>,
    pub wrap: Wrap,

    pub width: Val,
    pub height: Val,
    pub min_width: Val,
    pub min_height: Val,
    pub max_width: Val,
    pub max_height: Val,
    pub flex_basis: Val,
    pub flex_grow: f32,
    pub flex_shrink: f32,

    pub row_gap: Val,
    pub column_gap: Val,
    pub padding: Insets<Val>,
    pub margin: Insets<Val>,
    pub border: Insets<Val>,
    pub inset: Insets<Val>,
}

impl Component for LayoutStyle {}

/// CSS's defaults: a flex row, relative, auto-sized, no spacing, shrinkable.
/// Margin in particular is zero and not `Auto`, because an auto margin on a
/// flex item eats the free space that `justify` would otherwise distribute.
impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            direction: Direction::Row,
            position: Position::Relative,
            justify: Justify::Start,
            align_content: Justify::Start,
            align_items: Align::Stretch,
            align_self: None,
            wrap: Wrap::No,
            width: Val::Auto,
            height: Val::Auto,
            min_width: Val::Auto,
            min_height: Val::Auto,
            max_width: Val::Auto,
            max_height: Val::Auto,
            flex_basis: Val::Auto,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            row_gap: Val::Px(0.0),
            column_gap: Val::Px(0.0),
            padding: Insets::all(Val::Px(0.0)),
            margin: Insets::all(Val::Px(0.0)),
            border: Insets::all(Val::Px(0.0)),
            inset: Insets::all(Val::Auto),
        }
    }
}

/// The verbs. All by value, so a style is rewritten in one expression.
impl LayoutStyle {
    pub fn display(mut self, d: Display) -> Self {
        self.display = d;
        self
    }
    pub fn flex(self) -> Self {
        self.display(Display::Flex)
    }
    pub fn block(self) -> Self {
        self.display(Display::Block)
    }
    pub fn hidden(self) -> Self {
        self.display(Display::Hidden)
    }

    pub fn row(mut self) -> Self {
        self.direction = Direction::Row;
        self
    }
    pub fn row_reverse(mut self) -> Self {
        self.direction = Direction::RowReverse;
        self
    }
    pub fn column(mut self) -> Self {
        self.direction = Direction::Column;
        self
    }
    pub fn column_reverse(mut self) -> Self {
        self.direction = Direction::ColumnReverse;
        self
    }

    pub fn absolute(mut self) -> Self {
        self.position = Position::Absolute;
        self
    }
    pub fn relative(mut self) -> Self {
        self.position = Position::Relative;
        self
    }

    pub fn justify(mut self, j: Justify) -> Self {
        self.justify = j;
        self
    }
    pub fn align_content(mut self, j: Justify) -> Self {
        self.align_content = j;
        self
    }
    pub fn align_items(mut self, a: Align) -> Self {
        self.align_items = a;
        self
    }
    pub fn align_self(mut self, a: Align) -> Self {
        self.align_self = Some(a);
        self
    }
    /// Centres children on both axes.
    pub fn center(self) -> Self {
        self.justify(Justify::Center).align_items(Align::Center)
    }
    pub fn wrap(mut self) -> Self {
        self.wrap = Wrap::Yes;
        self
    }

    pub fn width(mut self, v: Val) -> Self {
        self.width = v;
        self
    }
    pub fn height(mut self, v: Val) -> Self {
        self.height = v;
        self
    }
    pub fn size(self, w: Val, h: Val) -> Self {
        self.width(w).height(h)
    }
    /// The whole of the parent on both axes.
    pub fn fill(self) -> Self {
        self.size(percent(100.0), percent(100.0))
    }
    pub fn min_width(mut self, v: Val) -> Self {
        self.min_width = v;
        self
    }
    pub fn min_height(mut self, v: Val) -> Self {
        self.min_height = v;
        self
    }
    pub fn max_width(mut self, v: Val) -> Self {
        self.max_width = v;
        self
    }
    pub fn max_height(mut self, v: Val) -> Self {
        self.max_height = v;
        self
    }
    pub fn flex_basis(mut self, v: Val) -> Self {
        self.flex_basis = v;
        self
    }
    pub fn grow(mut self, g: f32) -> Self {
        self.flex_grow = g;
        self
    }
    pub fn shrink(mut self, s: f32) -> Self {
        self.flex_shrink = s;
        self
    }

    /// The same gap between rows and between columns.
    pub fn gap(mut self, v: Val) -> Self {
        self.row_gap = v;
        self.column_gap = v;
        self
    }
    pub fn row_gap(mut self, v: Val) -> Self {
        self.row_gap = v;
        self
    }
    pub fn column_gap(mut self, v: Val) -> Self {
        self.column_gap = v;
        self
    }
    pub fn padding(mut self, e: Insets<Val>) -> Self {
        self.padding = e;
        self
    }
    pub fn padding_all(self, v: Val) -> Self {
        self.padding(Insets::all(v))
    }
    pub fn margin(mut self, e: Insets<Val>) -> Self {
        self.margin = e;
        self
    }
    pub fn border(mut self, e: Insets<Val>) -> Self {
        self.border = e;
        self
    }
    pub fn inset(mut self, e: Insets<Val>) -> Self {
        self.inset = e;
        self
    }
}

// ---------------------------------------------------------------------------
// Taffy's view of it
// ---------------------------------------------------------------------------

impl CoreStyle for LayoutStyle {
    type CustomIdent = String;

    fn box_generation_mode(&self) -> BoxGenerationMode {
        match self.display {
            Display::Hidden => BoxGenerationMode::None,
            _ => BoxGenerationMode::Normal,
        }
    }
    fn is_block(&self) -> bool {
        self.display == Display::Block
    }
    fn position(&self) -> TPosition {
        match self.position {
            Position::Relative => TPosition::Relative,
            Position::Absolute => TPosition::Absolute,
        }
    }
    fn inset(&self) -> TRect<LengthPercentageAuto> {
        insets_lpa(self.inset)
    }
    fn size(&self) -> TSize<Dimension> {
        TSize {
            width: self.width.dimension(),
            height: self.height.dimension(),
        }
    }
    fn min_size(&self) -> TSize<Dimension> {
        TSize {
            width: self.min_width.dimension(),
            height: self.min_height.dimension(),
        }
    }
    fn max_size(&self) -> TSize<Dimension> {
        TSize {
            width: self.max_width.dimension(),
            height: self.max_height.dimension(),
        }
    }
    fn margin(&self) -> TRect<LengthPercentageAuto> {
        insets_lpa(self.margin)
    }
    fn padding(&self) -> TRect<LengthPercentage> {
        insets_lp(self.padding)
    }
    fn border(&self) -> TRect<LengthPercentage> {
        insets_lp(self.border)
    }
}

impl FlexboxContainerStyle for LayoutStyle {
    fn flex_direction(&self) -> FlexDirection {
        match self.direction {
            Direction::Row => FlexDirection::Row,
            Direction::RowReverse => FlexDirection::RowReverse,
            Direction::Column => FlexDirection::Column,
            Direction::ColumnReverse => FlexDirection::ColumnReverse,
        }
    }
    fn flex_wrap(&self) -> FlexWrap {
        match self.wrap {
            Wrap::No => FlexWrap::NoWrap,
            Wrap::Yes => FlexWrap::Wrap,
        }
    }
    fn gap(&self) -> TSize<LengthPercentage> {
        TSize {
            width: self.column_gap.length_percentage(),
            height: self.row_gap.length_percentage(),
        }
    }
    fn align_items(&self) -> Option<AlignItems> {
        Some(self.align_items.items())
    }
    fn justify_content(&self) -> Option<AlignContent> {
        Some(self.justify.content())
    }
    fn align_content(&self) -> Option<AlignContent> {
        Some(self.align_content.content())
    }
}

impl FlexboxItemStyle for LayoutStyle {
    fn flex_basis(&self) -> Dimension {
        self.flex_basis.dimension()
    }
    fn flex_grow(&self) -> f32 {
        self.flex_grow
    }
    fn flex_shrink(&self) -> f32 {
        self.flex_shrink
    }
    fn align_self(&self) -> Option<AlignItems> {
        self.align_self.map(Align::items)
    }
}

impl BlockContainerStyle for LayoutStyle {}
impl BlockItemStyle for LayoutStyle {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn val_to_dimension() {
        assert_eq!(auto().dimension(), Dimension::auto());
        assert_eq!(px(3.0).dimension(), Dimension::length(3.0));
        assert_eq!(percent(50.0).dimension(), Dimension::percent(0.5));
    }

    #[test]
    fn val_to_length_percentage_auto() {
        assert_eq!(
            auto().length_percentage_auto(),
            LengthPercentageAuto::auto()
        );
        assert_eq!(
            px(3.0).length_percentage_auto(),
            LengthPercentageAuto::length(3.0)
        );
        assert_eq!(
            percent(25.0).length_percentage_auto(),
            LengthPercentageAuto::percent(0.25)
        );
    }

    #[test]
    fn val_to_length_percentage_has_no_auto() {
        assert_eq!(auto().length_percentage(), LengthPercentage::length(0.0));
        assert_eq!(px(3.0).length_percentage(), LengthPercentage::length(3.0));
        assert_eq!(
            percent(10.0).length_percentage(),
            LengthPercentage::percent(0.1)
        );
    }

    #[test]
    fn default_style_as_taffy_sees_it() {
        let s = LayoutStyle::default();
        assert_eq!(s.box_generation_mode(), BoxGenerationMode::Normal);
        assert!(!s.is_block());
        assert_eq!(CoreStyle::position(&s), TPosition::Relative);
        assert_eq!(CoreStyle::size(&s).width, Dimension::auto());
        assert_eq!(
            CoreStyle::margin(&s).left,
            LengthPercentageAuto::length(0.0)
        );
        assert_eq!(CoreStyle::padding(&s).top, LengthPercentage::length(0.0));
        assert_eq!(CoreStyle::inset(&s).top, LengthPercentageAuto::auto());
        assert_eq!(s.flex_direction(), FlexDirection::Row);
        assert_eq!(s.flex_wrap(), FlexWrap::NoWrap);
        assert_eq!(
            FlexboxContainerStyle::align_items(&s),
            Some(AlignItems::Stretch)
        );
        assert_eq!(s.justify_content(), Some(AlignContent::Start));
        assert_eq!(FlexboxItemStyle::flex_grow(&s), 0.0);
        assert_eq!(FlexboxItemStyle::flex_shrink(&s), 1.0);
        assert_eq!(FlexboxItemStyle::align_self(&s), None);
    }

    #[test]
    fn verbs_rewrite_by_value() {
        let s = LayoutStyle::default()
            .column()
            .center()
            .size(px(10.0), percent(50.0))
            .padding_all(px(4.0))
            .gap(px(2.0))
            .grow(1.0)
            .hidden()
            .absolute()
            .wrap();
        assert_eq!(s.direction, Direction::Column);
        assert_eq!(s.justify, Justify::Center);
        assert_eq!(s.align_items, Align::Center);
        assert_eq!((s.width, s.height), (px(10.0), percent(50.0)));
        assert_eq!(s.padding, Insets::all(px(4.0)));
        assert_eq!((s.row_gap, s.column_gap), (px(2.0), px(2.0)));
        assert_eq!(s.flex_grow, 1.0);
        assert_eq!(s.display, Display::Hidden);
        assert_eq!(s.position, Position::Absolute);
        assert_eq!(s.wrap, Wrap::Yes);
        assert_eq!(s.box_generation_mode(), BoxGenerationMode::None);
    }
}
