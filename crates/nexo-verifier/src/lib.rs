//! Independent verification of an exported NEXO manifest and its bytes.
//!
//! This crate intentionally depends only on `nexo-integrity` and the standard
//! filesystem APIs. It does not import the application, API, or database.

use std::fs;
use std::path::{Component, Path, PathBuf};

use nexo_integrity::{hash_bytes, Manifest, ManifestEntry, Sha256Digest};
use serde::Deserialize;

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
    InvalidDigest { field: &'static str },
    InvalidManifest(String),
    ManifestDigestMismatch { expected: String, actual: String },
    InvalidArtifactPath(String),
    ArtifactOutsideExport(String),
    ReadArtifact { path: PathBuf, source: std::io::Error },
    ArtifactDigestMismatch { path: PathBuf, expected: String, actual: String },
}

pub fn verify_export(manifest_path: impl AsRef<Path>) -> Result<VerificationReport, VerifyError> {
    let manifest_path = manifest_path.as_ref();
    let manifest_bytes = fs::read(manifest_path).map_err(VerifyError::ReadManifest)?;
    let export: ExportManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(VerifyError::ParseManifest)?;
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
            || relative
                .components()
                .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
        {
            return Err(VerifyError::InvalidArtifactPath(artifact.path.clone()));
        }
        let artifact_path = export_root.join(relative);
        let canonical_artifact = artifact_path
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
        fs::write(root.join("manifest.json"), serde_json::to_vec(&json).unwrap()).unwrap();
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
}
