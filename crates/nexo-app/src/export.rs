//! Atomic filesystem export producer.
//!
//! This module receives already-authorized bytes from the application
//! composition layer. It does not select case facts, evaluate policy, or
//! perform delivery. Its only responsibility is to materialize a directory
//! that `nexo-verify` can validate independently.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nexo_integrity::{hash_bytes, Manifest, ManifestEntry, Sha256Digest};
use serde_json::json;

/// Aggregate export budget. Individual objects are bounded by the object
/// store; this second limit prevents a caller from combining an unbounded
/// number of otherwise-valid objects into one export.
pub const DEFAULT_MAX_EXPORT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
pub struct ExportArtifact<'a> {
    pub label: &'a str,
    pub bytes: &'a [u8],
}

#[derive(Debug, Eq, PartialEq)]
pub struct ExportResult {
    pub directory: PathBuf,
    pub manifest_digest: Sha256Digest,
    pub artifact_count: usize,
}

#[derive(Debug)]
pub enum ExportError {
    InvalidRoot(PathBuf),
    InvalidArtifactLabel(String),
    Manifest(nexo_integrity::ManifestError),
    TooLarge { actual_bytes: u64, max_bytes: u64 },
    Json(serde_json::Error),
    Io(io::Error),
}

impl From<nexo_integrity::ManifestError> for ExportError {
    fn from(value: nexo_integrity::ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl From<serde_json::Error> for ExportError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<io::Error> for ExportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

/// Materializes one export directory without exposing a partially-written
/// export. `destination` must not already exist; callers should allocate a
/// new export identity rather than overwrite a previous one.
pub fn write_export(
    destination: impl AsRef<Path>,
    case_reference: u64,
    generated_at_unix_seconds: i64,
    policy_bundle_digest: Sha256Digest,
    artifacts: &[ExportArtifact<'_>],
) -> Result<ExportResult, ExportError> {
    write_export_with_limit(
        destination,
        case_reference,
        generated_at_unix_seconds,
        policy_bundle_digest,
        artifacts,
        DEFAULT_MAX_EXPORT_BYTES,
    )
}

pub fn write_export_with_limit(
    destination: impl AsRef<Path>,
    case_reference: u64,
    generated_at_unix_seconds: i64,
    policy_bundle_digest: Sha256Digest,
    artifacts: &[ExportArtifact<'_>],
    max_bytes: u64,
) -> Result<ExportResult, ExportError> {
    let destination = destination.as_ref();
    if destination.exists() {
        return Err(ExportError::InvalidRoot(destination.to_path_buf()));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| ExportError::InvalidRoot(destination.to_path_buf()))?;
    fs::create_dir_all(parent)?;

    let mut entries = Vec::with_capacity(artifacts.len());
    let mut materialized = Vec::with_capacity(artifacts.len());
    let mut total_bytes = 0_u64;
    for artifact in artifacts {
        if artifact.label.is_empty() {
            return Err(ExportError::InvalidArtifactLabel(artifact.label.to_owned()));
        }
        total_bytes =
            total_bytes
                .checked_add(artifact.bytes.len() as u64)
                .ok_or(ExportError::TooLarge {
                    actual_bytes: u64::MAX,
                    max_bytes,
                })?;
        if total_bytes > max_bytes {
            return Err(ExportError::TooLarge {
                actual_bytes: total_bytes,
                max_bytes,
            });
        }
        let digest = hash_bytes(artifact.bytes);
        entries.push(ManifestEntry::new(artifact.label, digest));
        materialized.push((digest, artifact.bytes));
    }
    let manifest = Manifest::try_new(
        case_reference,
        generated_at_unix_seconds,
        entries,
        policy_bundle_digest,
    )?;
    let manifest_digest = manifest.seal();

    let temporary = tempfile::tempdir_in(parent)?;
    let temporary_root = temporary.path();
    for (digest, bytes) in materialized {
        let hex = digest.to_string();
        let shard = &hex[..2];
        let object_path = temporary_root.join("objects").join(shard).join(&hex);
        fs::create_dir_all(object_path.parent().expect("object path has a parent"))?;
        fs::write(object_path, bytes)?;
        sync_file(temporary_root.join("objects").join(shard).join(&hex))?;
        sync_directory(temporary_root.join("objects").join(shard))?;
    }
    let manifest_json = json!({
        "schema_version": 1,
        "case_reference": case_reference,
        "generated_at_unix_seconds": generated_at_unix_seconds,
        "policy_bundle_digest": policy_bundle_digest.to_string(),
        "manifest_digest": manifest_digest.to_string(),
        "artifacts": artifacts.iter().map(|artifact| {
            let digest = hash_bytes(artifact.bytes).to_string();
            json!({
                "label": artifact.label,
                "digest": digest,
                "path": format!("objects/{}/{}", &digest[..2], digest),
            })
        }).collect::<Vec<_>>(),
    });
    let manifest_path = temporary_root.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest_json)?)?;
    sync_file(&manifest_path)?;
    sync_directory(temporary_root.join("objects"))?;
    sync_directory(temporary_root)?;
    fs::rename(temporary_root, destination)?;
    sync_directory(parent)?;

    Ok(ExportResult {
        directory: destination.to_path_buf(),
        manifest_digest,
        artifact_count: artifacts.len(),
    })
}

fn sync_file(path: impl AsRef<Path>) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(unix)]
fn sync_directory(path: impl AsRef<Path>) -> io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: impl AsRef<Path>) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn writes_export_consumable_by_verifier() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("export");
        let policy = hash_bytes(b"policy");
        let result = write_export(
            &destination,
            7,
            1_700_000_000,
            policy,
            &[ExportArtifact {
                label: "artifact/1",
                bytes: b"evidence",
            }],
        )
        .unwrap();

        assert_eq!(result.artifact_count, 1);
        assert!(destination.join("manifest.json").is_file());
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(
            manifest["manifest_digest"],
            result.manifest_digest.to_string()
        );
        let digest = hash_bytes(b"evidence").to_string();
        assert_eq!(
            fs::read(destination.join("objects").join(&digest[..2]).join(&digest)).unwrap(),
            b"evidence"
        );
        let verified = nexo_verifier::verify_export(destination.join("manifest.json")).unwrap();
        assert_eq!(verified.manifest_digest, result.manifest_digest);
    }

    #[test]
    fn rejects_overwrite_and_invalid_labels() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("export");
        let artifact = [ExportArtifact {
            label: "artifact/1",
            bytes: b"evidence",
        }];
        write_export(
            &destination,
            7,
            1_700_000_000,
            hash_bytes(b"policy"),
            &artifact,
        )
        .unwrap();
        assert!(matches!(
            write_export(
                &destination,
                7,
                1_700_000_000,
                hash_bytes(b"policy"),
                &artifact
            ),
            Err(ExportError::InvalidRoot(_))
        ));
        assert!(matches!(
            write_export_with_limit(
                root.path().join("invalid"),
                7,
                1_700_000_000,
                hash_bytes(b"policy"),
                &[ExportArtifact {
                    label: "",
                    bytes: b"evidence",
                }],
                100,
            ),
            Err(ExportError::InvalidArtifactLabel(_))
        ));
    }

    #[test]
    fn rejects_export_over_aggregate_budget_before_materializing() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("export");
        let result = write_export_with_limit(
            &destination,
            7,
            1_700_000_000,
            hash_bytes(b"policy"),
            &[ExportArtifact {
                label: "artifact/1",
                bytes: b"123456789",
            }],
            8,
        );
        assert!(matches!(
            result,
            Err(ExportError::TooLarge {
                actual_bytes: 9,
                max_bytes: 8
            })
        ));
        assert!(!destination.exists());
    }
}
