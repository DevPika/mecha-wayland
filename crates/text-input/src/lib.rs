#![forbid(unsafe_code)]
//! `zwp_text_input_v3`: on-screen-keyboard text into the `Input` widget.
//!
//! # Model
//!
//! - [`TextInputModule`] binds `zwp_text_input_manager_v3` and makes one
//!   `zwp_text_input_v3` for the seat. Installs after `RingModule`,
//!   `WaylandModule` (which must have bound `WlSeat`) and
//!   `InteractivityModule`.
//! - Focus follows the press, but the press is detected *in the
//!   widget*: an `Input` whose `Press` handler runs emits an
//!   `InputFocus { focused: true }` at itself, which this crate's
//!   `Emitted<InputFocus>` system turns into a committed `enable`
//!   carrying the widget's text as the surrounding text. A `Press` that
//!   misses the focused widget — by a cheap id check against the press
//!   targets, no per-press tree walk — blurs it with a committed
//!   `disable`. The surface focus the `Enter`/`Leave` events carry is
//!   the compositor's business: a `Leave` only resets (the compositor
//!   ignores our requests until the next `Enter` anyway) — and, not
//!   being double-buffered, clears the widget's preedit immediately —
//!   and that `Enter` re-enables for whichever widget is still focused.
//! - An edit is everything one `done` carries. The `preedit_string`,
//!   `commit_string` and `delete_surrounding_text` events since the last
//!   `done` accumulate in [`TextInput`]; the `done` emits one
//!   `widgets::InputEdit` at the focused widget, whose handler applies
//!   it — delete around the cursor, then the commit at the cursor, else
//!   the new preedit shown there — and funnels the result to its `Text`
//!   through the `Input` context's `set_text`.
//! - The `Emitted<InputEdit>` that follows sends the widget's new
//!   surrounding text back with `set_text_change_cause(input_method)`,
//!   committed, so the input method's picture of the text stays true
//!   across every edit: a backspace's `delete_surrounding_text` is
//!   counted from what we last reported.
//! - `language`, `action` and `preedit_hint` events are read and
//!   dropped: v0 has nowhere to put them. The `done` serial is not
//!   checked against our commit count; the surrounding text is resent
//!   after every edit regardless, which is what a mismatched serial
//!   asks for anyway.
//! - A focused widget removed from the tree blurs.
//!
//! # Quick start
//!
//! ```no_run
//! use app::prelude::*;
//! use atlas::prelude::*;
//! use interactivity::prelude::*;
//! use layout::prelude::*;
//! use paint::prelude::*;
//! use ring::prelude::*;
//! use text_input::prelude::*;
//! use wayland::prelude::*;
//! use widgets::prelude::*;
//! use window::prelude::*;
//!
//! let mut app = App::new();
//! app.add_module(LayoutModule)
//!     .add_module(PaintModule)
//!     .add_module(WindowModule)
//!     .add_module(InteractivityModule);
//! app.add_module(RingModule::default())
//!     .add_module(WaylandModule::new().bind::<WlSeat>())
//!     .add_module(TextInputModule);
//! app.insert_resource(Atlas::new());
//! let font = app
//!     .resource_mut::<Atlas>()
//!     .add_font(include_bytes!("../../atlas/tests/fixtures/Inter-Regular.ttf"))
//!     .unwrap();
//! let root = app.root();
//! let win = app.spawn(root, window());
//! app.spawn(win, input(font));
//! // A press on the input focuses it, the compositor raises the OSK,
//! // and every key press lands in the widget.
//! app.run();
//! ```

use app::prelude::*;
use interactivity::Press;
use wayland::prelude::*;
use widgets::{Input, InputEdit, InputFocus};

pub mod prelude {
    pub use crate::{TextInput, TextInputModule};
}

/// The protocol's byte cap on `set_surrounding_text`.
const SURROUNDING_MAX: usize = 4000;

/// The seat's text input: the `zwp_text_input_v3` object and the focus
/// bookkeeping around it. One per app in v0, for whatever seat
/// `WaylandModule` bound.
pub struct TextInput {
    input: ZwpTextInputV3,
    /// Whether the compositor's text-input focus is on our surface; an
    /// `enable` is only valid while it is.
    entered: bool,
    /// Whether a committed `enable` is in effect.
    enabled: bool,
    /// The input widget that wants the text, if any.
    focused: Option<NodeId>,
    /// The edit the events since the last `done` have built up.
    pending: Option<InputEdit>,
}
impl Resource for TextInput {}

