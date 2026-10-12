//! One bus connection: the socket, what is waiting to go out, what came
//! in and is not yet a whole message, and every call still owed a reply.
//! Methods return the signals to raise as [`Out`]s; they never touch the
//! `App`.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::time::Instant;

use ring::{Completed, IoEvent, Ring, Token};

use crate::bus::BusId;
use crate::client::{CallError, CallId, MethodCall};
use crate::matching::{Entry, MatchId, MatchRule, Matches, Owner};
use crate::message::{Flags, Message, MessageKind};
use crate::names::{BusName, InterfaceName, MemberName};
use crate::signals::{MatchFailed, MethodReply, NameAcquired, NameLost, Out, SignalReceived, out};
use crate::types::Body;

pub(crate) const DAEMON: &str = "org.freedesktop.DBus";
pub(crate) const DAEMON_PATH: &str = "/org/freedesktop/DBus";
/// The one receive's buffer.
pub(crate) const READ_CAPACITY: usize = 64 * 1024;
/// `SCM_MAX_FD`: room for every fd one `sendmsg` can carry.
pub(crate) const MAX_FDS: usize = 253;

/// A call this connection made and has not had answered.
pub(crate) enum Pending {
    /// The connection's own bookkeeping; answered internally.
    Internal(Internal),
    /// A caller's call; its answer is a `MethodReply`.
    User { deadline: Instant },
}

pub(crate) enum Internal {
    Hello,
    /// `AddMatch` for the rule with this string.
    AddMatch(String),
    /// `AddMatch` for an owner-following rule; failures are ignored.
    OwnerMatch,
    GetNameOwner(BusName),
}

pub(crate) struct Connection {
    pub(crate) id: BusId,
    stream: UnixStream,
    pub(crate) fds_ok: bool,
    pub(crate) unique_name: Option<BusName>,
    pub(crate) dead: bool,
    pub(crate) reading: Option<Token>,
    pub(crate) sending: Option<Token>,
    inbox: Vec<u8>,
    in_fds: VecDeque<OwnedFd>,
    /// Encoded messages and their fds, oldest first.
    outbox: VecDeque<(Vec<u8>, Vec<OwnedFd>)>,
    next_serial: u32,
    pending: HashMap<u32, Pending>,
    /// Every `User` call's deadline, earliest first. Entries for calls
    /// already answered are skipped when they surface.
    deadlines: BinaryHeap<Reverse<(Instant, u32)>>,
    /// Ring timeouts in flight, and the deadline each was armed for.
    timers: Vec<(Token, Instant)>,
    /// Calls that failed before being sent; answered at `BeforeWait`.
    deferred: Vec<(u32, CallError)>,
    pub(crate) matches: Matches,
}

impl Connection {
    pub(crate) fn new(id: BusId, stream: UnixStream, fds_ok: bool) -> Self {
        Connection {
            id,
            stream,
            fds_ok,
            unique_name: None,
            dead: false,
            reading: None,
            sending: None,
            inbox: Vec::new(),
            in_fds: VecDeque::new(),
            outbox: VecDeque::new(),
            next_serial: 1,
            pending: HashMap::new(),
            deadlines: BinaryHeap::new(),
            timers: Vec::new(),
            deferred: Vec::new(),
            matches: Matches::default(),
        }
    }

    pub(crate) fn arm_read(&mut self, ring: &mut Ring, buf: Vec<u8>) {
        self.reading = Some(ring.recvmsg(self.stream.as_fd(), buf, MAX_FDS));
    }

    /// The next serial: never zero, wrapping past `u32::MAX` to 1.
    pub(crate) fn serial(&mut self) -> u32 {
        let s = self.next_serial;
        self.next_serial = if s == u32::MAX { 1 } else { s + 1 };
        s
    }

    /// Queue `msg` under a fresh serial and return it. A dead connection
    /// spends the serial and sends nothing.
    pub(crate) fn queue(&mut self, msg: &Message) -> u32 {
        let serial = self.serial();
        if self.dead {
            return serial;
        }
        let (bytes, fds) = msg.encode(serial);
        let fds = fds
            .iter()
            .map(|f| f.try_clone_owned().expect("dbus: dup an fd to send"))
            .collect();
        self.outbox.push_back((bytes, fds));
        serial
    }

