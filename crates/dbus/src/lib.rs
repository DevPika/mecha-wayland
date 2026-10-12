#![forbid(unsafe_code)]
//! D-Bus on the ring. The crate docs are written in Task 14.

mod address;
mod auth;
mod bus;
mod client;
mod connection;
mod fd;
mod matching;
mod message;
mod names;
mod server;
mod signals;
mod signature;
mod standard;
mod types;
mod value;
mod wire;

#[cfg(feature = "fake")]
pub mod fake;

pub use bus::{Bus, BusId, DBus, DBusModule, NameFlags};
pub use client::{CallError, CallId, DEFAULT_TIMEOUT, MethodCall, ObjectRef};
pub use fd::UnixFd;
pub use matching::{MatchId, MatchRule, PropertiesChanged};
pub use message::{Flags, MAX_MESSAGE, Message, MessageKind};
pub use names::{BusName, ErrorName, InterfaceName, MemberName, NameError, ObjectPath};
pub use server::{Access, ExportError, ExportId, IncomingCall, InterfaceSpec, PropertyError};
pub use signals::{
    MatchFailed, MethodCallReceived, MethodReply, NameAcquired, NameLost, PropertySetRequested,
    SignalReceived,
};
pub use signature::Signature;
pub use types::{Basic, Body, Decode, Encode, FromBody, Type, from_bytes, to_bytes};
pub use value::{PropMap, PropMapExt, Value};
pub use wire::{DecodeError, Decoder, Encoder, MAX_ARRAY, MAX_DEPTH};
