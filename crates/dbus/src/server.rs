//! The serving side: what an exported object declares, and the calls
//! that reach it.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::{Rc, Weak};

use crate::bus::BusId;
use crate::client::PROPERTIES;
use crate::message::{Flags, Message};
use crate::names::{BusName, InterfaceName, MemberName, ObjectPath};
use crate::signature::Signature;
use crate::types::FromBody;
use crate::value::Value;
use crate::wire::DecodeError;

pub(crate) mod errors {
    pub const UNKNOWN_OBJECT: &str = "org.freedesktop.DBus.Error.UnknownObject";
    pub const UNKNOWN_INTERFACE: &str = "org.freedesktop.DBus.Error.UnknownInterface";
    pub const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";
    pub const INVALID_ARGS: &str = "org.freedesktop.DBus.Error.InvalidArgs";
    pub const UNKNOWN_PROPERTY: &str = "org.freedesktop.DBus.Error.UnknownProperty";
    pub const PROPERTY_READ_ONLY: &str = "org.freedesktop.DBus.Error.PropertyReadOnly";
    pub const NO_REPLY: &str = "org.freedesktop.DBus.Error.NoReply";
    pub const FAILED: &str = "org.freedesktop.DBus.Error.Failed";
}

pub(crate) const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";
pub(crate) const PEER: &str = "org.freedesktop.DBus.Peer";
/// Answered by the library on every path; never exported by the app.
pub(crate) const STANDARD: [&str; 3] = [INTROSPECTABLE, PEER, PROPERTIES];

/// Who may change a property from the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    /// A peer's `Set` arrives as `PropertySetRequested`.
    ReadWrite,
}

#[derive(Debug, Clone)]
pub(crate) struct MethodSpec {
    pub(crate) name: MemberName,
    pub(crate) input: Signature,
    pub(crate) output: Signature,
}

#[derive(Debug, Clone)]
pub(crate) struct PropertySpec {
    pub(crate) name: MemberName,
    pub(crate) sig: Signature,
    pub(crate) access: Access,
    pub(crate) initial: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct SignalSpec {
    pub(crate) name: MemberName,
    pub(crate) sig: Signature,
}

/// One interface an object implements: its methods with their argument
/// signatures, its properties with initial values, and its signals.
#[derive(Debug, Clone)]
pub struct InterfaceSpec {
    pub(crate) name: InterfaceName,
    pub(crate) methods: Vec<MethodSpec>,
    pub(crate) properties: Vec<PropertySpec>,
    pub(crate) signals: Vec<SignalSpec>,
}

impl InterfaceSpec {
    pub fn new(name: impl Into<InterfaceName>) -> Self {
        InterfaceSpec {
            name: name.into(),
            methods: Vec::new(),
            properties: Vec::new(),
            signals: Vec::new(),
        }
    }

    /// A method taking `input` and returning `output` (body signatures).
    pub fn method(
        mut self,
        name: impl Into<MemberName>,
        input: impl Into<Signature>,
        output: impl Into<Signature>,
    ) -> Self {
        self.methods.push(MethodSpec {
            name: name.into(),
            input: input.into(),
            output: output.into(),
        });
        self
    }

    /// # Panics
    ///
    /// If `initial` does not have the signature `sig`: a setup bug.
    pub fn property(
        mut self,
        name: impl Into<MemberName>,
        sig: impl Into<Signature>,
        access: Access,
        initial: Value,
    ) -> Self {
        let name = name.into();
        let sig = sig.into();
        assert_eq!(
            initial.signature(),
            sig,
            "dbus: property {name}'s initial value is not a {sig}"
        );
        self.properties.push(PropertySpec {
            name,
            sig,
            access,
            initial,
        });
        self
    }

