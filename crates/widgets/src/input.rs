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
use atlas::FontId;

use crate::{Text, TextContext, div, text};

/// One applied edit: everything a single `zwp_text_input_v3.done`
/// carried, emitted at the focused [`Input`] by the `text-input` crate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputEdit {
    /// Bytes to delete before the cursor.
    pub delete_before: u32,
    /// Bytes to delete after the cursor.
    pub delete_after: u32,
    /// The commit string: it replaces the preedit and lands at the
    /// cursor. `None` when this done carried no commit, in which case
    /// the preedit below is shown instead.
    pub commit: Option<String>,
    /// The preedit the input method is composing, shown at the cursor
    /// until a commit replaces it.
    pub preedit: Option<String>,
}
impl Event for InputEdit {}

pub struct Input {
    content: Handle<Text>,
    string: String,
    /// The cursor's byte offset in `string`.
    cursor: usize,
    /// The preedit shown at the cursor; never part of `string`.
    preedit: Option<String>,
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
    /// or the new preedit is shown there. Deletion lengths are bytes,
    /// clamped to whole characters.
    fn apply(&mut self, e: &InputEdit) {
        let start = back(&self.string, self.cursor, e.delete_before);
        let end = forward(&self.string, self.cursor, e.delete_after);
        self.string.drain(start..end);
        self.cursor = start;
        if let Some(text) = &e.commit {
            self.string.insert_str(self.cursor, text);
            self.cursor += text.len();
            self.preedit = None;
        } else {
            self.preedit = e.preedit.clone();
        }
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
        let div = s.spawn(me, div());
        let content = s.spawn(div, text(b.font, "").size(24));
        s.on::<InputEdit>(me, |ctx, e| {
            ctx.me().apply(e);
            sync(ctx);
        });
        Input {
            content,
            string: String::new(),
            cursor: 0,
            preedit: None,
        }
    }
}

/// Push what the widget shows to its `Text`.
fn sync(ctx: &mut Context<'_, Input>) {
    let (label, display) = {
        let me = ctx.me();
        (me.content, me.display())
    };
    ctx.at(label).unwrap().set_text(display);
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