impl TextInput {
    /// The input widget that currently holds the text focus.
    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }
}

/// Binds the manager, makes the seat's text input, and listens.
///
/// # Panics
///
/// At install, if `WlSeat` was not bound by `WaylandModule`, or if the
/// compositor does not advertise `zwp_text_input_manager_v3`.
pub struct TextInputModule;

impl Module for TextInputModule {
    fn install(self, app: &mut App) {
        let seat = *app.resource::<WlSeat>();
        let manager = {
            let global = app
                .resource::<Globals>()
                .find(ZwpTextInputManagerV3::NAME)
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "text-input: the compositor does not advertise {}",
                        ZwpTextInputManagerV3::NAME
                    )
                });
            let (globals, mut wl) = app.query::<(Res<Globals>, ResMut<Wayland>)>();
            globals.bind::<ZwpTextInputManagerV3>(&global, &mut wl)
        };
        let input = {
            let mut wl = app.resource_mut::<Wayland>();
            let input = manager.get_text_input(&mut wl, seat);
            manager.destroy(&mut wl);
            input
        };
        app.insert_resource(TextInput {
            input,
            entered: false,
            enabled: false,
            focused: None,
            pending: None,
        });
        app.system(on_text_input)
            .system(on_press)
            .system(on_focus)
            .system(on_edited)
            .system(on_removed);
    }
}

/// Focus `w`: the committed `enable` for it, with its text as the
/// surrounding text. A different widget already enabled is disabled
/// first, as the protocol asks. Before an `Enter` the flags are all the
/// request can set: the `Enter` that follows runs this again. Protocol
/// only — the caret toggle is the widget's own `InputFocus` handler,
/// driven by the emits the press path and `on_focus` produce.
fn focus(app: &mut App, w: NodeId) {
    let (text, cursor) = match app.widget::<Input>(w) {
        Some(i) => (i.text().to_string(), i.cursor()),
        None => (String::new(), 0),
    };
    let (mut ti, mut wl) = app.query::<(ResMut<TextInput>, ResMut<Wayland>)>();
    let ti = &mut *ti;
    if ti.enabled {
        ti.input.disable(&mut wl);
        ti.input.commit(&mut wl);
        ti.enabled = false;
    }
    ti.focused = Some(w);
    ti.pending = None;
    if ti.entered && !ti.enabled {
        ti.input.enable(&mut wl);
        let (text, cursor) = cap(&text, cursor);
        ti.input
            .set_surrounding_text(&mut wl, &text, cursor as i32, cursor as i32);
        ti.input.commit(&mut wl);
        ti.enabled = true;
    }
}

/// Blur: the committed `disable`, and no focused widget. Protocol only.
fn blur(app: &mut App) {
    let (mut ti, mut wl) = app.query::<(ResMut<TextInput>, ResMut<Wayland>)>();
    let ti = &mut *ti;
    if ti.enabled {
        ti.input.disable(&mut wl);
        ti.input.commit(&mut wl);
    }
    ti.enabled = false;
    ti.focused = None;
    ti.pending = None;
}

/// The surrounding text is capped at 4000 bytes by the protocol; keep
/// the window that ends at the string's end, so the cursor stays in it.
fn cap(text: &str, cursor: usize) -> (String, usize) {
    if text.len() <= SURROUNDING_MAX {
        return (text.to_string(), cursor);
    }
    let mut start = text.len() - SURROUNDING_MAX;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    (text[start..].to_string(), cursor.saturating_sub(start))
}

/// A press that misses the focused widget blurs it — the way tapping
/// outside a field ends its edit on a phone. Focus gain is the widget's
/// own `Press` handler emitting `InputFocus { focused: true }`; this
/// system only handles the converse, and by a cheap id check against the
/// press targets — no per-press tree walk for an `Input`.
fn on_press(app: &mut App, e: &Emitted<Press>) {
    let Some(focused) = app.resource::<TextInput>().focused else {
        return;
    };
    if !e.targets.iter().any(|&id| id == focused) {
        app.emit(InputFocus { focused: false }, focused);
    }
}

