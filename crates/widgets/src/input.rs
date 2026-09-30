//! `Input`: an editable line of text, driven by the `text-input` crate's
//! protocol end and readable through its context.
//!
//! The widget owns the committed string, the cursor's byte offset in it
//! and the preedit the input method is composing; what it shows is the
//! string with the preedit inserted at the cursor. Edits arrive as
//! [`InputEdit`] events at the node — the `text-input` crate emits one
//! per `zwp_text_input_v3.done` — and are applied in the order the
//! protocol prescribes: the preedit replaced by the cursor, the
//! requested surroundings deleted, then the commit inserted at the
//! cursor, or the new preedit shown there.

use app::{Build, Context, Event, Handle, Widget};
use atlas::{Atlas, FontId};
use geometry::Color;
use layout::{LayoutStyle, StyleContext, px};
use paint::{Paint, PaintContext, Quad};

use crate::{Div, Text, TextContext, div, text};

/// One applied edit: everything a single `zwp_text_input_v3.done`
/// carried, emitted at the focused [`Input`] by the `text-input` crate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputEdit {
    /// Bytes to delete before the cursor.
    pub delete_before: u32,
    /// Bytes to delete after the cursor.
    pub delete_after: u32,
    /// The commit string: it lands at the cursor (after the requested
    /// surroundings are deleted) and the cursor moves to its end. `None`
    /// when this done carried no commit. Not exclusive with `preedit`:
    /// a single `done` may carry both, the commit inserted first, then
    /// the preedit shown at the new cursor.
    pub commit: Option<String>,
    /// The preedit the input method is composing, shown at the cursor
    /// until a commit replaces it.
    pub preedit: Option<String>,
}
impl Event for InputEdit {}

/// A focus change: emitted at an [`Input`] by the `text-input` crate
/// when it gains or loses the text focus. The widget shows its caret
/// only while focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputFocus {
    pub focused: bool,
}
impl Event for InputFocus {}

pub struct Input {
    content: Handle<Text>,
    /// A thin absolutely-positioned bar overlaid on `content` at the
    /// text cursor's x; the static caret this widget shows.
    caret: Handle<Div>,
    font: FontId,
    px: u16,
    string: String,
    /// The cursor's byte offset in `string`.
    cursor: usize,
    /// The preedit shown at the cursor; never part of `string`.
    preedit: Option<String>,
    /// Whether this widget holds the text focus; the caret is shown
    /// only while this is `true`.
    focused: bool,
}

impl Input {
    /// The committed text, without any preedit.
    pub fn text(&self) -> &str {
        &self.string
    }

    /// The cursor's byte offset in the text.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The preedit shown at the cursor, if any.
    pub fn preedit(&self) -> Option<&str> {
        self.preedit.as_deref()
    }

    /// What the widget shows: the string with the preedit at the cursor.
    fn display(&self) -> String {
        match &self.preedit {
            Some(preedit) => format!(
                "{}{preedit}{}",
                &self.string[..self.cursor],
                &self.string[self.cursor..]
            ),
            None => self.string.clone(),
        }
    }

    /// One edit, in the protocol's order: the preedit is replaced by the
    /// cursor (it never was in `string`), the requested surroundings go,
    /// then the commit lands at the cursor with the cursor at its end,
    /// and the new preedit is shown at the cursor. Commit and preedit
    /// are not exclusive: one `done` may carry both. Deletion lengths
    /// are bytes, clamped to whole characters.
    fn apply(&mut self, e: &InputEdit) {
        let start = back(&self.string, self.cursor, e.delete_before);
        let end = forward(&self.string, self.cursor, e.delete_after);
        self.string.drain(start..end);
        self.cursor = start;
        if let Some(text) = &e.commit {
            self.string.insert_str(self.cursor, text);
            self.cursor += text.len();
        }
        // The preedit is shown at the cursor; `None` clears any prior
        // preedit. Applied after the commit, per the protocol's order.
        self.preedit = e.preedit.clone();
    }
}

/// `n` bytes below `index`, never splitting a character.
fn back(s: &str, index: usize, n: u32) -> usize {
    let mut i = index.saturating_sub(n as usize);
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// `n` bytes above `index`, never splitting a character.
fn forward(s: &str, index: usize, n: u32) -> usize {
    let mut i = (index + n as usize).min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
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
        const PX: u16 = 24;
        let container = s.spawn(me, div());
        let content = s.spawn(container, text(b.font, "").size(PX));
        // A static 1px caret, sized to the line, positioned over `content`
        // by `sync`'s `set_left`. Absolute so it never enters the flex flow.
        // Built with no paint: `sync` turns it on when the widget is
        // focused, so it is invisible until then.
        let line_height = s.resource::<Atlas>().line(b.font, PX).ascent;
        let caret = s.spawn(
            container,
            div().style(
                LayoutStyle::default()
                    .absolute()
                    .size(px(1.0), px(line_height)),
            ),
        );
        s.on::<InputEdit>(me, |ctx, e| {
            ctx.me().apply(e);
            sync(ctx);
        });
        s.on::<InputFocus>(me, |ctx, e| {
            ctx.me().focused = e.focused;
            sync(ctx);
        });
        Input {
            content,
            caret,
            font: b.font,
            px: PX,
            string: String::new(),
            cursor: 0,
            preedit: None,
            focused: false,
        }
    }
}

/// Push what the widget shows to its `Text`, and park the caret over
/// it at the text cursor's x — the width of the glyphs before the
/// cursor. The caret is hidden (`Paint::None`) when the widget is not
/// focused.
fn sync(ctx: &mut Context<'_, Input>) {
    let (content, caret, font, font_px, focused, before_cursor, display) = {
        let me = ctx.me();
        (
            me.content,
            me.caret,
            me.font,
            me.px,
            me.focused,
            me.string[..me.cursor].to_string(),
            me.display(),
        )
    };
    ctx.at(content).unwrap().set_text(display);
    let x = if focused {
        let mut atlas = ctx.resource_mut::<Atlas>();
        crate::text::measure(&mut atlas, font, font_px, &before_cursor)
    } else {
        0.0
    };
    let mut caret = ctx.at(caret).unwrap();
    if focused {
        caret.set_left(px(x));
        caret.set_paint(Paint::Quad(Quad::new(Color::WHITE)));
    } else {
        caret.set_paint(Paint::None);
    }
}

pub trait InputContext {
    /// Replace the content. The cursor moves to the end and any preedit
    /// is dropped.
    fn set_text(&mut self, text: impl Into<String>);
}

impl InputContext for Context<'_, Input> {
    fn set_text(&mut self, text: impl Into<String>) {
        let me = self.me();
        me.string = text.into();
        me.cursor = me.string.len();
        me.preedit = None;
        sync(self);
    }
}
