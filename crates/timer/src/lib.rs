#![forbid(unsafe_code)]
//! Timers for widgets, on the ring.

use app::prelude::*;
use ring::prelude::*;

mod ext;
mod systems;
mod timers;

pub use ext::{ContextTimers, SpawnerTimers};
pub use timers::Timers;

pub mod prelude {
    pub use crate::{ContextTimers, SpawnerTimers, Timeout, TimerId, TimerModule, Timers};
}

/// The name of one timer. Never reused within a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimerId(u64);

/// A timer's deadline came. Emitted at the node that registered it.
#[derive(Clone, Copy, Debug)]
pub struct Timeout {
    pub id: TimerId,
}
impl Event for Timeout {}

/// Inserts [`Timers`] and its systems. Installs after `RingModule`.
pub struct TimerModule;

impl Module for TimerModule {
    fn install(self, app: &mut App) {
        assert!(
            app.has_resource::<Ring>(),
            "TimerModule installs after RingModule"
        );
        app.insert_resource(Timers::default());
        app.system(systems::submit).system(systems::complete);
    }
}