/// The widget's focus request (from its `Press` handler) or blur (from
/// `on_press`): drive the protocol — `enable`/`disable`/
/// `set_surrounding_text`/`commit` — for the change. Idempotent: a
/// redundant `{ focused: true }` on the already-focused widget, or
/// `{ focused: false }` on one no longer focused, is a no-op.
fn on_focus(app: &mut App, e: &Emitted<InputFocus>) {
    let Some(&w) = e.targets.first() else {
        return;
    };
    if e.event.focused {
        if app.resource::<TextInput>().focused != Some(w) {
            focus(app, w);
        }
    } else if app.resource::<TextInput>().focused == Some(w) {
        blur(app);
    }
}

/// The protocol's events. The edit events accumulate; `done` hands them
/// to the focused widget as one `InputEdit`.
fn on_text_input(app: &mut App, e: &ZwpTextInputV3Event) {
    match e {
        ZwpTextInputV3Event::Enter { .. } => {
            let w = {
                let mut ti = app.resource_mut::<TextInput>();
                ti.entered = true;
                ti.pending = None;
                ti.focused.filter(|_| !ti.enabled)
            };
            if let Some(w) = w {
                focus(app, w);
            }
        }
        ZwpTextInputV3Event::Leave { .. } => {
            let w = {
                let mut ti = app.resource_mut::<TextInput>();
                ti.entered = false;
                ti.enabled = false;
                ti.pending = None;
                ti.focused
            };
            // `Leave` is not double-buffered: clear the widget's preedit.
            if let Some(w) = w {
                app.emit(InputEdit::default(), w);
            }
        }
        ZwpTextInputV3Event::PreeditString { text, .. } => {
            let mut ti = app.resource_mut::<TextInput>();
            let edit = ti.pending.get_or_insert_default();
            edit.preedit = text.clone().filter(|t| !t.is_empty());
        }
        ZwpTextInputV3Event::CommitString { text, .. } => {
            let mut ti = app.resource_mut::<TextInput>();
            let edit = ti.pending.get_or_insert_default();
            // A null text is an empty commit; not exclusive with preedit.
            edit.commit = Some(text.clone().unwrap_or_default());
        }
        ZwpTextInputV3Event::DeleteSurroundingText {
            before_length,
            after_length,
            ..
        } => {
            let mut ti = app.resource_mut::<TextInput>();
            let edit = ti.pending.get_or_insert_default();
            edit.delete_before = *before_length;
            edit.delete_after = *after_length;
        }
        ZwpTextInputV3Event::Done { .. } => {
            let (w, edit) = {
                let mut ti = app.resource_mut::<TextInput>();
                (ti.focused, ti.pending.take())
            };
            // A `done` with no accumulated events still must clear any
            // preedit the widget is showing: a default `InputEdit`
            // carries no commit and an empty preedit, which `apply`
            // turns into `self.preedit = None`.
            if let Some(w) = w {
                app.emit(edit.unwrap_or_default(), w);
            }
        }
        // `action`, `language` and `preedit_hint` have nowhere to go in v0.
        _ => {}
    }
}

/// The focused widget applied an edit: report its new text back, with
/// the input method as the cause, committed.
fn on_edited(app: &mut App, e: &Emitted<InputEdit>) {
    let Some(&w) = e.targets.first() else {
        return;
    };
    let (text, cursor) = match app.widget::<Input>(w) {
        Some(i) => cap(i.text(), i.cursor()),
        None => return,
    };
    let (mut ti, mut wl) = app.query::<(ResMut<TextInput>, ResMut<Wayland>)>();
    let ti = &mut *ti;
    if !ti.enabled {
        return;
    }
    ti.input
        .set_surrounding_text(&mut wl, &text, cursor as i32, cursor as i32);
    ti.input
        .set_text_change_cause(&mut wl, ZwpTextInputV3ChangeCause::InputMethod);
    ti.input.commit(&mut wl);
}

/// A focused widget leaving the tree blurs.
fn on_removed(app: &mut App, r: &Removed) {
    if app.resource::<TextInput>().focused == Some(r.id) {
        blur(app);
    }
}