    /// Whether a send is queued or in flight.
    pub(crate) fn has_pending_output(&self) -> bool {
        !self.dead && (!self.outbox.is_empty() || self.sending.is_some())
    }

    /// Send what is queued in one `sendmsg`, if none is in flight, cut
    /// at a message boundary so no send carries more than [`MAX_FDS`].
    pub(crate) fn flush(&mut self, ring: &mut Ring) {
        if self.dead || self.sending.is_some() || self.outbox.is_empty() {
            return;
        }
        let mut buf = Vec::new();
        let mut fds = Vec::new();
        while let Some((_, f)) = self.outbox.front() {
            if !buf.is_empty() && fds.len() + f.len() > MAX_FDS {
                break;
            }
            let (b, f) = self.outbox.pop_front().unwrap();
            buf.extend_from_slice(&b);
            fds.extend(f);
        }
        self.sending = Some(ring.sendmsg(self.stream.as_fd(), buf, fds));
    }

    pub(crate) fn owns(&self, t: Token) -> bool {
        self.reading == Some(t)
            || self.sending == Some(t)
            || self.timers.iter().any(|&(tt, _)| tt == t)
    }

    pub(crate) fn before_wait(&mut self, ring: &mut Ring) -> Vec<Out> {
        let bus = self.id;
        let outs = self
            .deferred
            .drain(..)
            .map(|(serial, e)| {
                out(MethodReply {
                    call: CallId { bus, serial },
                    result: Err(e),
                })
            })
            .collect();
        self.flush(ring);
        self.arm_timer(ring);
        outs
    }

    pub(crate) fn call(&mut self, call: MethodCall) -> CallId {
        let fds_refused = !call.msg.fds().is_empty() && !self.fds_ok;
        let serial = if fds_refused {
            self.serial()
        } else {
            self.queue(&call.msg)
        };
        if fds_refused {
            self.deferred.push((serial, CallError::FdsUnsupported));
        } else if self.dead {
            self.deferred.push((serial, CallError::Disconnected));
        } else {
            let deadline = Instant::now() + call.timeout;
            self.pending.insert(serial, Pending::User { deadline });
            self.deadlines.push(Reverse((deadline, serial)));
        }
        CallId {
            bus: self.id,
            serial,
        }
    }

    pub(crate) fn call_no_reply(&mut self, call: MethodCall) {
        if !call.msg.fds().is_empty() && !self.fds_ok {
            return;
        }
        let flags = call.msg.flags() | Flags::NO_REPLY_EXPECTED;
        let msg = call.msg.with_flags(flags);
        self.queue(&msg);
    }

    /// Forget a call: no `MethodReply` will be raised for it.
    pub(crate) fn cancel(&mut self, serial: u32) {
        if matches!(self.pending.get(&serial), Some(Pending::User { .. })) {
            self.pending.remove(&serial);
        }
        self.deferred.retain(|(s, _)| *s != serial);
    }

    /// Arm a ring timeout for the earliest pending deadline unless one
    /// armed already fires by then.
    fn arm_timer(&mut self, ring: &mut Ring) {
        while let Some(Reverse((_, s))) = self.deadlines.peek() {
            if matches!(self.pending.get(s), Some(Pending::User { .. })) {
                break;
            }
            self.deadlines.pop();
        }
        let Some(&Reverse((at, _))) = self.deadlines.peek() else {
            return;
        };
        if self.timers.iter().any(|&(_, t)| t <= at) {
            return;
        }
        let token = ring.timeout(at.saturating_duration_since(Instant::now()));
        self.timers.push((token, at));
    }

