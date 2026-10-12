#![forbid(unsafe_code)]
//! D-Bus on the ring. The crate docs are written in Task 14.

mod fd;
mod message;
mod names;
mod signature;
mod types;
mod value;
mod wire;

pub use fd::UnixFd;
pub use message::{Flags, MAX_MESSAGE, Message, MessageKind};
pub use names::{BusName, ErrorName, InterfaceName, MemberName, NameError, ObjectPath};
pub use signature::Signature;
pub use types::{Basic, Body, Decode, Encode, FromBody, Type, from_bytes, to_bytes};
pub use value::{PropMap, PropMapExt, Value};
pub use wire::{DecodeError, Decoder, Encoder, MAX_ARRAY, MAX_DEPTH};
