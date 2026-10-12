//! The typed side: [`Type`], [`Encode`] and [`Decode`] for Rust values,
//! and [`Body`] / [`FromBody`] for a message's top-level arguments.

use std::collections::{BTreeMap, HashMap};
use std::hash::{BuildHasher, Hash};

use crate::fd::UnixFd;
use crate::names::{BusName, ErrorName, InterfaceName, MemberName, ObjectPath};
use crate::signature::{self, Signature};
use crate::wire::{DecodeError, Decoder, Encoder};

/// A type with a D-Bus signature.
pub trait Type {
    /// Append this type's signature to `out`.
    fn sig(out: &mut String);

    fn signature() -> Signature {
        let mut s = String::new();
        Self::sig(&mut s);
        Signature::from_valid(s)
    }
}

/// A value that can be written.
pub trait Encode: Type {
    fn encode(&self, enc: &mut Encoder);
}

/// A value that can be read.
pub trait Decode: Type + Sized {
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError>;
}

/// A basic type: the only kind a dict key may be.
pub trait Basic: Type {}

/// A message body to send: a tuple of [`Encode`] values, one per
/// top-level argument. `("a", 1u32)` is the body `su`, not `(su)`; `()`
/// is the empty body.
pub trait Body {
    fn body_sig(out: &mut String);
    fn encode_body(&self, enc: &mut Encoder);

    fn body_signature() -> Signature
    where
        Self: Sized,
    {
        let mut s = String::new();
        Self::body_sig(&mut s);
        Signature::from_valid(s)
    }
}

/// A message body to read: a tuple of [`Decode`] values.
pub trait FromBody: Sized {
    fn body_sig(out: &mut String);
    fn decode_body(dec: &mut Decoder<'_>) -> Result<Self, DecodeError>;

    fn body_signature() -> Signature {
        let mut s = String::new();
        Self::body_sig(&mut s);
        Signature::from_valid(s)
    }
}

/// The alignment of `T`'s first type code.
pub(crate) fn align_of<T: Type + ?Sized>() -> usize {
    let mut s = String::new();
    T::sig(&mut s);
    signature::alignment(s.as_bytes()[0])
}

/// One value encoded on its own, as at the start of a body.
pub fn to_bytes<T: Encode + ?Sized>(v: &T) -> (Vec<u8>, Vec<UnixFd>) {
    let mut e = Encoder::new();
    v.encode(&mut e);
    e.into_parts()
}

/// One value of type `T` that fills `bytes` exactly.
pub fn from_bytes<T: Decode>(bytes: &[u8], fds: &[UnixFd]) -> Result<T, DecodeError> {
    let mut d = Decoder::new(bytes, false, fds);
    let v = T::decode(&mut d)?;
    if !d.is_empty() {
        return Err(DecodeError::Trailing(d.remaining()));
    }
    Ok(v)
}

macro_rules! basic {
    ($t:ty, $c:literal, $m:ident) => {
        impl Type for $t {
            fn sig(out: &mut String) {
                out.push($c);
            }
        }
        impl Basic for $t {}
        impl Encode for $t {
            fn encode(&self, enc: &mut Encoder) {
                enc.$m(*self);
            }
        }
        impl Decode for $t {
            fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
                dec.$m()
            }
        }
    };
}

basic!(u8, 'y', u8);
basic!(bool, 'b', bool);
basic!(i16, 'n', i16);
basic!(u16, 'q', u16);
basic!(i32, 'i', i32);
basic!(u32, 'u', u32);
basic!(i64, 'x', i64);
basic!(u64, 't', u64);
basic!(f64, 'd', f64);

impl Type for str {
    fn sig(out: &mut String) {
        out.push('s');
    }
}
impl Basic for str {}
impl Encode for str {
    fn encode(&self, enc: &mut Encoder) {
        enc.str(self);
    }
}

