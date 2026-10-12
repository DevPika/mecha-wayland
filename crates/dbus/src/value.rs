//! [`Value`], the dynamic side: any D-Bus value with its type.

use std::collections::{BTreeMap, HashMap};

use crate::fd::UnixFd;
use crate::names::ObjectPath;
use crate::signature::{self, Signature};
use crate::types::{self, Basic, Decode, Encode, Type};
use crate::wire::{DecodeError, Decoder, Encoder};

/// Any D-Bus value. As the type `v` (a variant) it encodes its signature
/// first; decoding a `v` gives the value *inside*, so `Variant` only
/// appears for a variant holding a variant.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    U8(u8),
    Bool(bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    ObjectPath(ObjectPath),
    Signature(Signature),
    Fd(UnixFd),
    /// Every item must have the signature `elem`, which must be exactly one
    /// complete type. Encoding panics otherwise: that is a caller bug.
    Array {
        elem: Signature,
        items: Vec<Value>,
    },
    /// Every key must have `key` (exactly one basic type) and every value
    /// `val` (exactly one complete type). Encoding panics otherwise.
    Dict {
        key: Signature,
        val: Signature,
        entries: Vec<(Value, Value)>,
    },
    Struct(Vec<Value>),
    Variant(Box<Value>),
}

/// `a{sv}`: the property map NetworkManager, BlueZ and the portals use.
pub type PropMap = HashMap<String, Value>;

pub trait PropMapExt {
    /// The entry at `key` as a `T`; `None` if absent.
    fn get_as<T: Decode>(&self, key: &str) -> Option<Result<T, DecodeError>>;
}

impl PropMapExt for PropMap {
    fn get_as<T: Decode>(&self, key: &str) -> Option<Result<T, DecodeError>> {
        self.get(key).map(Value::get)
    }
}

impl Value {
    pub fn signature(&self) -> Signature {
        let mut s = String::new();
        self.write_sig(&mut s);
        Signature::from_valid(s)
    }

    fn write_sig(&self, out: &mut String) {
        let c = match self {
            Value::U8(_) => 'y',
            Value::Bool(_) => 'b',
            Value::I16(_) => 'n',
            Value::U16(_) => 'q',
            Value::I32(_) => 'i',
            Value::U32(_) => 'u',
            Value::I64(_) => 'x',
            Value::U64(_) => 't',
            Value::F64(_) => 'd',
            Value::Str(_) => 's',
            Value::ObjectPath(_) => 'o',
            Value::Signature(_) => 'g',
            Value::Fd(_) => 'h',
            Value::Variant(_) => 'v',
            Value::Array { elem, .. } => {
                assert!(
                    elem.is_single(),
                    "dbus: an array's element signature must be one complete type, not {elem:?}"
                );
                out.push('a');
                out.push_str(elem.as_str());
                return;
            }
            Value::Dict { key, val, .. } => {
                assert!(
                    key.as_str().len() == 1 && signature::is_basic(key.as_str().as_bytes()[0]),
                    "dbus: a dict's key must be one basic type, not {key:?}"
                );
                assert!(
                    val.is_single(),
                    "dbus: a dict's value must be one complete type, not {val:?}"
                );
                out.push_str("a{");
                out.push_str(key.as_str());
                out.push_str(val.as_str());
                out.push('}');
                return;
            }
            Value::Struct(fields) => {
                out.push('(');
                for f in fields {
                    f.write_sig(out);
                }
                out.push(')');
                return;
            }
        };
        out.push(c);
    }

    /// This value as a `T`. Variants unwrap on the way, unless `T` is
    /// [`Value`] itself.
    pub fn get<T: Decode>(&self) -> Result<T, DecodeError> {
        let want = T::signature();
        let mut enc = Encoder::new();
        if want.as_str() == "v" {
            self.encode(&mut enc);
        } else {
            let mut v = self;
            while let Value::Variant(inner) = v {
                v = inner;
            }
            let found = v.signature();
            if found != want {
                return Err(DecodeError::Mismatch {
                    expected: want.as_str().to_owned(),
                    found: found.as_str().to_owned(),
                });
            }
            v.encode_raw(&mut enc);
        }
        let (bytes, fds) = enc.into_parts();
        types::from_bytes(&bytes, &fds)
    }

    /// Any typed value, as a `Value`.
    pub fn from_typed<T: Encode + ?Sized>(v: &T) -> Value {
        let (bytes, fds) = types::to_bytes(v);
        let sig = T::signature();
        // Our own bytes, so no peer depth cap: a value nested near the
        // cap gains levels here and must still come back.
        let mut d = Decoder::unbounded(&bytes, false, &fds);
        Value::decode_raw(sig.as_str(), &mut d).expect("dbus: re-decoding a value just encoded")
    }

