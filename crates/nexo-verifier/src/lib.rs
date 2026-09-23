//! Independent verification of an exported NEXO manifest and its bytes.
//!
//! This crate intentionally depends only on `nexo-integrity` and the standard
//! filesystem APIs. It does not import the application, API, or database.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use nexo_integrity::{
    AuditHmacKeyring, Manifest, ManifestEntry, Sha256Digest, audit_checkpoint_hmac_v1,
    audit_digest_v2, audit_hmac_v1, canonical_json, hash_bytes,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct ExportManifest {
    schema_version: u64,
    case_reference: u64,
    generated_at_unix_seconds: i64,
    policy_bundle_digest: String,
    manifest_digest: String,
    artifacts: Vec<ExportArtifact>,
}

#[derive(Debug, Deserialize)]
struct ExportArtifact {
    label: String,
    digest: String,
    path: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct VerificationReport {
    pub case_reference: u64,
    pub artifact_count: usize,
    pub manifest_digest: Sha256Digest,
}

#[derive(Debug)]
pub enum VerifyError {
    ReadManifest(std::io::Error),
    ParseManifest(serde_json::Error),
    UnknownSchema(u64),
    InvalidDigest {
        field: &'static str,
    },
    InvalidManifest(String),
    ManifestDigestMismatch {
        expected: String,
        actual: String,
    },
    InvalidArtifactPath(String),
    ArtifactOutsideExport(String),
    ReadArtifact {
        path: PathBuf,
        source: std::io::Error,
    },
    ArtifactDigestMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    AuditRead(std::io::Error),
    AuditParse(serde_json::Error),
    AuditSchema(u64),
    AuditInvalid(String),
    AuditHmacKeyUnavailable(String),
}

#[derive(Debug, Deserialize)]
struct AuditExport {
    schema_version: u64,
    chain_id: String,
    chain_version: u64,
    auth_scheme: String,
    hmac_key_version: String,
    genesis_digest: String,
    events: Vec<AuditExportEvent>,
    checkpoint: AuditCheckpoint,
}

#[derive(Debug, Deserialize)]
struct AuditExportEvent {
    sequence: u64,
    event_id: String,
    occurred_at: String,
    actor_id: i64,
    case_id: Option<i64>,
    event_kind: String,
    event_payload: Value,
    provenance_refs: Value,
    previous_digest: String,
    entry_digest: String,
    auth_scheme: String,
    hmac_key_version: String,
    entry_hmac: String,
}

#[derive(Debug, Deserialize)]
struct AuditCheckpoint {
    chain_version: u64,
    auth_scheme: String,
    hmac_key_version: String,
    at_sequence: u64,
    tip_digest: String,
    tip_hmac: String,
    entry_count: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub struct AuditVerificationReport {
    pub chain_id: String,
    pub chain_version: u64,
    pub entries: usize,
    pub integrity_ok: bool,
    pub linkage_ok: bool,
    pub sequence_ok: bool,
    pub genesis_ok: bool,
    pub checkpoint_ok: bool,
    pub complete_history: bool,
    pub tip_digest: Sha256Digest,
    pub hmac_checked: bool,
    pub hmac_ok: bool,
}

/// Verifies audit-export-v1 without importing NEXO's API, application, or
/// database code. The checkpoint must be retained with the export; it is the
/// completeness witness for the presented history, not proof that the events
/// themselves are true.
pub fn verify_audit_export(path: impl AsRef<Path>) -> Result<AuditVerificationReport, VerifyError> {
    let keyring =
        AuditHmacKeyring::from_environment().map_err(VerifyError::AuditHmacKeyUnavailable)?;
    verify_audit_export_with_keyring(path, &keyring)
}

pub fn verify_audit_export_with_keyring(
    path: impl AsRef<Path>,
    keyring: &AuditHmacKeyring,
) -> Result<AuditVerificationReport, VerifyError> {
    let bytes = fs::read(path).map_err(VerifyError::AuditRead)?;
    let export: AuditExport = serde_json::from_slice(&bytes).map_err(VerifyError::AuditParse)?;
    if export.schema_version != 2 {
        return Err(VerifyError::AuditSchema(export.schema_version));
    }
    if export.chain_id.is_empty() || export.chain_version != 2 {
        return Err(VerifyError::AuditInvalid("unknown chain identity".into()));
    }
    if export.auth_scheme != "hmac-sha256" {
        return Err(VerifyError::AuditInvalid(
            "unknown audit authentication scheme".into(),
        ));
    }
    if export.hmac_key_version != keyring.current_version() {
        return Err(VerifyError::AuditInvalid(
            "export does not use the configured current HMAC key version".into(),
        ));
    }
    if export.events.is_empty() {
        return Err(VerifyError::AuditInvalid(
            "audit-export-v2 requires at least one committed event".into(),
        ));
    }
    let genesis = parse_audit_digest("genesis_digest", &export.genesis_digest)?;
    if genesis != Sha256Digest::zero() {
        return Err(VerifyError::AuditInvalid(
            "audit-export-v2 requires zero genesis".into(),
        ));
    }

    let mut previous = genesis;
    let mut expected_sequence = 1_u64;
    let mut integrity_ok = true;
    let mut linkage_ok = true;
    let mut sequence_ok = true;
    let mut genesis_ok = true;
    let mut tip = genesis;
    let mut event_ids = HashSet::new();
    let mut hmac_ok = true;

    for event in &export.events {
        let previous_digest = parse_audit_digest("previous_digest", &event.previous_digest)?;
        let entry_digest = parse_audit_digest("entry_digest", &event.entry_digest)?;
        if event.sequence != expected_sequence {
            sequence_ok = false;
        }
        if event.previous_digest != previous_digest.to_string() || previous_digest != previous {
            linkage_ok = false;
        }
        if event.event_id != format!("{}:{}", export.chain_id, event.sequence)
            || !event_ids.insert(&event.event_id)
        {
            integrity_ok = false;
        }
        if event.auth_scheme != export.auth_scheme || event.hmac_key_version.is_empty() {
            hmac_ok = false;
        }
        let event_value = json!({
            "schema_version": 2,
            "actor_id": event.actor_id,
            "case_id": event.case_id,
            "event_id": event.event_id,
            "event_kind": event.event_kind,
            "event_payload": event.event_payload,
            "occurred_at": event.occurred_at,
            "provenance_refs": event.provenance_refs,
        });
        let canonical = canonical_json(&event_value)
            .map_err(|_| VerifyError::AuditInvalid("event contains a float".into()))?;
        let recomputed = audit_digest_v2(
            &export.chain_id,
            export.chain_version,
            event.sequence,
            &canonical,
            previous_digest,
        );
        if recomputed != entry_digest {
            integrity_ok = false;
        }
        let key = keyring
            .key(&event.hmac_key_version)
            .ok_or_else(|| VerifyError::AuditHmacKeyUnavailable(event.hmac_key_version.clone()))?;
        let expected_hmac = audit_hmac_v1(
            key,
            &event.hmac_key_version,
            &export.chain_id,
            export.chain_version,
            event.sequence,
            &canonical,
            previous_digest,
            entry_digest,
        );
        if event.entry_hmac != expected_hmac.to_string() {
            hmac_ok = false;
        }
        previous = entry_digest;
        tip = entry_digest;
        expected_sequence = event.sequence.saturating_add(1);
    }

    if export
        .events
        .first()
        .map(|event| event.previous_digest.as_str())
        != Some(&genesis.to_string())
    {
        genesis_ok = false;
    }
    let checkpoint_tip =
        parse_audit_digest("checkpoint.tip_digest", &export.checkpoint.tip_digest)?;
    if export.checkpoint.auth_scheme != export.auth_scheme
        || export.checkpoint.chain_version != export.chain_version
    {
        hmac_ok = false;
    }
    let checkpoint_key = keyring
        .key(&export.checkpoint.hmac_key_version)
        .ok_or_else(|| {
            VerifyError::AuditHmacKeyUnavailable(export.checkpoint.hmac_key_version.clone())
        })?;
    let expected_checkpoint_hmac = audit_checkpoint_hmac_v1(
        checkpoint_key,
        &export.checkpoint.hmac_key_version,
        &export.chain_id,
        export.chain_version,
        export.checkpoint.at_sequence,
        checkpoint_tip,
        export.checkpoint.entry_count,
    );
    if export.checkpoint.tip_hmac != expected_checkpoint_hmac.to_string() {
        hmac_ok = false;
    }
    let checkpoint_ok = export.checkpoint.chain_version == export.chain_version
        && export.checkpoint.at_sequence == export.events.len() as u64
        && export.checkpoint.entry_count == export.events.len() as u64
        && checkpoint_tip == tip;
    let complete_history = checkpoint_ok && sequence_ok && genesis_ok && hmac_ok;

    Ok(AuditVerificationReport {
        chain_id: export.chain_id,
        chain_version: export.chain_version,
        entries: export.events.len(),
        integrity_ok,
        linkage_ok,
        sequence_ok,
        genesis_ok,
        checkpoint_ok,
        complete_history,
        tip_digest: tip,
        hmac_checked: true,
        hmac_ok,
    })
}

fn parse_audit_digest(field: &'static str, value: &str) -> Result<Sha256Digest, VerifyError> {
    Sha256Digest::from_hex(value).map_err(|_| VerifyError::AuditInvalid(format!("invalid {field}")))
}

pub fn verify_export(manifest_path: impl AsRef<Path>) -> Result<VerificationReport, VerifyError> {
    let manifest_path = manifest_path.as_ref();
    let manifest_bytes = fs::read(manifest_path).map_err(VerifyError::ReadManifest)?;
    let export: ExportManifest =
        serde_json::from_slice(&manifest_bytes).map_err(VerifyError::ParseManifest)?;
    if export.schema_version != 1 {
        return Err(VerifyError::UnknownSchema(export.schema_version));
    }

    let policy_digest = parse_digest("policy_bundle_digest", &export.policy_bundle_digest)?;
    let expected_manifest_digest = parse_digest("manifest_digest", &export.manifest_digest)?;
    let entries = export
        .artifacts
        .iter()
        .map(|artifact| {
            Ok(ManifestEntry::new(
                artifact.label.clone(),
                parse_digest("artifact_digest", &artifact.digest)?,
            ))
        })
        .collect::<Result<Vec<_>, VerifyError>>()?;
    let manifest = Manifest::try_new(
        export.case_reference,
        export.generated_at_unix_seconds,
        entries,
        policy_digest,
    )
    .map_err(|error| VerifyError::InvalidManifest(format!("{error:?}")))?;
    let actual_manifest_digest = manifest.seal();
    if actual_manifest_digest != expected_manifest_digest {
        return Err(VerifyError::ManifestDigestMismatch {
            expected: expected_manifest_digest.to_string(),
            actual: actual_manifest_digest.to_string(),
        });
    }

    let manifest_path = manifest_path
        .canonicalize()
        .map_err(VerifyError::ReadManifest)?;
    let export_root = manifest_path
        .parent()
        .expect("canonical manifest path has a parent")
        .to_path_buf();
    for artifact in &export.artifacts {
        let relative = Path::new(&artifact.path);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(VerifyError::InvalidArtifactPath(artifact.path.clone()));
        }
        let artifact_path = export_root.join(relative);
        let canonical_artifact =
            artifact_path
                .canonicalize()
                .map_err(|source| VerifyError::ReadArtifact {
                    path: artifact_path.clone(),
                    source,
                })?;
        if !canonical_artifact.starts_with(&export_root) {
            return Err(VerifyError::ArtifactOutsideExport(artifact.path.clone()));
        }
        let bytes = fs::read(&canonical_artifact).map_err(|source| VerifyError::ReadArtifact {
            path: canonical_artifact.clone(),
            source,
        })?;
        let expected = parse_digest("artifact_digest", &artifact.digest)?;
        let actual = hash_bytes(&bytes);
        if actual != expected {
            return Err(VerifyError::ArtifactDigestMismatch {
                path: canonical_artifact,
                expected: expected.to_string(),
                actual: actual.to_string(),
            });
        }
    }