impl Type for String {
    fn sig(out: &mut String) {
        out.push('s');
    }
}
impl Basic for String {}
impl Encode for String {
    fn encode(&self, enc: &mut Encoder) {
        enc.str(self);
    }
}
impl Decode for String {
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(dec.str()?.to_owned())
    }
}

impl<T: Type + ?Sized> Type for &T {
    fn sig(out: &mut String) {
        T::sig(out);
    }
}
impl<T: Basic + ?Sized> Basic for &T {}
impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, enc: &mut Encoder) {
        (**self).encode(enc);
    }
}

macro_rules! string_name {
    ($t:ty, $c:literal) => {
        impl Type for $t {
            fn sig(out: &mut String) {
                out.push($c);
            }
        }
        impl Basic for $t {}
        impl Encode for $t {
            fn encode(&self, enc: &mut Encoder) {
                enc.str(self.as_str());
            }
        }
        impl Decode for $t {
            fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
                Ok(<$t>::new(dec.str()?)?)
            }
        }
    };
}

string_name!(ObjectPath, 'o');
string_name!(BusName, 's');
string_name!(InterfaceName, 's');
string_name!(MemberName, 's');
string_name!(ErrorName, 's');

impl Type for Signature {
    fn sig(out: &mut String) {
        out.push('g');
    }
}
impl Basic for Signature {}
impl Encode for Signature {
    fn encode(&self, enc: &mut Encoder) {
        enc.signature(self.as_str());
    }
}
impl Decode for Signature {
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(Signature::from_valid(dec.signature()?.to_owned()))
    }
}

impl Type for UnixFd {
    fn sig(out: &mut String) {
        out.push('h');
    }
}
impl Basic for UnixFd {}
impl Encode for UnixFd {
    fn encode(&self, enc: &mut Encoder) {
        enc.fd(self);
    }
}
impl Decode for UnixFd {
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        dec.fd()
    }
}

impl<T: Type> Type for [T] {
    fn sig(out: &mut String) {
        out.push('a');
        T::sig(out);
    }
}
impl<T: Encode> Encode for [T] {
    fn encode(&self, enc: &mut Encoder) {
        enc.array(align_of::<T>(), |e| {
            for x in self {
                x.encode(e);
            }
        });
    }
}

impl<T: Type> Type for Vec<T> {
    fn sig(out: &mut String) {
        out.push('a');
        T::sig(out);
    }
}
impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, enc: &mut Encoder) {
        self.as_slice().encode(enc);
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let mut out = Vec::new();
        dec.array(align_of::<T>(), |d| {
            out.push(T::decode(d)?);
            Ok(())
        })?;
        Ok(out)
    }
}

impl<K: Basic, V: Type, S> Type for HashMap<K, V, S> {
    fn sig(out: &mut String) {
        out.push_str("a{");
        K::sig(out);
        V::sig(out);
        out.push('}');
    }
}
impl<K: Basic + Encode, V: Encode, S> Encode for HashMap<K, V, S> {
    fn encode(&self, enc: &mut Encoder) {
        enc.array(8, |e| {
            for (k, v) in self {
                e.structure(|e| {
                    k.encode(e);
                    v.encode(e);
                });
            }
        });
    }
}
impl<K, V, S> Decode for HashMap<K, V, S>
where
    K: Basic + Decode + Eq + Hash,
    V: Decode,
    S: BuildHasher + Default,
{
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let mut out = HashMap::with_hasher(S::default());
        dec.array(8, |d| {
            d.structure(|d| {
                let k = K::decode(d)?;
                let v = V::decode(d)?;
                out.insert(k, v);
                Ok(())
            })
        })?;
        Ok(out)
    }
}

