//! The sugar: timers registered from a build.

use std::time::{Duration, Instant};

use app::prelude::*;

use crate::timers::Kind;
use crate::{Timeout, TimerId, Timers};

/// Timers registered from [`Widget::build`]: each is an
/// `on::<Timeout>` handler on the node being built that runs only for its
/// own timer. Keep the returned [`TimerId`] to control the timer from a
/// handler.
pub trait SpawnerTimers<W: Widget> {
    /// Run `handler` once, `after` from now.
    fn set_timeout(
        &mut self,
        after: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId;

    /// [`SpawnerTimers::set_timeout`], registered stopped: it runs
    /// `after` from whenever it is started.
    fn add_timeout(
        &mut self,
        after: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId;
}

impl<W: Widget> SpawnerTimers<W> for Spawner<'_, W> {
    fn set_timeout(
        &mut self,
        after: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId {
        let id = register(self, Kind::Timeout, after, handler);
        self.resource_mut::<Timers>().start(id, Instant::now());
        id
    }

    fn add_timeout(
        &mut self,
        after: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId {
        register(self, Kind::Timeout, after, handler)
    }
}

fn register<W: Widget>(
    s: &mut Spawner<'_, W>,
    kind: Kind,
    dur: Duration,
    mut handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
) -> TimerId {
    let me = s.me();
    let id = s.resource_mut::<Timers>().register(me.into(), kind, dur);
    s.on::<Timeout>(me, move |ctx, e| {
        if e.id == id {
            handler(ctx, e);
        }
    });
    id
}