    pub fn signal(mut self, name: impl Into<MemberName>, sig: impl Into<Signature>) -> Self {
        self.signals.push(SignalSpec {
            name: name.into(),
            sig: sig.into(),
        });
        self
    }
}

/// Names one `export`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExportId {
    pub bus: BusId,
    pub(crate) n: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// That path already exports that interface.
    AlreadyExported,
    /// Introspectable, Peer and Properties are the library's.
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyError {
    UnknownExport,
    UnknownProperty,
    Signature {
        expected: Signature,
        found: Signature,
    },
}

/// Calls dropped unanswered: the caller's name and the call's serial.
pub(crate) type Unanswered = Rc<RefCell<Vec<(Option<BusName>, u32)>>>;

/// One exported interface at one path, and its property values.
pub(crate) struct Export {
    pub(crate) path: ObjectPath,
    pub(crate) spec: InterfaceSpec,
    pub(crate) values: BTreeMap<String, Value>,
    /// Properties changed since the last `BeforeWait`.
    pub(crate) changed: BTreeSet<String>,
}

impl Export {
    pub(crate) fn new(path: ObjectPath, spec: InterfaceSpec) -> Self {
        let values = spec
            .properties
            .iter()
            .map(|p| (p.name.as_str().to_owned(), p.initial.clone()))
            .collect();
        Export {
            path,
            spec,
            values,
            changed: BTreeSet::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Server {
    pub(crate) next: u64,
    pub(crate) exports: BTreeMap<u64, Export>,
    pub(crate) unanswered: Unanswered,
}

impl Server {
    pub(crate) fn export_at(&self, path: &ObjectPath, interface: &str) -> Option<(u64, &Export)> {
        self.exports
            .iter()
            .find(|(_, e)| e.path == *path && e.spec.name.as_str() == interface)
            .map(|(n, e)| (*n, e))
    }

    pub(crate) fn at_path<'a>(
        &'a self,
        path: &'a ObjectPath,
    ) -> impl Iterator<Item = (u64, &'a Export)> {
        self.exports
            .iter()
            .filter(move |(_, e)| e.path == *path)
            .map(|(n, e)| (*n, e))
    }
}

/// One call to an exported object. Clone it to answer later with
/// `DBus::reply` or `DBus::reply_error`. When the last clone drops
/// unanswered, the caller gets `org.freedesktop.DBus.Error.NoReply`
/// (unless it asked for no reply).
#[derive(Clone)]
pub struct IncomingCall(Rc<CallInner>);

struct CallInner {
    bus: BusId,
    msg: Message,
    answered: Cell<bool>,
    unanswered: Weak<RefCell<Vec<(Option<BusName>, u32)>>>,
}

impl Drop for CallInner {
    fn drop(&mut self) {
        if self.answered.get() || self.msg.flags().contains(Flags::NO_REPLY_EXPECTED) {
            return;
        }
        if let Some(queue) = self.unanswered.upgrade() {
            queue
                .borrow_mut()
                .push((self.msg.sender().cloned(), self.msg.serial()));
        }
    }
}

impl IncomingCall {
    pub(crate) fn new(bus: BusId, msg: Message, unanswered: &Unanswered) -> Self {
        IncomingCall(Rc::new(CallInner {
            bus,
            msg,
            answered: Cell::new(false),
            unanswered: Rc::downgrade(unanswered),
        }))
    }

    pub fn bus(&self) -> BusId {
        self.0.bus
    }

    pub fn message(&self) -> &Message {
        &self.0.msg
    }

    pub fn member(&self) -> &MemberName {
        self.0
            .msg
            .member()
            .expect("dbus: a method call has a member")
    }

    pub fn sender(&self) -> Option<&BusName> {
        self.0.msg.sender()
    }

    /// The arguments as `T`.
    pub fn args<T: FromBody>(&self) -> Result<T, DecodeError> {
        self.0.msg.body()
    }

    pub fn is_answered(&self) -> bool {
        self.0.answered.get()
    }

    pub(crate) fn no_reply(&self) -> bool {
        self.0.msg.flags().contains(Flags::NO_REPLY_EXPECTED)
    }

    /// Mark answered; `true` if this is the first answer.
    pub(crate) fn take_answer(&self) -> bool {
        !self.0.answered.replace(true)
    }
}

impl fmt::Debug for IncomingCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IncomingCall")
            .field("bus", &self.0.bus)
            .field("member", &self.0.msg.member())
            .field("serial", &self.0.msg.serial())
            .field("answered", &self.0.answered.get())
            .finish()
    }
}