impl<K: Basic, V: Type> Type for BTreeMap<K, V> {
    fn sig(out: &mut String) {
        out.push_str("a{");
        K::sig(out);
        V::sig(out);
        out.push('}');
    }
}
impl<K: Basic + Encode, V: Encode> Encode for BTreeMap<K, V> {
    fn encode(&self, enc: &mut Encoder) {
        enc.array(8, |e| {
            for (k, v) in self {
                e.structure(|e| {
                    k.encode(e);
                    v.encode(e);
                });
            }
        });
    }
}
impl<K: Basic + Decode + Ord, V: Decode> Decode for BTreeMap<K, V> {
    fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let mut out = BTreeMap::new();
        dec.array(8, |d| {
            d.structure(|d| {
                let k = K::decode(d)?;
                let v = V::decode(d)?;
                out.insert(k, v);
                Ok(())
            })
        })?;
        Ok(out)
    }
}

impl Body for () {
    fn body_sig(_: &mut String) {}
    fn encode_body(&self, _: &mut Encoder) {}
}

impl FromBody for () {
    fn body_sig(_: &mut String) {}
    fn decode_body(_: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        Ok(())
    }
}

macro_rules! tuple {
    ($($n:tt $t:ident),+) => {
        impl<$($t: Type),+> Type for ($($t,)+) {
            fn sig(out: &mut String) {
                out.push('(');
                $($t::sig(out);)+
                out.push(')');
            }
        }
        impl<$($t: Encode),+> Encode for ($($t,)+) {
            fn encode(&self, enc: &mut Encoder) {
                enc.structure(|e| {
                    $(self.$n.encode(e);)+
                });
            }
        }
        impl<$($t: Decode),+> Decode for ($($t,)+) {
            fn decode(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
                dec.structure(|d| Ok(($($t::decode(d)?,)+)))
            }
        }
        impl<$($t: Encode),+> Body for ($($t,)+) {
            fn body_sig(out: &mut String) {
                $($t::sig(out);)+
            }
            fn encode_body(&self, enc: &mut Encoder) {
                $(self.$n.encode(enc);)+
            }
        }
        impl<$($t: Decode),+> FromBody for ($($t,)+) {
            fn body_sig(out: &mut String) {
                $($t::sig(out);)+
            }
            fn decode_body(dec: &mut Decoder<'_>) -> Result<Self, DecodeError> {
                Ok(($($t::decode(dec)?,)+))
            }
        }
    };
}

