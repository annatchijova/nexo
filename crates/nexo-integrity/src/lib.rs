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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestHexError {
    WrongLength,
    InvalidHexDigit,
}

impl Sha256Digest {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses a lowercase or uppercase 64-character hex digest, the same
    /// shape `Display` produces. This exists so a digest can be pinned as a
    /// literal (in code, in a contract doc, in a config file) and compared
    /// against freshly hashed bytes — the comparison is only meaningful if
    /// the expected side did not itself come from hashing those same bytes
    /// a second time.
    pub fn from_hex(hex: &str) -> Result<Self, DigestHexError> {
        let hex = hex.as_bytes();
        if hex.len() != 64 {
            return Err(DigestHexError::WrongLength);
        }
        let mut out = [0u8; 32];
        for (index, pair) in hex.as_chunks::<2>().0.iter().enumerate() {
            let high = hex_digit(pair[0]).ok_or(DigestHexError::InvalidHexDigit)?;
            let low = hex_digit(pair[1]).ok_or(DigestHexError::InvalidHexDigit)?;
            out[index] = (high << 4) | low;
        }
        Ok(Self(out))
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
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

/// An append-only event whose digest commits its complete payload and predecessor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEntry {
    sequence: u64,
    event: CanonicalValue,
    previous: Option<Sha256Digest>,
    digest: Sha256Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuditReceipt {
    length: u64,
    tip: Option<Sha256Digest>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuditTrail {
    entries: Vec<AuditEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditVerificationError {
    Sequence { index: usize },
    Link { index: usize },
    Digest { index: usize },
    ReceiptLength,
    ReceiptTip,
}

impl AuditTrail {
    pub fn append(&mut self, event: CanonicalValue) -> &AuditEntry {
        let sequence = self.entries.len() as u64;
        let previous = self.entries.last().map(|entry| entry.digest);
        let digest = audit_digest(sequence, &event, previous);
        self.entries.push(AuditEntry {
            sequence,
            event,
            previous,
            digest,
        });
        self.entries.last().expect("entry was pushed")
    }
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }
    pub fn receipt(&self) -> AuditReceipt {
        AuditReceipt {
            length: self.entries.len() as u64,
            tip: self.entries.last().map(|entry| entry.digest),
        }
    }
}

pub fn verify_audit(
    entries: &[AuditEntry],
    receipt: AuditReceipt,
) -> Result<(), AuditVerificationError> {
    if receipt.length != entries.len() as u64 {
        return Err(AuditVerificationError::ReceiptLength);
    }
    if receipt.tip != entries.last().map(|entry| entry.digest) {
        return Err(AuditVerificationError::ReceiptTip);
    }
    let mut previous = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.sequence != index as u64 {
            return Err(AuditVerificationError::Sequence { index });
        }
        if entry.previous != previous {
            return Err(AuditVerificationError::Link { index });
        }
        if entry.digest != audit_digest(entry.sequence, &entry.event, entry.previous) {
            return Err(AuditVerificationError::Digest { index });
        }
        previous = Some(entry.digest);
    }
    Ok(())
}

fn audit_digest(
    sequence: u64,
    event: &CanonicalValue,
    previous: Option<Sha256Digest>,
) -> Sha256Digest {
    let mut fields = BTreeMap::new();
    fields.insert("event".into(), event.clone());
    fields.insert(
        "previous".into(),
        previous
            .map(|d| CanonicalValue::Bytes(d.0.to_vec()))
            .unwrap_or(CanonicalValue::Null),
    );
    fields.insert("sequence".into(), CanonicalValue::U64(sequence));
    seal(&CanonicalValue::Map(fields))
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

/// Schema version for [`Manifest`]'s canonical form. A future incompatible
/// manifest shape bumps this; a reader must reject an unknown version rather
/// than guess its layout, mirroring `PolicySchemaVersion` in `nexo-core`.
pub const MANIFEST_SCHEMA_VERSION: u64 = 1;

/// One named artifact digest included in an export.
///
/// `label` identifies the artifact's role within the export (for example a
/// graph-node reference rendered by the application layer); this crate does
/// not know or depend on `nexo-core`'s id types, so the label is an opaque,
/// non-empty string supplied by the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestEntry {
    label: String,
    digest: Sha256Digest,
}

impl ManifestEntry {
    pub fn new(label: impl Into<String>, digest: Sha256Digest) -> Self {
        Self {
            label: label.into(),
            digest,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestError {
    EmptyArtifacts,
    EmptyLabel,
    DuplicateLabel,
}

/// A versioned, canonical export manifest: it names every artifact digest
/// and the policy-bundle digest an export depended on, per
/// `docs/ARCHITECTURE.md`'s "Data ownership" section.
///
/// This type only hashes and canonicalizes; it performs no I/O, opens no
/// files, and reads no clock (`generated_at_unix_seconds` is supplied by the
/// caller), consistent with ADR 0003. Entries are stored sorted by label so
/// that two manifests built from the same content in a different insertion
/// order seal to the same digest — manifest identity is the content, not
/// the order an adapter happened to collect it in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    case_reference: u64,
    generated_at_unix_seconds: i64,
    artifacts: Vec<ManifestEntry>,
    policy_bundle_digest: Sha256Digest,
}

impl Manifest {
    pub fn try_new(
        case_reference: u64,
        generated_at_unix_seconds: i64,
        mut artifacts: Vec<ManifestEntry>,
        policy_bundle_digest: Sha256Digest,
    ) -> Result<Self, ManifestError> {
        if artifacts.is_empty() {
            return Err(ManifestError::EmptyArtifacts);
        }
        if artifacts.iter().any(|entry| entry.label.is_empty()) {
            return Err(ManifestError::EmptyLabel);
        }
        artifacts.sort_by(|a, b| a.label.cmp(&b.label));
        if artifacts.windows(2).any(|pair| pair[0].label == pair[1].label) {
            return Err(ManifestError::DuplicateLabel);
        }
        Ok(Self {
            case_reference,
            generated_at_unix_seconds,
            artifacts,
            policy_bundle_digest,
        })
    }

    pub fn artifacts(&self) -> &[ManifestEntry] {
        &self.artifacts
    }

    pub const fn policy_bundle_digest(&self) -> Sha256Digest {
        self.policy_bundle_digest
    }

    fn canonical_value(&self) -> CanonicalValue {
        let mut fields = BTreeMap::new();
        fields.insert(
            "schema_version".into(),
            CanonicalValue::U64(MANIFEST_SCHEMA_VERSION),
        );
        fields.insert(
            "case_reference".into(),
            CanonicalValue::U64(self.case_reference),
        );
        fields.insert(
            "generated_at_unix_seconds".into(),
            CanonicalValue::I64(self.generated_at_unix_seconds),
        );
        fields.insert(
            "policy_bundle_digest".into(),
            CanonicalValue::Bytes(self.policy_bundle_digest.as_bytes().to_vec()),
        );
        let artifacts = self
            .artifacts
            .iter()
            .map(|entry| {
                let mut entry_fields = BTreeMap::new();
                entry_fields.insert("label".into(), CanonicalValue::Text(entry.label.clone()));
                entry_fields.insert(
                    "digest".into(),
                    CanonicalValue::Bytes(entry.digest.as_bytes().to_vec()),
                );
                CanonicalValue::Map(entry_fields)
            })
            .collect();
        fields.insert("artifacts".into(), CanonicalValue::List(artifacts));
        CanonicalValue::Map(fields)
    }

    /// Seals this manifest's canonical bytes. Two manifests are the same
    /// export identity if and only if this digest matches.
    pub fn seal(&self) -> Sha256Digest {
        seal(&self.canonical_value())
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
    #[test]
    fn audit_detects_payload_edit_and_tail_truncation() {
        let mut trail = AuditTrail::default();
        trail.append(CanonicalValue::Text("first".into()));
        trail.append(CanonicalValue::Text("second".into()));
        let receipt = trail.receipt();
        assert_eq!(verify_audit(trail.entries(), receipt), Ok(()));
        let mut edited = trail.entries.clone();
        edited[1].event = CanonicalValue::Text("altered".into());
        assert_eq!(
            verify_audit(&edited, receipt),
            Err(AuditVerificationError::Digest { index: 1 })
        );
        assert_eq!(
            verify_audit(&trail.entries()[..1], receipt),
            Err(AuditVerificationError::ReceiptLength)
        );
    }

    #[test]
    fn audit_detects_reordering_and_broken_predecessor() {
        let mut trail = AuditTrail::default();
        trail.append(CanonicalValue::Text("first".into()));
        trail.append(CanonicalValue::Text("second".into()));
        let receipt = trail.receipt();
        let mut reordered = trail.entries.clone();
        reordered.swap(0, 1);
        assert_eq!(
            verify_audit(&reordered, receipt),
            Err(AuditVerificationError::ReceiptTip)
        );
        let mut unlinked = trail.entries.clone();
        unlinked[1].previous = None;
        assert_eq!(
            verify_audit(&unlinked, receipt),
            Err(AuditVerificationError::Link { index: 1 })
        );
    }

    fn sample_manifest(artifacts: Vec<ManifestEntry>) -> Manifest {
        Manifest::try_new(1, 1_700_000_000, artifacts, hash_bytes(b"bundle-v1")).unwrap()
    }

    #[test]
    fn manifest_seal_is_independent_of_entry_insertion_order() {
        let forward = sample_manifest(vec![
            ManifestEntry::new("a", hash_bytes(b"a")),
            ManifestEntry::new("b", hash_bytes(b"b")),
        ]);
        let reversed = sample_manifest(vec![
            ManifestEntry::new("b", hash_bytes(b"b")),
            ManifestEntry::new("a", hash_bytes(b"a")),
        ]);
        assert_eq!(forward.seal(), reversed.seal());
    }

    #[test]
    fn manifest_seal_changes_if_one_artifact_digest_changes() {
        let original = sample_manifest(vec![ManifestEntry::new("a", hash_bytes(b"a"))]);
        let tampered = sample_manifest(vec![ManifestEntry::new("a", hash_bytes(b"a-tampered"))]);
        assert_ne!(original.seal(), tampered.seal());
    }

    #[test]
    fn manifest_seal_changes_if_policy_bundle_digest_changes() {
        let a = Manifest::try_new(
            1,
            1_700_000_000,
            vec![ManifestEntry::new("a", hash_bytes(b"a"))],
            hash_bytes(b"bundle-v1"),
        )
        .unwrap();
        let b = Manifest::try_new(
            1,
            1_700_000_000,
            vec![ManifestEntry::new("a", hash_bytes(b"a"))],
            hash_bytes(b"bundle-v2"),
        )
        .unwrap();
        assert_ne!(a.seal(), b.seal());
    }

    #[test]
    fn manifest_rejects_empty_artifacts() {
        let result = Manifest::try_new(1, 0, vec![], hash_bytes(b"bundle-v1"));
        assert_eq!(result.unwrap_err(), ManifestError::EmptyArtifacts);
    }

    #[test]
    fn manifest_rejects_empty_label() {
        let result = Manifest::try_new(
            1,
            0,
            vec![ManifestEntry::new("", hash_bytes(b"a"))],
            hash_bytes(b"bundle-v1"),
        );
        assert_eq!(result.unwrap_err(), ManifestError::EmptyLabel);
    }

    #[test]
    fn manifest_rejects_duplicate_label() {
        let result = Manifest::try_new(
            1,
            0,
            vec![
                ManifestEntry::new("a", hash_bytes(b"one")),
                ManifestEntry::new("a", hash_bytes(b"two")),
            ],
            hash_bytes(b"bundle-v1"),
        );
        assert_eq!(result.unwrap_err(), ManifestError::DuplicateLabel);
    }

    #[test]
    fn manifest_reordering_a_tail_of_matching_labels_cannot_hide_a_swap() {
        // Two artifacts whose labels sort adjacently but whose digests are
        // swapped must not seal identically to the original: sorting by
        // label must not lose which digest belonged to which label.
        let original = sample_manifest(vec![
            ManifestEntry::new("a", hash_bytes(b"first")),
            ManifestEntry::new("b", hash_bytes(b"second")),
        ]);
        let swapped_digests = sample_manifest(vec![
            ManifestEntry::new("a", hash_bytes(b"second")),
            ManifestEntry::new("b", hash_bytes(b"first")),
        ]);
        assert_ne!(original.seal(), swapped_digests.seal());
    }

    #[test]
    fn from_hex_round_trips_with_display() {
        let digest = hash_bytes(b"abc");
        let parsed = Sha256Digest::from_hex(&digest.to_string()).unwrap();
        assert_eq!(digest, parsed);
    }

    #[test]
    fn from_hex_accepts_uppercase() {
        let digest = hash_bytes(b"abc");
        let upper = digest.to_string().to_uppercase();
        assert_eq!(Sha256Digest::from_hex(&upper).unwrap(), digest);
    }

    #[test]
    fn from_hex_rejects_wrong_length() {
        assert_eq!(
            Sha256Digest::from_hex("ab").unwrap_err(),
            DigestHexError::WrongLength
        );
    }

    #[test]
    fn from_hex_rejects_invalid_digit() {
        let bad = "g".repeat(64);
        assert_eq!(
            Sha256Digest::from_hex(&bad).unwrap_err(),
            DigestHexError::InvalidHexDigit
        );
    }
}
