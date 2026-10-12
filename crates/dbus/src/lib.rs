#![forbid(unsafe_code)]
//! D-Bus on the ring. The crate docs are written in Task 14.

mod fd;
mod names;
mod signature;
mod types;
mod wire;

pub use fd::UnixFd;
pub use names::{BusName, ErrorName, InterfaceName, MemberName, NameError, ObjectPath};
pub use signature::Signature;
pub use types::{Basic, Body, Decode, Encode, FromBody, Type, from_bytes, to_bytes};
pub use wire::{DecodeError, Decoder, Encoder, MAX_ARRAY, MAX_DEPTH};
