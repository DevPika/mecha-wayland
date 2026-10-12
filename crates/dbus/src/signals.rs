//! What the crate tells the app: one `Signal` per kind of news, and the
//! plumbing that raises them after the `DBus` guard is released.

use app::prelude::*;
use smallvec::SmallVec;

use crate::bus::BusId;
use crate::client::{CallError, CallId};
use crate::matching::MatchId;
use crate::message::Message;
use crate::names::BusName;

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

/// The answer to one call. Exactly one per [`CallId`], unless the call
/// was cancelled first.
#[derive(Debug, Clone)]
pub struct MethodReply {
    pub call: CallId,
    pub result: Result<Message, CallError>,
}
impl Signal for MethodReply {}

/// A signal satisfying at least one rule added with `add_match`. One per
/// signal, listing every rule it satisfies in the order they were added.
#[derive(Debug, Clone)]
pub struct SignalReceived {
    pub bus: BusId,
    pub matches: SmallVec<[MatchId; 2]>,
    pub msg: Message,
}
impl Signal for SignalReceived {}

/// The daemon gave this connection `name`.
#[derive(Debug, Clone)]
pub struct NameAcquired {
    pub bus: BusId,
    pub name: BusName,
}
impl Signal for NameAcquired {}

/// This connection no longer owns `name`.
#[derive(Debug, Clone)]
pub struct NameLost {
    pub bus: BusId,
    pub name: BusName,
}
impl Signal for NameLost {}

/// The daemon refused the rule behind `id`. The local filter stays, so
/// signals the daemon sends anyway still match.
#[derive(Debug, Clone)]
pub struct MatchFailed {
    pub id: MatchId,
    pub error: CallError,
}
impl Signal for MatchFailed {}
