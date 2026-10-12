//! The marshalling primitives: an [`Encoder`] that appends aligned,
//! little-endian values, and a [`Decoder`] that reads either endianness
//! and never panics on what a peer sent.

use std::fmt;

use crate::fd::UnixFd;
use crate::names::NameError;
use crate::signature;

/// The longest array the specification allows, in bytes.
pub const MAX_ARRAY: usize = 64 * 1024 * 1024;
/// The deepest container nesting a decoder follows, variants included.
pub const MAX_DEPTH: u32 = 64;

/// Why bytes from a peer did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes ended inside a value.
    Truncated,
    /// Alignment padding was not zero.
    Padding,
    /// A string was not UTF-8, held a NUL, or lacked its terminator.
    BadString,
    /// A boolean other than 0 or 1.
    BadBool(u32),
    /// A name, path or signature failed validation.
    Name(NameError),
    /// Containers nested deeper than [`MAX_DEPTH`].
    TooDeep,
    /// An array length over [`MAX_ARRAY`].
    ArrayTooLong(usize),
    /// An array's elements ran past its length.
    ArrayOverrun,
    /// An `h` index with no fd behind it.
    FdIndex(u32),
    /// Bytes left after the expected values.
    Trailing(usize),
    /// The signature is not the one asked for.
    Mismatch { expected: String, found: String },
    /// A variant holding other than one complete type.
    BadVariant(String),
    /// The fixed header or its fields are wrong.
    BadHeader(&'static str),
    /// A message over the specification's 128 MiB.
    TooLarge(usize),
    /// The header declared more fds than arrived with the message.
    MissingFds { declared: u32, available: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DecodeError {}

impl From<NameError> for DecodeError {
    fn from(e: NameError) -> Self {
        DecodeError::Name(e)
    }
}

/// Appends values with D-Bus alignment, little-endian. Offsets count
/// from the buffer's start, which is 8-aligned in the message: a
/// header's start or a body's.
#[derive(Default)]
pub struct Encoder {
    buf: Vec<u8>,
    fds: Vec<UnixFd>,
}

impl Encoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// The bytes, and the fds the `h` values index.
    pub fn into_parts(self) -> (Vec<u8>, Vec<UnixFd>) {
        (self.buf, self.fds)
    }

    pub fn align(&mut self, n: usize) {
        let to = self.buf.len().next_multiple_of(n);
        self.buf.resize(to, 0);
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.u32(v as u32);
    }

    pub fn i16(&mut self, v: i16) {
        self.align(2);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u16(&mut self, v: u16) {
        self.align(2);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.align(8);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.align(8);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.align(8);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// A string (`s`, `o`). Panics if it holds a NUL, which D-Bus cannot
    /// carry: passing one is a caller bug.
    pub fn str(&mut self, s: &str) {
        assert!(
            !s.contains('\0'),
            "dbus: a string with a NUL cannot be sent: {s:?}"
        );
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    /// A signature (`g`): one length byte, the codes, a NUL.
    pub fn signature(&mut self, s: &str) {
        self.buf.push(s.len() as u8);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }

    /// An fd (`h`): its index among the fds this encoder collected.
    pub fn fd(&mut self, fd: &UnixFd) {
        let i = self.fds.len() as u32;
        self.fds.push(fd.clone());
        self.u32(i);
    }

    /// An array whose elements align to `elem_align`; `f` writes them.
    /// The padding after the length is there even when `f` writes none.
    pub fn array(&mut self, elem_align: usize, f: impl FnOnce(&mut Encoder)) {
        self.align(4);
        let at = self.buf.len();
        self.buf.extend_from_slice(&[0; 4]);
        self.align(elem_align);
        let start = self.buf.len();
        f(self);
        let n = (self.buf.len() - start) as u32;
        self.buf[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }

    /// A struct or dict entry: 8-aligned, then what `f` writes.
    pub fn structure(&mut self, f: impl FnOnce(&mut Encoder)) {
        self.align(8);
        f(self);
    }
}

/// Reads values written by any peer, in its endianness. Every read
/// returns `Err` rather than panicking on bad input.
pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    big: bool,
    fds: &'a [UnixFd],
    depth: u32,
}

macro_rules! read_num {
    ($name:ident, $t:ty, $n:literal) => {
        pub fn $name(&mut self) -> Result<$t, DecodeError> {
            self.align($n)?;
            let b: [u8; $n] = self.take($n)?.try_into().unwrap();
            Ok(if self.big {
                <$t>::from_be_bytes(b)
            } else {
                <$t>::from_le_bytes(b)
            })
        }
    };
}

impl<'a> Decoder<'a> {
    pub fn new(buf: &'a [u8], big_endian: bool, fds: &'a [UnixFd]) -> Self {
        Decoder {
            buf,
            pos: 0,
            big: big_endian,
            fds,
            depth: 0,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos == self.buf.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    /// Skip to a multiple of `n`; the skipped bytes must be zero.
    pub fn align(&mut self, n: usize) -> Result<(), DecodeError> {
        let pad = self.pos.next_multiple_of(n) - self.pos;
        if self.take(pad)?.iter().any(|&b| b != 0) {
            return Err(DecodeError::Padding);
        }
        Ok(())
    }

    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    read_num!(i16, i16, 2);
    read_num!(u16, u16, 2);
    read_num!(i32, i32, 4);
    read_num!(u32, u32, 4);
    read_num!(i64, i64, 8);
    read_num!(u64, u64, 8);
    read_num!(f64, f64, 8);

    pub fn bool(&mut self) -> Result<bool, DecodeError> {
        match self.u32()? {
            0 => Ok(false),
            1 => Ok(true),
            n => Err(DecodeError::BadBool(n)),
        }
    }

    pub fn str(&mut self) -> Result<&'a str, DecodeError> {
        let n = self.u32()? as usize;
        let b = self.take(n)?;
        if self.u8()? != 0 {
            return Err(DecodeError::BadString);
        }
        let s = std::str::from_utf8(b).map_err(|_| DecodeError::BadString)?;
        if s.contains('\0') {
            return Err(DecodeError::BadString);
        }
        Ok(s)
    }

    pub fn signature(&mut self) -> Result<&'a str, DecodeError> {
        let n = self.u8()? as usize;
        let b = self.take(n)?;
        if self.u8()? != 0 {
            return Err(DecodeError::BadString);
        }
        let s = std::str::from_utf8(b).map_err(|_| DecodeError::BadString)?;
        if !signature::valid(s) {
            return Err(DecodeError::Name(NameError {
                kind: "signature",
                input: s.to_owned(),
            }));
        }
        Ok(s)
    }

    pub fn fd(&mut self) -> Result<UnixFd, DecodeError> {
        let i = self.u32()?;
        self.fds
            .get(i as usize)
            .cloned()
            .ok_or(DecodeError::FdIndex(i))
    }

    /// An array whose elements align to `elem_align`; `f` reads one
    /// element per call until the array's length is used up.
    pub fn array(
        &mut self,
        elem_align: usize,
        mut f: impl FnMut(&mut Self) -> Result<(), DecodeError>,
    ) -> Result<(), DecodeError> {
        let n = self.u32()? as usize;
        if n > MAX_ARRAY {
            return Err(DecodeError::ArrayTooLong(n));
        }
        self.align(elem_align)?;
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or(DecodeError::Truncated)?;
        self.enter()?;
        while self.pos < end {
            f(self)?;
        }
        self.leave();
        if self.pos != end {
            return Err(DecodeError::ArrayOverrun);
        }
        Ok(())
    }

    /// A struct or dict entry: 8-aligned, then what `f` reads.
    pub fn structure<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, DecodeError>,
    ) -> Result<T, DecodeError> {
        self.align(8)?;
        self.enter()?;
        let v = f(self)?;
        self.leave();
        Ok(v)
    }

    pub(crate) fn enter(&mut self) -> Result<(), DecodeError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(DecodeError::TooDeep);
        }
        Ok(())
    }

    pub(crate) fn leave(&mut self) {
        self.depth -= 1;
    }
}
