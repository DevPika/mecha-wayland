//! The systems that move data between [`Timers`], the [`Ring`] and the
//! app. Each borrows one resource at a time.

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
        app.emit(Timeout { id }, owner);
    }
}
