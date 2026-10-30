//! The systems that move data between [`Timers`], the [`Ring`] and the
//! app. Each borrows one resource at a time.

use std::collections::HashSet;
use std::time::Instant;

use app::prelude::*;
use ring::prelude::*;

use crate::{Timeout, Timers};

/// Submit an op for every timer waiting for one.
pub(crate) fn submit(app: &mut App, _: &BeforeWait) {
    if app.resource::<Timers>().in_flight() == 0 {
        return;
    }
    let subs = app
        .resource_mut::<Timers>()
        .take_submissions(Instant::now());
    for (id, after) in subs {
        let token = app.resource_mut::<Ring>().timeout(after);
        app.resource_mut::<Timers>().submitted(id, token);
    }
}

/// Finish our tokens; fire the timer whose deadline came.
pub(crate) fn complete(app: &mut App, e: &IoEvent) {
    if !app.resource::<Timers>().owns(e.token) {
        return;
    }
    app.resource_mut::<Ring>().finish(e.token);
    if e.result != -libc::ETIME {
        debug_assert!(
            e.result == -libc::ECANCELED,
            "timer op completed with {}",
            e.result
        );
        app.resource_mut::<Timers>().abandon(e.token);
        return;
    }
    let fired = app
        .resource_mut::<Timers>()
        .complete(e.token, Instant::now());
    if let Some((owner, id)) = fired {
        // Removed later in the same flush that queued the `Removed`
        // still to come: sweep now rather than re-arm for a dead node.
        if app.is_live(owner) {
            app.emit(Timeout { id }, owner);
        } else {
            sweep(app);
        }
    }
}

/// A subtree went: its timers go with it. `Removed` names only the
/// subtree's root, so sweep every timer by its owner's liveness.
pub(crate) fn removed(app: &mut App, _: &Removed) {
    sweep(app);
}

fn sweep(app: &mut App) {
    let live: HashSet<NodeId> = app
        .resource::<Timers>()
        .owners()
        .into_iter()
        .filter(|&owner| app.is_live(owner))
        .collect();
    app.resource_mut::<Timers>()
        .retain_live(|owner| live.contains(&owner));
}