    /// Fail every call whose deadline has passed.
    fn expire(&mut self) -> Vec<Out> {
        let now = Instant::now();
        let mut outs = Vec::new();
        while let Some(&Reverse((at, serial))) = self.deadlines.peek() {
            if at > now {
                break;
            }
            self.deadlines.pop();
            if let Some(Pending::User { .. }) = self.pending.get(&serial) {
                self.pending.remove(&serial);
                outs.push(out(MethodReply {
                    call: CallId {
                        bus: self.id,
                        serial,
                    },
                    result: Err(CallError::Timeout),
                }));
            }
        }
        outs
    }

    pub(crate) fn on_io(&mut self, e: &IoEvent, done: Completed, ring: &mut Ring) -> Vec<Out> {
        if let Some(i) = self.timers.iter().position(|&(t, _)| t == e.token) {
            self.timers.swap_remove(i);
            return self.expire();
        }
        if self.reading == Some(e.token) {
            self.reading = None;
            if e.result <= 0 {
                return self.die();
            }
            self.inbox.extend_from_slice(&done.buf);
            self.in_fds.extend(done.fds);
            self.arm_read(ring, done.buf);
            self.read_messages()
        } else if self.sending == Some(e.token) {
            self.sending = None;
            if e.result < 0 {
                return self.die();
            }
            let sent = e.result as usize;
            if sent < done.buf.len() {
                // The fds went with the first byte; the tail carries none.
                self.outbox
                    .push_front((done.buf[sent..].to_vec(), Vec::new()));
            }
            Vec::new()
        } else {
            Vec::new()
        }
    }

    /// Handle every whole message in the inbox. Anything unframeable is a
    /// protocol violation: the specification says to drop the connection.
    fn read_messages(&mut self) -> Vec<Out> {
        let mut outs = Vec::new();
        let mut at = 0;
        loop {
            match Message::frame_len(&self.inbox[at..]) {
                Ok(None) => break,
                Ok(Some(n)) => match Message::parse(&self.inbox[at..at + n], &mut self.in_fds) {
                    Ok(msg) => {
                        at += n;
                        outs.extend(self.handle(msg));
                        if self.dead {
                            return outs;
                        }
                    }
                    Err(_) => {
                        outs.extend(self.die());
                        return outs;
                    }
                },
                Err(_) => {
                    outs.extend(self.die());
                    return outs;
                }
            }
        }
        self.inbox.drain(..at);
        outs
    }

    fn handle(&mut self, msg: Message) -> Vec<Out> {
        match msg.kind() {
            MessageKind::MethodReturn | MessageKind::Error => {
                let Some(serial) = msg.reply_serial() else {
                    return Vec::new();
                };
                match self.pending.remove(&serial) {
                    Some(Pending::Internal(i)) => self.internal_reply(i, msg),
                    Some(Pending::User { .. }) => {
                        let call = CallId {
                            bus: self.id,
                            serial,
                        };
                        let result = if msg.kind() == MessageKind::Error {
                            Err(CallError::Remote {
                                name: msg
                                    .error_name()
                                    .cloned()
                                    .expect("dbus: parse requires an error name"),
                                message: msg.error_text().unwrap_or_default(),
                            })
                        } else {
                            Ok(msg)
                        };
                        vec![out(MethodReply { call, result })]
                    }
                    None => Vec::new(),
                }
            }
            MessageKind::Signal => {
                let mut outs = self.daemon_signal(&msg);
                let matches = self.matches.matching(&msg, self.id);
                if !matches.is_empty() {
                    outs.push(out(SignalReceived {
                        bus: self.id,
                        matches,
                        msg,
                    }));
                }
                outs
            }
            MessageKind::MethodCall => Vec::new(),
        }
    }