    /// The value without a variant's signature in front.
    pub(crate) fn encode_raw(&self, e: &mut Encoder) {
        match self {
            Value::U8(v) => e.u8(*v),
            Value::Bool(v) => e.bool(*v),
            Value::I16(v) => e.i16(*v),
            Value::U16(v) => e.u16(*v),
            Value::I32(v) => e.i32(*v),
            Value::U32(v) => e.u32(*v),
            Value::I64(v) => e.i64(*v),
            Value::U64(v) => e.u64(*v),
            Value::F64(v) => e.f64(*v),
            Value::Str(s) => e.str(s),
            Value::ObjectPath(p) => e.str(p.as_str()),
            Value::Signature(s) => e.signature(s.as_str()),
            Value::Fd(f) => e.fd(f),
            Value::Array { elem, items } => {
                e.array(signature::alignment(elem.as_str().as_bytes()[0]), |e| {
                    for i in items {
                        i.encode_raw(e);
                    }
                })
            }
            Value::Dict { entries, .. } => e.array(8, |e| {
                for (k, v) in entries {
                    e.structure(|e| {
                        k.encode_raw(e);
                        v.encode_raw(e);
                    });
                }
            }),
            Value::Struct(fields) => e.structure(|e| {
                for f in fields {
                    f.encode_raw(e);
                }
            }),
            Value::Variant(inner) => inner.encode(e),
        }
    }

    /// Read one value of the single complete type `sig`.
    pub(crate) fn decode_raw(sig: &str, d: &mut Decoder<'_>) -> Result<Value, DecodeError> {
        let b = sig.as_bytes();
        Ok(match b[0] {
            b'y' => Value::U8(d.u8()?),
            b'b' => Value::Bool(d.bool()?),
            b'n' => Value::I16(d.i16()?),
            b'q' => Value::U16(d.u16()?),
            b'i' => Value::I32(d.i32()?),
            b'u' => Value::U32(d.u32()?),
            b'x' => Value::I64(d.i64()?),
            b't' => Value::U64(d.u64()?),
            b'd' => Value::F64(d.f64()?),
            b's' => Value::Str(d.str()?.to_owned()),
            b'o' => Value::ObjectPath(ObjectPath::new(d.str()?)?),
            b'g' => Value::Signature(Signature::from_valid(d.signature()?.to_owned())),
            b'h' => Value::Fd(d.fd()?),
            b'v' => Value::Variant(Box::new(Value::decode(d)?)),
            b'a' if b[1] == b'{' => {
                let key = &sig[2..3];
                let val = &sig[3..sig.len() - 1];
                let mut entries = Vec::new();
                d.array(8, |d| {
                    d.structure(|d| {
                        let k = Value::decode_raw(key, d)?;
                        let v = Value::decode_raw(val, d)?;
                        entries.push((k, v));
                        Ok(())
                    })
                })?;
                Value::Dict {
                    key: Signature::from_valid(key.to_owned()),
                    val: Signature::from_valid(val.to_owned()),
                    entries,
                }
            }
            b'a' => {
                let elem = &sig[1..];
                let mut items = Vec::new();
                d.array(signature::alignment(b[1]), |d| {
                    items.push(Value::decode_raw(elem, d)?);
                    Ok(())
                })?;
                Value::Array {
                    elem: Signature::from_valid(elem.to_owned()),
                    items,
                }
            }
            b'(' => {
                let inner = &sig[1..sig.len() - 1];
                let fields = d.structure(|d| {
                    signature::split(inner)
                        .map(|t| Value::decode_raw(t, d))
                        .collect::<Result<Vec<_>, _>>()
                })?;
                Value::Struct(fields)
            }
            _ => unreachable!("dbus: {sig:?} is not a single complete type"),
        })
    }
}

impl Type for Value {
    fn sig(out: &mut String) {
        out.push('v');
    }
}

impl Encode for Value {
    fn encode(&self, e: &mut Encoder) {
        e.signature(self.signature().as_str());
        self.encode_raw(e);
    }
}

impl Decode for Value {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, DecodeError> {
        let sig = d.signature()?;
        if !Signature::from_valid(sig.to_owned()).is_single() {
            return Err(DecodeError::BadVariant(sig.to_owned()));
        }
        d.enter()?;
        let v = Value::decode_raw(sig, d);
        d.leave();
        v
    }
}

macro_rules! from {
    ($($t:ty => $v:ident),* $(,)?) => {
        $(impl From<$t> for Value {
            fn from(x: $t) -> Self {
                Value::$v(x)
            }
        })*
    };
}

