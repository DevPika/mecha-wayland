//! One bus connection: the socket, what is waiting to go out, what came
//! in and is not yet a whole message, and every call still owed a reply.
//! Methods return the signals to raise as [`Out`]s; they never touch the
//! `App`.

use std::collections::{HashMap, VecDeque};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;

use ring::{Completed, IoEvent, Ring, Token};

use crate::bus::BusId;
use crate::message::{Message, MessageKind};
use crate::names::BusName;
use crate::signals::Out;

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
}

pub(crate) enum Internal {
    Hello,
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
        self.reading == Some(t) || self.sending == Some(t)
    }

    pub(crate) fn before_wait(&mut self, ring: &mut Ring) -> Vec<Out> {
        self.flush(ring);
        Vec::new()
    }

    pub(crate) fn on_io(&mut self, e: &IoEvent, done: Completed, ring: &mut Ring) -> Vec<Out> {
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
                    None => Vec::new(),
                }
            }
            _ => Vec::new(),
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