tuple!(0 A);
tuple!(0 A, 1 B);
tuple!(0 A, 1 B, 2 C);
tuple!(0 A, 1 B, 2 C, 3 D);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I, 9 J);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I, 9 J, 10 K);
tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H, 8 I, 9 J, 10 K, 11 L);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::MAX_ARRAY;
    use std::fmt::Debug;

    fn round<T: Encode + Decode + PartialEq + Debug>(v: T) {
        let (b, f) = to_bytes(&v);
        assert_eq!(from_bytes::<T>(&b, &f).unwrap(), v);
    }

    #[test]
    fn basics_round_trip() {
        round(7u8);
        round(true);
        round(-3i16);
        round(9u16);
        round(-70000i32);
        round(70000u32);
        round(-1i64 << 40);
        round(1u64 << 50);
        round(1.5f64);
        round("héllo".to_string());
        round(ObjectPath::from("/a/b"));
        round(BusName::from(":1.4"));
        round(Signature::from("a{sv}"));
    }

    #[test]
    fn containers_round_trip() {
        round(vec![1u32, 2, 3]);
        round(Vec::<u64>::new());
        round(vec![(1u8, 2u64)]);
        round(BTreeMap::from([("a".to_string(), 1u32)]));
        round(HashMap::from([(1u8, vec!["x".to_string()])]));
        round((1u8, (2u16, "s".to_string()), vec![true]));
    }

    #[test]
    fn signatures_of_types() {
        assert_eq!(
            <HashMap<String, Vec<(u8, ObjectPath)>>>::signature(),
            "a{sa(yo)}"
        );
        assert_eq!(<(u8, u64)>::signature(), "(yt)");
        assert_eq!(<(String, u32) as Body>::body_signature(), "su");
        assert_eq!(<(String, u32) as FromBody>::body_signature(), "su");
        assert_eq!(<() as Body>::body_signature(), "");
        assert_eq!(<(&str, &[u8])>::signature(), "(say)");
    }

    #[test]
    fn alignment_is_exact() {
        // (yt): y at 0, seven bytes of padding, t at 8.
        let (b, _) = to_bytes(&(1u8, 2u64));
        assert_eq!(b, [1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
        // An empty array of 8-aligned elements still pads after its length.
        let (b, _) = to_bytes(&Vec::<u64>::new());
        assert_eq!(b, [0, 0, 0, 0, 0, 0, 0, 0]);
        // (yay): y, padding to 4, the length, then the bytes.
        let (b, _) = to_bytes(&(1u8, vec![9u8, 8]));
        assert_eq!(b, [1, 0, 0, 0, 2, 0, 0, 0, 9, 8]);
    }

    #[test]
    fn fds_are_indexed() {
        let (r, _w) = std::io::pipe().unwrap();
        let fd = UnixFd::new(r.into());
        let (b, f) = to_bytes(&(fd.clone(), fd.clone()));
        assert_eq!(b, [0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(f.len(), 2);
        let back: (UnixFd, UnixFd) = from_bytes(&b, &f).unwrap();
        assert_eq!(back.0, f[0]);
        assert_eq!(back.1, f[1]);
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        assert_eq!(from_bytes::<u32>(&[1, 0], &[]), Err(DecodeError::Truncated));
        assert_eq!(
            from_bytes::<bool>(&2u32.to_le_bytes(), &[]),
            Err(DecodeError::BadBool(2))
        );
        assert_eq!(
            from_bytes::<(u8, u32)>(&[1, 9, 0, 0, 5, 0, 0, 0], &[]),
            Err(DecodeError::Padding)
        );

        let mut no_nul = 2u32.to_le_bytes().to_vec();
        no_nul.extend_from_slice(b"hi!");
        assert_eq!(
            from_bytes::<String>(&no_nul, &[]),
            Err(DecodeError::BadString)
        );

        let mut not_utf8 = 1u32.to_le_bytes().to_vec();
        not_utf8.extend_from_slice(&[0xff, 0]);
        assert_eq!(
            from_bytes::<String>(&not_utf8, &[]),
            Err(DecodeError::BadString)
        );

        let mut inner_nul = 3u32.to_le_bytes().to_vec();
        inner_nul.extend_from_slice(b"a\0b\0");
        assert_eq!(
            from_bytes::<String>(&inner_nul, &[]),
            Err(DecodeError::BadString)
        );

        let mut not_a_path = 1u32.to_le_bytes().to_vec();
        not_a_path.extend_from_slice(b"x\0");
        assert!(matches!(
            from_bytes::<ObjectPath>(&not_a_path, &[]),
            Err(DecodeError::Name(_))
        ));

        let mut past_end = 8u32.to_le_bytes().to_vec();
        past_end.extend_from_slice(&[1, 2]);
        assert_eq!(
            from_bytes::<Vec<u8>>(&past_end, &[]),
            Err(DecodeError::Truncated)
        );

        let huge = (MAX_ARRAY as u32 + 1).to_le_bytes();
        assert_eq!(
            from_bytes::<Vec<u8>>(&huge, &[]),
            Err(DecodeError::ArrayTooLong(MAX_ARRAY + 1))
        );

        let mut overrun = 3u32.to_le_bytes().to_vec();
        overrun.extend_from_slice(&[1, 0, 0, 0]);
        assert_eq!(
            from_bytes::<Vec<u32>>(&overrun, &[]),
            Err(DecodeError::ArrayOverrun)
        );

        assert_eq!(
            from_bytes::<UnixFd>(&0u32.to_le_bytes(), &[]).map(|_| ()),
            Err(DecodeError::FdIndex(0))
        );
        assert_eq!(
            from_bytes::<u8>(&[1, 2], &[]),
            Err(DecodeError::Trailing(1))
        );
    }

    #[test]
    #[should_panic(expected = "a string with a NUL cannot be sent")]
    fn encoding_a_nul_panics() {
        to_bytes("a\0b");
    }
}
