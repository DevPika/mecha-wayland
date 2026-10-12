//! Type signatures: validated strings of type codes.

use std::fmt;

use crate::names::NameError;

/// A validated D-Bus signature: possibly empty, possibly several complete
/// types (a message body's). At most 255 bytes; arrays and structs (dict
/// entries count as structs) each nest at most 32 deep.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Signature(String);

impl Signature {
    /// Validate a runtime string.
    pub fn new(s: &str) -> Result<Self, NameError> {
        Self::try_from(s.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Exactly one complete type: what a variant may hold.
    pub fn is_single(&self) -> bool {
        !self.0.is_empty() && complete(self.0.as_bytes(), 0, 0, 0) == Some(self.0.len())
    }

    /// Each complete type, in order: `"sa{sv}u"` gives `s`, `a{sv}`, `u`.
    pub fn types(&self) -> impl Iterator<Item = &str> {
        split(&self.0)
    }

    pub(crate) fn from_valid(s: String) -> Self {
        debug_assert!(valid(&s), "not a signature: {s:?}");
        Signature(s)
    }
}

impl TryFrom<String> for Signature {
    type Error = NameError;
    fn try_from(s: String) -> Result<Self, NameError> {
        if valid(&s) {
            Ok(Signature(s))
        } else {
            Err(NameError {
                kind: "signature",
                input: s,
            })
        }
    }
}

/// A literal. Panics if it is invalid.
impl From<&'static str> for Signature {
    fn from(s: &'static str) -> Self {
        Self::new(s).unwrap_or_else(|e| panic!("dbus: {e}"))
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for Signature {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Signature {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

pub(crate) fn valid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() > 255 {
        return false;
    }
    let mut i = 0;
    while i < b.len() {
        match complete(b, i, 0, 0) {
            Some(next) => i = next,
            None => return false,
        }
    }
    true
}

/// The complete types of a valid signature, in order.
pub(crate) fn split(s: &str) -> impl Iterator<Item = &str> {
    let mut rest = s;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let n = complete(rest.as_bytes(), 0, 0, 0).expect("a valid signature");
        let (first, tail) = rest.split_at(n);
        rest = tail;
        Some(first)
    })
}

pub(crate) fn is_basic(c: u8) -> bool {
    b"ybnqiuxtdsogh".contains(&c)
}

/// The alignment of the type whose code is `c`.
pub(crate) fn alignment(c: u8) -> usize {
    match c {
        b'n' | b'q' => 2,
        b'b' | b'i' | b'u' | b'h' | b's' | b'o' | b'a' => 4,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 1, // y, g, v
    }
}

/// Where the complete type starting at `i` ends, or `None` if it is not
/// one. `arrays` and `structs` are the nesting so far.
fn complete(b: &[u8], i: usize, arrays: u32, structs: u32) -> Option<usize> {
    match *b.get(i)? {
        c if is_basic(c) || c == b'v' => Some(i + 1),
        b'a' if arrays < 32 => {
            if b.get(i + 1) == Some(&b'{') {
                if structs >= 32 || !is_basic(*b.get(i + 2)?) {
                    return None;
                }
                let end = complete(b, i + 3, arrays + 1, structs + 1)?;
                (b.get(end) == Some(&b'}')).then_some(end + 1)
            } else {
                complete(b, i + 1, arrays + 1, structs)
            }
        }
        b'(' if structs < 32 => {
            let mut j = i + 1;
            if b.get(j) == Some(&b')') {
                return None;
            }
            while *b.get(j)? != b')' {
                j = complete(b, j, arrays, structs + 1)?;
            }
            Some(j + 1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_signatures() {
        for ok in [
            "",
            "y",
            "a{sv}",
            "(ii)",
            "aa(sa{sv})",
            "a{oa{sa{sv}}}",
            "sa{sv}as",
            "h",
            "v",
        ] {
            assert!(Signature::new(ok).is_ok(), "{ok}");
        }
        assert!(Signature::new(&("a".repeat(32) + "y")).is_ok());
        let structs = "(".repeat(32) + "y" + &")".repeat(32);
        assert!(Signature::new(&structs).is_ok());
    }

    #[test]
    fn invalid_signatures() {
        for bad in [
            "a", "()", "(", ")", "{sv}", "a{vs}", "a{s}", "a{svv}", "z", "a{(s)v}",
        ] {
            assert!(Signature::new(bad).is_err(), "{bad}");
        }
        assert!(Signature::new(&("a".repeat(33) + "y")).is_err());
        let structs = "(".repeat(33) + "y" + &")".repeat(33);
        assert!(Signature::new(&structs).is_err());
        assert!(Signature::new(&"y".repeat(256)).is_err());
    }

    #[test]
    fn single_and_split() {
        assert!(Signature::from("a{sv}").is_single());
        assert!(!Signature::from("su").is_single());
        assert!(!Signature::from("").is_single());
        let s = Signature::from("sa{sv}(ii)u");
        assert_eq!(s.types().collect::<Vec<_>>(), ["s", "a{sv}", "(ii)", "u"]);
    }

    #[test]
    fn alignments() {
        assert_eq!(alignment(b'y'), 1);
        assert_eq!(alignment(b'q'), 2);
        assert_eq!(alignment(b'a'), 4);
        assert_eq!(alignment(b'('), 8);
        assert_eq!(alignment(b'v'), 1);
    }
}
