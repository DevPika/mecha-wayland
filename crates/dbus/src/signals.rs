//! What the crate tells the app: one `Signal` per kind of news, and the
//! plumbing that raises them after the `DBus` guard is released.

use app::prelude::*;

/// One signal to raise. Connection code builds these while it holds the
/// `DBus` resource; [`emit`] raises them once it is released.
pub(crate) type Out = Box<dyn FnOnce(&mut App)>;

pub(crate) fn out<S: Signal>(s: S) -> Out {
    Box::new(move |app: &mut App| app.signal(s))
}

pub(crate) fn emit(app: &mut App, outs: Vec<Out>) {
    for o in outs {
        o(app);
    }
}
