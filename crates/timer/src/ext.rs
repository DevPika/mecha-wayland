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

    /// Run `handler` every `every`, starting `every` from now. Each
    /// period counts from the fire before it, so a late fire shifts the
    /// rest later; it never fires twice to catch up.
    ///
    /// # Panics
    ///
    /// If `every` is zero.
    fn set_interval(
        &mut self,
        every: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId;

    /// [`SpawnerTimers::set_interval`], registered stopped.
    ///
    /// # Panics
    ///
    /// If `every` is zero.
    fn add_interval(
        &mut self,
        every: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId;
}

/// Control from a handler over timers registered with
/// [`SpawnerTimers`]. Every method takes the [`TimerId`] the
/// registration returned and returns `false`, doing nothing, for a timer
/// that is gone: its node was removed after the id was stored.
pub trait ContextTimers {
    /// Start `id` with its current duration. A running timer restarts
    /// from now.
    fn start_timer(&mut self, id: TimerId) -> bool;

    /// Make `dur` the duration of `id` (an interval's period) and start
    /// it as [`ContextTimers::start_timer`].
    ///
    /// # Panics
    ///
    /// If `id` is an interval and `dur` is zero.
    fn start_timer_for(&mut self, id: TimerId, dur: Duration) -> bool;

    /// Stop `id`. It can be started again.
    fn cancel_timer(&mut self, id: TimerId) -> bool;

    /// Whether `id` is counting down.
    fn timer_running(&self, id: TimerId) -> bool;
}

impl<W: Widget> ContextTimers for Context<'_, W> {
    fn start_timer(&mut self, id: TimerId) -> bool {
        self.resource_mut::<Timers>().start(id, Instant::now())
    }

    fn start_timer_for(&mut self, id: TimerId, dur: Duration) -> bool {
        self.resource_mut::<Timers>()
            .start_for(id, dur, Instant::now())
    }

    fn cancel_timer(&mut self, id: TimerId) -> bool {
        self.resource_mut::<Timers>().cancel(id)
    }

    fn timer_running(&self, id: TimerId) -> bool {
        self.resource::<Timers>().is_running(id)
    }
}

impl<W: Widget> SpawnerTimers<W> for Spawner<'_, W> {
    fn set_interval(
        &mut self,
        every: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId {
        let id = register(self, Kind::Interval, every, handler);
        self.resource_mut::<Timers>().start(id, Instant::now());
        id
    }

    fn add_interval(
        &mut self,
        every: Duration,
        handler: impl FnMut(&mut Context<'_, W>, &Timeout) + 'static,
    ) -> TimerId {
        register(self, Kind::Interval, every, handler)
    }

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