    Ok(VerificationReport {
        case_reference: export.case_reference,
        artifact_count: export.artifacts.len(),
        manifest_digest: actual_manifest_digest,
    })
}

fn parse_digest(field: &'static str, value: &str) -> Result<Sha256Digest, VerifyError> {
    Sha256Digest::from_hex(value).map_err(|_| VerifyError::InvalidDigest { field })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_export() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("nexo-verifier-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn write_export(root: &Path, artifact_path: &str, manifest_digest: &str) {
        let bytes = b"evidence";
        if !artifact_path.contains("..") {
            fs::write(root.join(artifact_path), bytes).unwrap();
        }
        let artifact_digest = hash_bytes(bytes);
        let policy_digest = hash_bytes(b"policy");
        let manifest = Manifest::try_new(
            7,
            1_700_000_000,
            vec![ManifestEntry::new("artifact/1", artifact_digest)],
            policy_digest,
        )
        .unwrap();
        let json = serde_json::json!({
            "schema_version": 1,
            "case_reference": 7,
            "generated_at_unix_seconds": 1_700_000_000,
            "policy_bundle_digest": policy_digest.to_string(),
            "manifest_digest": if manifest_digest.is_empty() { manifest.seal().to_string() } else { manifest_digest.to_owned() },
            "artifacts": [{"label": "artifact/1", "digest": artifact_digest.to_string(), "path": artifact_path}],
        });
        fs::write(
            root.join("manifest.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn verifies_manifest_and_artifact_bytes() {
        let root = temp_export();
        write_export(&root, "evidence.bin", "");
        let report = verify_export(root.join("manifest.json")).unwrap();
        assert_eq!(report.case_reference, 7);
        assert_eq!(report.artifact_count, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_tampered_artifact() {
        let root = temp_export();
        write_export(&root, "evidence.bin", "");
        fs::write(root.join("evidence.bin"), b"tampered").unwrap();
        assert!(matches!(
            verify_export(root.join("manifest.json")),
            Err(VerifyError::ArtifactDigestMismatch { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_path_traversal() {
        let root = temp_export();
        write_export(&root, "../outside.bin", "");
        assert!(matches!(
            verify_export(root.join("manifest.json")),
            Err(VerifyError::InvalidArtifactPath(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    fn write_audit_export(root: &Path, events: usize) {
        let keyring = test_keyring();
        let key = keyring.key("v1").unwrap();
        let mut previous = Sha256Digest::zero();
        let mut serialized = Vec::new();
        for sequence in 1..=events as u64 {
            let event = serde_json::json!({
                "schema_version": 2,
                "actor_id": 7,
                "case_id": 9,
                "event_id": format!("mutations:{sequence}"),
                "event_kind": "case.created",
                "event_payload": {"case_id": 9},
                "occurred_at": "2026-09-22T12:00:00+00:00",
                "provenance_refs": [],
            });
            let canonical = canonical_json(&event).unwrap();
            let entry = audit_digest_v2("mutations", 2, sequence, &canonical, previous);
            let entry_hmac = audit_hmac_v1(
                key,
                "v1",
                "mutations",
                2,
                sequence,
                &canonical,
                previous,
                entry,
            );
            serialized.push(serde_json::json!({
                "sequence": sequence,
                "event_id": format!("mutations:{sequence}"),
                "occurred_at": "2026-09-22T12:00:00+00:00",
                "actor_id": 7,
                "case_id": 9,
                "event_kind": "case.created",
                "event_payload": {"case_id": 9},
                "provenance_refs": [],
                "previous_digest": previous.to_string(),
                "entry_digest": entry.to_string(),
                "auth_scheme": "hmac-sha256",
                "hmac_key_version": "v1",
                "entry_hmac": entry_hmac.to_string(),
            }));
            previous = entry;
        }
        let export = serde_json::json!({
            "schema_version": 2,
            "chain_id": "mutations",
            "chain_version": 2,
            "auth_scheme": "hmac-sha256",
            "hmac_key_version": "v1",
            "genesis_digest": Sha256Digest::zero().to_string(),
            "events": serialized,
            "checkpoint": {
                "chain_version": 2,
                "auth_scheme": "hmac-sha256",
                "hmac_key_version": "v1",
                "at_sequence": events,
                "tip_digest": previous.to_string(),
                "tip_hmac": audit_checkpoint_hmac_v1(
                    key,
                    "v1",
                    "mutations",
                    2,
                    events as u64,
                    previous,
                    events as u64,
                )
                .to_string(),
                "entry_count": events,
            },
        });
        fs::write(
            root.join("audit.json"),
            serde_json::to_vec(&export).unwrap(),
        )
        .unwrap();
    }

    fn test_keyring() -> AuditHmacKeyring {
        AuditHmacKeyring::from_parts("v1", b"01234567890123456789012345678901", "").unwrap()
    }

    #[test]
    fn verifies_audit_export_and_checkpoint() {
        let root = temp_export();
        write_audit_export(&root, 2);
        let report =
            verify_audit_export_with_keyring(root.join("audit.json"), &test_keyring()).unwrap();
        assert!(report.integrity_ok);
        assert!(report.linkage_ok);
        assert!(report.sequence_ok);
        assert!(report.genesis_ok);
        assert!(report.checkpoint_ok);
        assert!(report.complete_history);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_export_rejects_payload_edit() {
        let root = temp_export();
        write_audit_export(&root, 2);
        let path = root.join("audit.json");
        let mut export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        export["events"][0]["event_payload"]["case_id"] = serde_json::json!(99);
        fs::write(&path, serde_json::to_vec(&export).unwrap()).unwrap();
        let report = verify_audit_export_with_keyring(path, &test_keyring()).unwrap();
        assert!(!report.integrity_ok);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_export_rejects_metadata_edit() {
        let root = temp_export();
        write_audit_export(&root, 2);
        let path = root.join("audit.json");
        let mut export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        export["events"][0]["occurred_at"] = serde_json::json!("2030-01-01T00:00:00+00:00");
        fs::write(&path, serde_json::to_vec(&export).unwrap()).unwrap();
        let report = verify_audit_export_with_keyring(path, &test_keyring()).unwrap();
        assert!(!report.integrity_ok);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_export_rejects_hmac_edit() {
        let root = temp_export();
        write_audit_export(&root, 2);
        let path = root.join("audit.json");
        let mut export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        export["events"][0]["entry_hmac"] = serde_json::json!(Sha256Digest::zero().to_string());
        fs::write(&path, serde_json::to_vec(&export).unwrap()).unwrap();
        let report = verify_audit_export_with_keyring(path, &test_keyring()).unwrap();
        assert!(report.hmac_checked);
        assert!(!report.hmac_ok);
        assert!(!report.complete_history);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_export_rejects_wrong_and_missing_hmac_keys() {
        let root = temp_export();
        write_audit_export(&root, 1);
        let path = root.join("audit.json");
        let wrong =
            AuditHmacKeyring::from_parts("v1", b"different-key-012345678901234567890", "").unwrap();
        let report = verify_audit_export_with_keyring(&path, &wrong).unwrap();
        assert!(report.hmac_checked);
        assert!(!report.hmac_ok);
        let missing =
            AuditHmacKeyring::from_parts("v1", b"different-key-012345678901234567890", "").unwrap();
        let mut missing_export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        missing_export["events"][0]["hmac_key_version"] = serde_json::json!("v2");
        fs::write(&path, serde_json::to_vec(&missing_export).unwrap()).unwrap();
        assert!(matches!(
            verify_audit_export_with_keyring(path, &missing),
            Err(VerifyError::AuditHmacKeyUnavailable(version)) if version == "v2"
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_export_accepts_key_rotation_with_previous_verification_key() {
        let root = temp_export();
        write_audit_export(&root, 1);
        let path = root.join("audit.json");
        let mut export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        export["hmac_key_version"] = serde_json::json!("v2");
        fs::write(&path, serde_json::to_vec(&export).unwrap()).unwrap();
        let rotated = AuditHmacKeyring::from_parts(
            "v2",
            b"new-key-012345678901234567890123456",
            "v1=01234567890123456789012345678901",
        )
        .unwrap();
        let report = verify_audit_export_with_keyring(path, &rotated).unwrap();
        assert!(report.hmac_ok);
        assert!(report.complete_history);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_verifier_cannot_reject_a_fully_rewritten_chain_without_external_witness() {
        let root = temp_export();
        write_audit_export(&root, 2);
        let path = root.join("audit.json");
        let mut export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let mut previous = Sha256Digest::zero();
        for (index, event) in export["events"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            event["event_payload"]["case_id"] = serde_json::json!(99);
            let sequence = (index + 1) as u64;
            let event_value = serde_json::json!({
                "schema_version": 2,
                "actor_id": event["actor_id"],
                "case_id": event["case_id"],
                "event_id": event["event_id"],
                "event_kind": event["event_kind"],
                "event_payload": event["event_payload"],
                "occurred_at": event["occurred_at"],
                "provenance_refs": event["provenance_refs"],
            });
            let digest = audit_digest_v2(
                "mutations",
                2,
                sequence,
                &canonical_json(&event_value).unwrap(),
                previous,
            );
            let hmac = audit_hmac_v1(
                test_keyring().key("v1").unwrap(),
                "v1",
                "mutations",
                2,
                sequence,
                &canonical_json(&event_value).unwrap(),
                previous,
                digest,
            );
            event["previous_digest"] = serde_json::json!(previous.to_string());
            event["entry_digest"] = serde_json::json!(digest.to_string());
            event["entry_hmac"] = serde_json::json!(hmac.to_string());
            previous = digest;
        }
        export["checkpoint"]["tip_digest"] = serde_json::json!(previous.to_string());
        export["checkpoint"]["tip_hmac"] = serde_json::json!(
            audit_checkpoint_hmac_v1(
                test_keyring().key("v1").unwrap(),
                "v1",
                "mutations",
                2,
                2,
                previous,
                2,
            )
            .to_string()
        );
        fs::write(&path, serde_json::to_vec(&export).unwrap()).unwrap();
        let report = verify_audit_export_with_keyring(path, &test_keyring()).unwrap();
        assert!(report.integrity_ok);
        assert!(report.linkage_ok);
        assert!(report.complete_history);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn audit_export_rejects_tail_truncation_against_checkpoint() {
        let root = temp_export();
        write_audit_export(&root, 3);
        let path = root.join("audit.json");
        let mut export: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        export["events"].as_array_mut().unwrap().pop();
        fs::write(&path, serde_json::to_vec(&export).unwrap()).unwrap();
        let report = verify_audit_export_with_keyring(path, &test_keyring()).unwrap();
        assert!(!report.checkpoint_ok);
        assert!(!report.complete_history);
        fs::remove_dir_all(root).unwrap();
    }
}