from!(
    u8 => U8, bool => Bool, i16 => I16, u16 => U16, i32 => I32, u32 => U32,
    i64 => I64, u64 => U64, f64 => F64, String => Str,
    ObjectPath => ObjectPath, Signature => Signature, UnixFd => Fd,
);

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Str(s.to_owned())
    }
}

impl<T: Encode> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::from_typed(&v)
    }
}

impl<K: Basic + Encode, V: Encode> From<HashMap<K, V>> for Value {
    fn from(m: HashMap<K, V>) -> Self {
        Value::from_typed(&m)
    }
}

impl<K: Basic + Encode, V: Encode> From<BTreeMap<K, V>> for Value {
    fn from(m: BTreeMap<K, V>) -> Self {
        Value::from_typed(&m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{from_bytes, to_bytes};

    #[test]
    fn dynamic_and_typed_agree_on_bytes() {
        let typed = BTreeMap::from([("k".to_string(), (1u8, vec![ObjectPath::from("/a")]))]);
        let v = Value::from(typed.clone());
        assert_eq!(v.signature(), "a{s(yao)}");
        let (want, _) = to_bytes(&typed);
        let mut e = Encoder::new();
        v.encode_raw(&mut e);
        assert_eq!(e.into_parts().0, want);
        assert_eq!(
            v.get::<BTreeMap<String, (u8, Vec<ObjectPath>)>>().unwrap(),
            typed
        );
    }

    #[test]
    fn variants_unwrap_for_typed_reads() {
        let v = Value::Variant(Box::new(Value::U32(5)));
        assert_eq!(v.get::<u32>(), Ok(5));
        assert_eq!(v.get::<Value>(), Ok(v.clone()));
        assert_eq!(Value::U32(5).get::<Value>(), Ok(Value::U32(5)));
    }

    #[test]
    fn a_mismatch_is_an_error() {
        assert!(matches!(
            Value::Str("x".into()).get::<u32>(),
            Err(DecodeError::Mismatch { .. })
        ));
    }

    #[test]
    fn a_variant_decodes_to_what_it_holds() {
        let (b, f) = to_bytes(&Value::Str("hi".into()));
        assert_eq!(b, [1, b's', 0, 0, 2, 0, 0, 0, b'h', b'i', 0]);
        assert_eq!(from_bytes::<Value>(&b, &f), Ok(Value::Str("hi".into())));
    }

    #[test]
    fn prop_maps() {
        let wire = BTreeMap::from([
            ("on".to_string(), Value::Bool(true)),
            ("n".to_string(), Value::U32(3)),
        ]);
        let (b, f) = to_bytes(&wire);
        let m: PropMap = from_bytes(&b, &f).unwrap();
        assert_eq!(m.get_as::<bool>("on"), Some(Ok(true)));
        assert_eq!(m.get_as::<u32>("n"), Some(Ok(3)));
        assert!(m.get_as::<u32>("missing").is_none());
        assert!(matches!(m.get_as::<u32>("on"), Some(Err(_))));
    }

    #[test]
    fn variant_nesting_is_bounded() {
        let mut v = Value::U8(1);
        for _ in 0..70 {
            v = Value::Variant(Box::new(v));
        }
        let (b, f) = to_bytes(&v);
        assert_eq!(from_bytes::<Value>(&b, &f), Err(DecodeError::TooDeep));
    }

    #[test]
    fn a_variant_must_hold_one_type() {
        let b = [2, b'u', b'u', 0, 1, 0, 0, 0, 2, 0, 0, 0];
        assert!(matches!(
            from_bytes::<Value>(&b, &[]),
            Err(DecodeError::BadVariant(_))
        ));
        let empty = [0, 0];
        assert!(matches!(
            from_bytes::<Value>(&empty, &[]),
            Err(DecodeError::BadVariant(_))
        ));
    }

    #[test]
    fn a_deep_value_survives_from_typed() {
        let mut v = Value::U8(1);
        for _ in 0..63 {
            v = Value::Variant(Box::new(v));
        }
        let wrapped = Value::from(vec![v]);
        assert_eq!(wrapped.signature(), "av");
    }

    #[test]
    #[should_panic(expected = "dbus: an array's element signature")]
    fn an_array_needs_a_single_element_type() {
        let bad = Value::Array {
            elem: Signature::default(),
            items: Vec::new(),
        };
        let _ = to_bytes(&bad);
    }

    #[test]
    #[should_panic(expected = "dbus: a dict's key must be one basic type")]
    fn a_dict_needs_a_basic_key() {
        let bad = Value::Dict {
            key: Signature::from_valid("v".to_owned()),
            val: Signature::from_valid("u".to_owned()),
            entries: Vec::new(),
        };
        let _ = to_bytes(&bad);
    }
}