    fn internal_reply(&mut self, i: Internal, msg: Message) -> Vec<Out> {
        match i {
            Internal::Hello => {
                if msg.kind() == MessageKind::MethodReturn
                    && let Ok((name,)) = msg.body::<(BusName,)>()
                {
                    self.unique_name = Some(name);
                    return Vec::new();
                }
                // Refused, or no name: install would wait forever.
                self.die()
            }
            Internal::AddMatch(key) => {
                if msg.kind() != MessageKind::Error {
                    return Vec::new();
                }
                let error = CallError::Remote {
                    name: msg
                        .error_name()
                        .cloned()
                        .expect("dbus: parse requires an error name"),
                    message: msg.error_text().unwrap_or_default(),
                };
                let bus = self.id;
                self.matches
                    .rules
                    .get(&key)
                    .map(|e| {
                        e.ids
                            .iter()
                            .map(|&n| {
                                out(MatchFailed {
                                    id: MatchId { bus, n },
                                    error: error.clone(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Internal::OwnerMatch => Vec::new(),
            Internal::GetNameOwner(name) => {
                let unique = msg.body::<(BusName,)>().ok().map(|(u,)| u);
                if let Some(owner) = self.matches.owners.get_mut(&name) {
                    owner.unique = unique;
                }
                Vec::new()
            }
        }
    }

    /// A call to the daemon whose answer the connection handles itself.
    fn daemon_call<B: Body>(&mut self, member: &'static str, body: &B, why: Internal) {
        let msg = Message::method_call(DAEMON_PATH, member)
            .with_destination(DAEMON)
            .with_interface(DAEMON)
            .with_body(body);
        let serial = self.queue(&msg);
        self.pending.insert(serial, Pending::Internal(why));
    }

    fn daemon_call_no_reply<B: Body>(&mut self, member: &'static str, body: &B) {
        let msg = Message::method_call(DAEMON_PATH, member)
            .with_destination(DAEMON)
            .with_interface(DAEMON)
            .with_flags(Flags::NO_REPLY_EXPECTED)
            .with_body(body);
        self.queue(&msg);
    }

    pub(crate) fn add_match(&mut self, rule: MatchRule) -> MatchId {
        let n = self.matches.next;
        self.matches.next += 1;
        let key = rule.to_rule_string();
        self.matches.ids.insert(n, key.clone());
        let fresh = !self.matches.rules.contains_key(&key);
        let sender = rule.sender_name().cloned();
        self.matches
            .rules
            .entry(key.clone())
            .or_insert(Entry {
                rule,
                ids: Vec::new(),
            })
            .ids
            .push(n);
        if fresh {
            self.daemon_call(
                "AddMatch",
                &(key.as_str(),),
                Internal::AddMatch(key.clone()),
            );
            if let Some(s) = sender.filter(|s| !s.is_unique() && s.as_str() != DAEMON) {
                self.track_owner(s);
            }
        }
        MatchId { bus: self.id, n }
    }

    pub(crate) fn remove_match(&mut self, id: MatchId) {
        let Some(key) = self.matches.ids.remove(&id.n) else {
            return;
        };
        let entry = self
            .matches
            .rules
            .get_mut(&key)
            .expect("dbus: an id's rule exists");
        entry.ids.retain(|&x| x != id.n);
        if !entry.ids.is_empty() {
            return;
        }
        let entry = self.matches.rules.remove(&key).unwrap();
        self.daemon_call_no_reply("RemoveMatch", &(key.as_str(),));
        if let Some(s) = entry
            .rule
            .sender_name()
            .filter(|s| !s.is_unique() && s.as_str() != DAEMON)
        {
            self.untrack_owner(s.clone());
        }
    }

    /// Follow `name`'s owner: a `NameOwnerChanged` rule first, then
    /// `GetNameOwner`, so the daemon answers both before anything sent
    /// after them and no later signal can beat the owner.
    fn track_owner(&mut self, name: BusName) {
        let owner = self.matches.owners.entry(name.clone()).or_insert(Owner {
            refs: 0,
            unique: None,
        });
        owner.refs += 1;
        let first = owner.refs == 1;
        if first {
            let rule = owner_rule(&name);
            self.daemon_call("AddMatch", &(rule.as_str(),), Internal::OwnerMatch);
            self.daemon_call(
                "GetNameOwner",
                &(name.as_str(),),
                Internal::GetNameOwner(name.clone()),
            );
        }
    }

    fn untrack_owner(&mut self, name: BusName) {
        let Some(owner) = self.matches.owners.get_mut(&name) else {
            return;
        };
        owner.refs -= 1;
        if owner.refs == 0 {
            self.matches.owners.remove(&name);
            let rule = owner_rule(&name);
            self.daemon_call_no_reply("RemoveMatch", &(rule.as_str(),));
        }
    }

    /// The daemon's own signals: owner changes for followed names, and
    /// names this connection gained or lost.
    fn daemon_signal(&mut self, msg: &Message) -> Vec<Out> {
        if msg.sender().map(BusName::as_str) != Some(DAEMON)
            || msg.interface().map(InterfaceName::as_str) != Some(DAEMON)
        {
            return Vec::new();
        }
        let bus = self.id;
        match msg.member().map(MemberName::as_str) {
            Some("NameOwnerChanged") => {
                if let Ok((name, _, new)) = msg.body::<(String, String, String)>() {
                    let followed = BusName::new(&name)
                        .ok()
                        .and_then(|n| self.matches.owners.get_mut(&n));
                    if let Some(owner) = followed {
                        owner.unique = BusName::new(&new).ok();
                    }
                }
                Vec::new()
            }
            Some("NameAcquired") => msg
                .body::<(BusName,)>()
                .map(|(name,)| vec![out(NameAcquired { bus, name })])
                .unwrap_or_default(),
            Some("NameLost") => msg
                .body::<(BusName,)>()
                .map(|(name,)| vec![out(NameLost { bus, name })])
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn hello(&mut self) {
        let msg = Message::method_call(DAEMON_PATH, "Hello")
            .with_destination(DAEMON)
            .with_interface(DAEMON);
        let serial = self.queue(&msg);
        self.pending
            .insert(serial, Pending::Internal(Internal::Hello));
    }

    /// Mark the connection dead. Task 12 makes this fail pending calls
    /// and raise `BusDisconnected`.
    pub(crate) fn die(&mut self) -> Vec<Out> {
        if self.dead {
            return Vec::new();
        }
        self.dead = true;
        self.outbox.clear();
        Vec::new()
    }
}

/// The rule that follows `name`'s owner.
fn owner_rule(name: &BusName) -> String {
    MatchRule::signal()
        .sender(DAEMON)
        .interface(DAEMON)
        .member("NameOwnerChanged")
        .arg(0, name.as_str())
        .to_rule_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> (Connection, UnixStream) {
        let (a, b) = UnixStream::pair().unwrap();
        (Connection::new(BusId::Session, a, true), b)
    }

    #[test]
    fn serials_skip_zero_when_they_wrap() {
        let (mut c, _peer) = conn();
        c.next_serial = u32::MAX;
        assert_eq!(c.serial(), u32::MAX);
        assert_eq!(c.serial(), 1);
    }

    #[test]
    fn a_partial_send_keeps_the_unsent_tail_first() {
        let (mut c, _peer) = conn();
        let mut ring = Ring::new(4);
        let t = Token::from_raw(7);
        c.sending = Some(t);
        c.outbox.push_back((b"next".to_vec(), Vec::new()));
        let outs = c.on_io(
            &IoEvent {
                token: t,
                result: 3,
            },
            Completed {
                buf: b"abcdef".to_vec(),
                fds: Vec::new(),
            },
            &mut ring,
        );
        assert!(outs.is_empty());
        assert!(c.sending.is_none());
        let queued: Vec<&[u8]> = c.outbox.iter().map(|(b, _)| b.as_slice()).collect();
        assert_eq!(queued, [b"def".as_slice(), b"next".as_slice()]);
    }

    #[test]
    fn a_refused_hello_kills_the_connection() {
        let (mut c, _peer) = conn();
        c.hello();
        let reply = Message::error(1, "org.freedesktop.DBus.Error.AccessDenied", "no");
        c.handle(reply);
        assert!(c.dead);
        assert!(c.unique_name.is_none());
    }

    #[test]
    fn a_hello_reply_without_a_name_kills_the_connection() {
        let (mut c, _peer) = conn();
        c.hello();
        c.handle(Message::method_return(1).with_body(&(1u32,)));
        assert!(c.dead);
        assert!(c.unique_name.is_none());
    }
}
