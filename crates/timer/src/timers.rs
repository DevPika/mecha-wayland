//! The bookkeeping: which timers exist, when each is due, and which ring
//! tokens are ours. Pure over `Instant`; the systems move data between
//! this, the ring and the app.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use app::prelude::*;
use ring::Token;

use crate::TimerId;

/// A completion this close to its deadline fires rather than re-arming
/// for the rest, so a timeout never chases its own tail in tiny ops.
const SLACK: Duration = Duration::from_millis(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Timeout,
}

struct Entry {
    owner: NodeId,
    kind: Kind,
    dur: Duration,
    /// When it is due; `None` while stopped. The truth: a ring op only
    /// wakes the runner to look at it.
    deadline: Option<Instant>,
    /// The op in flight for this timer and the deadline it was submitted
    /// for. At most one.
    current: Option<(Token, Instant)>,
    /// Running with no op in flight: the next `BeforeWait` submits one.
    needs_submit: bool,
}

/// Every timer, by id, and every ring token the timers still have in
/// flight. Reach it through the `Spawner` and `Context` extension traits;
/// read it with [`Timers::is_running`] and [`Timers::in_flight`].
#[derive(Default)]
pub struct Timers {
    next: u64,
    timers: HashMap<TimerId, Entry>,
    /// Current and stale tokens alike, until their completion arrives.
    tokens: HashMap<Token, TimerId>,
}
impl Resource for Timers {}

impl Timers {
    /// Ring ops the timers have in flight, stale ones included, plus
    /// timers waiting for their op to be submitted. Zero means no timer
    /// will wake the runner.
    pub fn in_flight(&self) -> usize {
        self.tokens.len() + self.timers.values().filter(|e| e.needs_submit).count()
    }

    /// Whether `id` is counting down. `false` for an unknown id.
    pub fn is_running(&self, id: TimerId) -> bool {
        self.timers.get(&id).is_some_and(|e| e.deadline.is_some())
    }

    pub(crate) fn register(&mut self, owner: NodeId, kind: Kind, dur: Duration) -> TimerId {
        self.next += 1;
        let id = TimerId(self.next);
        self.timers.insert(
            id,
            Entry {
                owner,
                kind,
                dur,
                deadline: None,
                current: None,
                needs_submit: false,
            },
        );
        id
    }

    /// Due `dur` from `now`, restarting if running. A later deadline
    /// keeps the op in flight, which re-arms for the rest when it
    /// completes early; an earlier one needs a new op.
    pub(crate) fn start(&mut self, id: TimerId, now: Instant) -> bool {
        let Some(e) = self.timers.get_mut(&id) else {
            return false;
        };
        let deadline = now + e.dur;
        e.deadline = Some(deadline);
        match e.current {
            Some((_, submitted)) if deadline >= submitted => {}
            _ => {
                e.current = None;
                e.needs_submit = true;
            }
        }
        true
    }

    /// Every timer waiting for an op, with how long from `now` it is due.
    pub(crate) fn take_submissions(&mut self, now: Instant) -> Vec<(TimerId, Duration)> {
        self.timers
            .iter_mut()
            .filter(|(_, e)| e.needs_submit)
            .filter_map(|(&id, e)| {
                e.needs_submit = false;
                Some((id, e.deadline?.saturating_duration_since(now)))
            })
            .collect()
    }

    /// `token` is the op just submitted for `id`.
    pub(crate) fn submitted(&mut self, id: TimerId, token: Token) {
        self.tokens.insert(token, id);
        if let Some(e) = self.timers.get_mut(&id)
            && let Some(deadline) = e.deadline
        {
            e.current = Some((token, deadline));
        }
    }

    pub(crate) fn owns(&self, token: Token) -> bool {
        self.tokens.contains_key(&token)
    }

    /// `token` completed at `now`. The node and timer to fire, or `None`
    /// for a stale token, a timer moved later (re-armed for the rest), or
    /// a token that is not ours.
    pub(crate) fn complete(&mut self, token: Token, now: Instant) -> Option<(NodeId, TimerId)> {
        let id = self.tokens.remove(&token)?;
        let e = self.timers.get_mut(&id)?;
        if e.current.map(|(t, _)| t) != Some(token) {
            return None;
        }
        e.current = None;
        let deadline = e.deadline?;
        if now + SLACK < deadline {
            e.needs_submit = true;
            return None;
        }
        match e.kind {
            Kind::Timeout => e.deadline = None,
        }
        Some((e.owner, id))
    }

    /// `token` ended without its timeout elapsing. A running timer whose
    /// op it was gets a new one rather than silently stopping.
    pub(crate) fn abandon(&mut self, token: Token) {
        let Some(id) = self.tokens.remove(&token) else {
            return;
        };
        if let Some(e) = self.timers.get_mut(&id)
            && e.current.map(|(t, _)| t) == Some(token)
        {
            e.current = None;
            e.needs_submit = e.deadline.is_some();
        }
    }
}
