//! Deterministic, typed integrity primitives for NEXO.
//!
//! This crate seals byte identity and ordered event history. It does not make
//! the sealed content true, legal, or authoritative.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest as _, Sha256};

pub const CANONICAL_VERSION: u8 = 1;
const HEADER: &[u8] = b"NEXO-C14N\x01";

/// The complete, float-free set of values that may enter a NEXO seal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<CanonicalValue>),
    Map(BTreeMap<String, CanonicalValue>),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

pub fn hash_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest(Sha256::digest(bytes).into())
}

pub fn canonical_bytes(value: &CanonicalValue) -> Vec<u8> {
    let mut output = HEADER.to_vec();
    encode(value, &mut output);
    output
}

pub fn seal(value: &CanonicalValue) -> Sha256Digest {
    hash_bytes(&canonical_bytes(value))
}

fn length(value: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(&(value as u64).to_be_bytes());
}
fn blob(tag: u8, bytes: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    length(bytes.len(), out);
    out.extend_from_slice(bytes);
}
fn encode(value: &CanonicalValue, out: &mut Vec<u8>) {
    match value {
        CanonicalValue::Null => out.push(0),
        CanonicalValue::Bool(false) => out.push(1),
        CanonicalValue::Bool(true) => out.push(2),
        CanonicalValue::I64(n) => {
            out.push(3);
            out.extend_from_slice(&n.to_be_bytes());
        }
        CanonicalValue::U64(n) => {
            out.push(4);
            out.extend_from_slice(&n.to_be_bytes());
        }
        CanonicalValue::Text(s) => blob(5, s.as_bytes(), out),
        CanonicalValue::Bytes(b) => blob(6, b, out),
        CanonicalValue::List(values) => {
            out.push(7);
            length(values.len(), out);
            for v in values {
                encode(v, out);
            }
        }
        CanonicalValue::Map(values) => {
            out.push(8);
            length(values.len(), out);
            for (k, v) in values {
                blob(5, k.as_bytes(), out);
                encode(v, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn map_insertion_order_does_not_change_seal() {
        let mut a = BTreeMap::new();
        a.insert("b".into(), CanonicalValue::U64(2));
        a.insert("a".into(), CanonicalValue::U64(1));
        let mut b = BTreeMap::new();
        b.insert("a".into(), CanonicalValue::U64(1));
        b.insert("b".into(), CanonicalValue::U64(2));
        assert_eq!(seal(&CanonicalValue::Map(a)), seal(&CanonicalValue::Map(b)));
    }
    #[test]
    fn types_do_not_collide() {
        assert_ne!(
            seal(&CanonicalValue::U64(1)),
            seal(&CanonicalValue::Text("1".into()))
        );
        assert_ne!(
            seal(&CanonicalValue::Bool(true)),
            seal(&CanonicalValue::U64(1))
        );
        assert_ne!(
            seal(&CanonicalValue::Text("x".into())),
            seal(&CanonicalValue::Bytes(b"x".to_vec()))
        );
    }

    #[test]
    fn unicode_and_line_endings_remain_byte_distinct() {
        assert_ne!(
            seal(&CanonicalValue::Text("é".into())),
            seal(&CanonicalValue::Text("e\u{301}".into()))
        );
        assert_ne!(
            seal(&CanonicalValue::Text("a\nb".into())),
            seal(&CanonicalValue::Text("a\r\nb".into()))
        );
    }

    #[test]
    fn list_order_is_sealed() {
        assert_ne!(
            seal(&CanonicalValue::List(vec![
                CanonicalValue::U64(1),
                CanonicalValue::U64(2)
            ])),
            seal(&CanonicalValue::List(vec![
                CanonicalValue::U64(2),
                CanonicalValue::U64(1)
            ]))
        );
    }
    #[test]
    fn byte_hash_has_known_sha256() {
        assert_eq!(
            hash_bytes(b"abc").to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
