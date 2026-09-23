//! Application-owned bridge from durable receipt evidence to the core
//! preparation capability. It proves the authority boundary before material
//! generation and persistence.

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU64;
use std::path::Path;

use chrono::Utc;
use nexo_app::export::{self, ExportArtifact, ExportResult};
use nexo_app::object_store::{FilesystemObjectStore, ObjectStoreError};
use nexo_app::repository::{self, ActionEvaluationRowId, CaseRowId, Pool, ProvenanceRowId};
use nexo_core::{ActionOption, NodeId, VerifiedPreparationSnapshot};
use nexo_integrity::{hash_bytes, seal, CanonicalValue, Sha256Digest};
use serde_json::{json, Value};

#[derive(Debug)]
pub enum PreparationVerificationError {
    Repository(repository::RepoError),
    ObjectStore(ObjectStoreError),
    CaseNotOwned,
    ReceiptNotFound,
    EvaluationNotSupported,
    InvalidDurableIdentity,
    ActionFingerprintMismatch,
    InputManifestChanged,
    PolicyBundleChanged,
    PreparationOutputMismatch,
    PreparationNotFound,
    PreparationNotPrepared,
    InvalidExportIdentity,
    Export(export::ExportError),
    ExistingExportInvalid(nexo_verifier::VerifyError),
    ExportManifestMismatch,
    ExportDestinationConflict,
}

pub fn action_fingerprint(action: &ActionOption) -> String {
    let factual_support = action
        .factual_support()
        .iter()
        .map(|support| match support {
            nexo_core::FactualSupport::Artifact(id) => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".into(), CanonicalValue::Text("artifact".into()));
                fields.insert("node_id".into(), CanonicalValue::U64(id.as_u64()));
                CanonicalValue::Map(fields)
            }
            nexo_core::FactualSupport::Observation(id) => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".into(), CanonicalValue::Text("observation".into()));
                fields.insert("node_id".into(), CanonicalValue::U64(id.as_u64()));
                CanonicalValue::Map(fields)
            }
            nexo_core::FactualSupport::UserAssertion(id) => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".into(), CanonicalValue::Text("user_assertion".into()));
                fields.insert("node_id".into(), CanonicalValue::U64(id.as_u64()));
                CanonicalValue::Map(fields)
            }
            nexo_core::FactualSupport::DerivedFact(id) => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".into(), CanonicalValue::Text("derived_fact".into()));
                fields.insert("node_id".into(), CanonicalValue::U64(id.as_u64()));
                CanonicalValue::Map(fields)
            }
        })
        .collect();
    let legal_support = action
        .legal_support()
        .iter()
        .map(|id| CanonicalValue::U64(id.node_id().as_u64()))
        .collect();
    let unmet_requirements = action
        .unmet_requirements()
        .iter()
        .map(|id| CanonicalValue::U64(id.node_id().as_u64()))
        .collect();
    let mut fields = BTreeMap::new();
    fields.insert(
        "status".into(),
        CanonicalValue::Text(
            match action.status() {
                nexo_core::ActionStatus::Supported => "supported",
                nexo_core::ActionStatus::ConditionallySupported => "conditionally_supported",
            }
            .into(),
        ),
    );
    fields.insert(
        "factual_support".into(),
        CanonicalValue::List(factual_support),
    );
    fields.insert("legal_support".into(), CanonicalValue::List(legal_support));
    fields.insert(
        "unmet_requirements".into(),
        CanonicalValue::List(unmet_requirements),
    );
    seal(&CanonicalValue::Map(fields)).to_string()
}

impl From<repository::RepoError> for PreparationVerificationError {
    fn from(value: repository::RepoError) -> Self {
        Self::Repository(value)
    }
}

impl From<ObjectStoreError> for PreparationVerificationError {
    fn from(value: ObjectStoreError) -> Self {
        Self::ObjectStore(value)
    }
}

impl From<export::ExportError> for PreparationVerificationError {
    fn from(value: export::ExportError) -> Self {
        Self::Export(value)
    }
}

fn node_id(value: i64) -> Result<NodeId, PreparationVerificationError> {
    let value = u64::try_from(value)
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or(PreparationVerificationError::InvalidDurableIdentity)?;
    Ok(NodeId::new(value))
}

/// Mints the core capability only from an actor-authorized, receipt-backed
/// supported evaluation. Route, evaluation, and digest identities are all
/// resolved from the joined durable receipt; none are accepted from the
/// request path.
pub async fn verify_receipt_and_mint_snapshot(
    pool: &Pool,
    actor: repository::ActorRowId,
    case: CaseRowId,
    evaluation: ActionEvaluationRowId,
    action: ActionOption,
) -> Result<VerifiedPreparationSnapshot, PreparationVerificationError> {
    if repository::case_owner(pool, case).await? != Some(actor) {
        return Err(PreparationVerificationError::CaseNotOwned);
    }

    if !action.is_available() {
        return Err(PreparationVerificationError::EvaluationNotSupported);
    }

    let binding = repository::find_evaluation_receipt_binding(pool, case, evaluation)
        .await?
        .ok_or(PreparationVerificationError::ReceiptNotFound)?;
    if binding.evaluation_id != evaluation.0
        || binding.result_kind != "actionable"
        || binding.action_status.as_deref() != Some("supported")
    {
        return Err(PreparationVerificationError::EvaluationNotSupported);
    }
    if action_fingerprint(&action) != binding.action_digest_hex {
        return Err(PreparationVerificationError::ActionFingerprintMismatch);
    }

    let action_identity = node_id(binding.route_id)?;
    let evaluation_snapshot = node_id(binding.evaluation_id)?;
    let policy_bundle_digest = nexo_core::DigestId::new(node_id(binding.policy_bundle_digest_id)?);
    let input_manifest_digest =
        nexo_core::DigestId::new(node_id(binding.input_manifest_digest_id)?);

    Ok(
        VerifiedPreparationSnapshot::from_application_verified_evaluation(
            action.with_identity(action_identity),
            evaluation_snapshot,
            policy_bundle_digest,
            input_manifest_digest,
        ),
    )
}

/// Stores generated bytes through the content-addressed object store and then
/// records the preparation using receipt-derived bindings. This is an
/// application helper only; it is not routed and has no transport capability.
#[allow(clippy::too_many_arguments)]
pub async fn persist_prepared_material(
    pool: &Pool,
    store: &FilesystemObjectStore,
    actor: repository::ActorRowId,
    case: CaseRowId,
    evaluation: ActionEvaluationRowId,
    action: ActionOption,
    kind: &str,
    generator_version: &str,
    output_provenance: ProvenanceRowId,
    bytes: &[u8],
) -> Result<i64, PreparationVerificationError> {
    verify_receipt_and_mint_snapshot(pool, actor, case, evaluation, action).await?;
    let binding = repository::find_evaluation_receipt_binding(pool, case, evaluation)
        .await?
        .ok_or(PreparationVerificationError::ReceiptNotFound)?;
    let mut tx = pool.begin().await.map_err(repository::RepoError::from)?;
    repository::lock_case(&mut tx, case).await?;
    repository::lock_policy_jurisdiction(
        &mut tx,
        repository::PolicyBundleRowId(binding.policy_bundle_id),
    )
    .await?;
    if repository::current_policy_bundle_id(
        &mut tx,
        repository::PolicyBundleRowId(binding.policy_bundle_id),
    )
    .await?
        != Some(binding.policy_bundle_id)
    {
        return Err(PreparationVerificationError::PolicyBundleChanged);
    }
    let current_manifest = crate::projection::current_input_manifest_digest(pool, case)
        .await
        .map_err(|error| match error {
            crate::projection::ProjectionError::Repo(error) => {
                PreparationVerificationError::Repository(error)
            }
            _ => PreparationVerificationError::InputManifestChanged,
        })?;
    if current_manifest != binding.input_manifest_digest_hex {
        return Err(PreparationVerificationError::InputManifestChanged);
    }
    let output_digest = hash_bytes(bytes);
    if let Some((existing, existing_digest)) =
        repository::find_active_preparation(&mut tx, evaluation, kind, generator_version).await?
    {
        if existing_digest != output_digest.to_string() {
            return Err(PreparationVerificationError::PreparationOutputMismatch);
        }
        return Ok(existing);
    }
    let output_digest = store.put(bytes)?;
    let output_digest_hex = output_digest.to_string();
    let output_digest = repository::upsert_digest(&mut tx, "sha256", &output_digest_hex).await?;
    let preparation = repository::insert_preparation(
        &mut tx,
        evaluation,
        repository::ActionRouteRowId(binding.route_id),
        repository::DigestRowId(binding.policy_bundle_digest_id),
        repository::DigestRowId(binding.input_manifest_digest_id),
        generator_version,
        kind,
        output_digest,
        output_provenance,
    )
    .await?;
    nexo_app::audit::append(
        &mut tx,
        actor,
        Some(case),
        "preparation.created",
        serde_json::json!({
            "preparation_id": preparation,
            "evaluation_id": evaluation.0,
            "kind": kind,
            "output_digest": output_digest_hex,
        }),
        serde_json::json!([]),
    )
    .await
    .map_err(repository::RepoError::from)?;
    tx.commit().await.map_err(repository::RepoError::from)?;
    Ok(preparation)
}

/// Same preparation boundary as `persist_prepared_material`, but owns the
/// provenance write so an HTTP preparation command has one durable commit.
#[allow(clippy::too_many_arguments)]
pub async fn persist_prepared_material_with_provenance(
    pool: &Pool,
    store: &FilesystemObjectStore,
    actor: repository::ActorRowId,
    case: CaseRowId,
    evaluation: ActionEvaluationRowId,
    action: ActionOption,
    kind: &str,
    generator_version: &str,
    bytes: &[u8],
) -> Result<i64, PreparationVerificationError> {
    verify_receipt_and_mint_snapshot(pool, actor, case, evaluation, action).await?;
    let binding = repository::find_evaluation_receipt_binding(pool, case, evaluation)
        .await?
        .ok_or(PreparationVerificationError::ReceiptNotFound)?;
    let mut tx = pool.begin().await.map_err(repository::RepoError::from)?;
    repository::lock_case(&mut tx, case).await?;
    repository::lock_policy_jurisdiction(
        &mut tx,
        repository::PolicyBundleRowId(binding.policy_bundle_id),
    )
    .await?;
    if repository::current_policy_bundle_id(
        &mut tx,
        repository::PolicyBundleRowId(binding.policy_bundle_id),
    )
    .await?
        != Some(binding.policy_bundle_id)
    {
        return Err(PreparationVerificationError::PolicyBundleChanged);
    }
    let current_manifest = crate::projection::current_input_manifest_digest(pool, case)
        .await
        .map_err(|error| match error {
            crate::projection::ProjectionError::Repo(error) => {
                PreparationVerificationError::Repository(error)
            }
            _ => PreparationVerificationError::InputManifestChanged,
        })?;
    if current_manifest != binding.input_manifest_digest_hex {
        return Err(PreparationVerificationError::InputManifestChanged);
    }
    let output_digest = hash_bytes(bytes);
    if let Some((existing, existing_digest)) =
        repository::find_active_preparation(&mut tx, evaluation, kind, generator_version).await?
    {
        if existing_digest != output_digest.to_string() {
            return Err(PreparationVerificationError::PreparationOutputMismatch);
        }
        return Ok(existing);
    }
    let provenance = repository::insert_provenance(
        &mut tx,
        case,
        "generated_preparation",
        Some(actor),
        chrono::Utc::now(),
        json!({
            "evaluation_id": evaluation.0,
            "generator_version": generator_version,
            "kind": kind,
        }),
    )
    .await?;
    let output_digest = store.put(bytes)?;
    let output_digest_hex = output_digest.to_string();
    let output_digest = repository::upsert_digest(&mut tx, "sha256", &output_digest_hex).await?;
    let preparation = repository::insert_preparation(
        &mut tx,
        evaluation,
        repository::ActionRouteRowId(binding.route_id),
        repository::DigestRowId(binding.policy_bundle_digest_id),
        repository::DigestRowId(binding.input_manifest_digest_id),
        generator_version,
        kind,
        output_digest,
        provenance,
    )
    .await?;
    nexo_app::audit::append(
        &mut tx,
        actor,
        Some(case),
        "preparation.created",
        serde_json::json!({
            "preparation_id": preparation,
            "evaluation_id": evaluation.0,
            "kind": kind,
            "output_digest": output_digest_hex,
            "provenance_id": provenance.0,
        }),
        serde_json::json!([{"provenance_id": provenance.0}]),
    )
    .await
    .map_err(repository::RepoError::from)?;
    tx.commit().await.map_err(repository::RepoError::from)?;
    Ok(preparation)
}

/// Materializes an authorized preparation as a standalone verifiable export
/// and advances its lifecycle only after the bytes have been written. The
/// preparation row stays locked for the whole operation, so invalidation or
/// a concurrent export cannot pass between the durable check and the state
/// transition.
pub async fn export_preparation(
    pool: &Pool,
    store: &FilesystemObjectStore,
    actor: repository::ActorRowId,
    case: CaseRowId,
    preparation: i64,
    destination: impl AsRef<std::path::Path>,
) -> Result<ExportResult, PreparationVerificationError> {
    if repository::case_owner(pool, case).await? != Some(actor) {
        return Err(PreparationVerificationError::CaseNotOwned);
    }
    let mut tx = pool.begin().await.map_err(repository::RepoError::from)?;
    let binding = repository::lock_preparation_for_export(&mut tx, case, preparation)
        .await?
        .ok_or(PreparationVerificationError::PreparationNotFound)?;
    let destination = destination.as_ref();
    if binding.status == "exported" {
        let report = nexo_verifier::verify_export(destination.join("manifest.json"))
            .map_err(PreparationVerificationError::ExistingExportInvalid)?;
        let expected_manifest = binding
            .export_manifest_digest_hex
            .as_deref()
            .and_then(|digest| Sha256Digest::from_hex(digest).ok())
            .ok_or(PreparationVerificationError::InvalidExportIdentity)?;
        if report.manifest_digest != expected_manifest {
            return Err(PreparationVerificationError::ExportManifestMismatch);
        }
        return Ok(ExportResult {
            directory: destination.to_path_buf(),
            manifest_digest: report.manifest_digest,
            artifact_count: report.artifact_count,
        });
    }
    if binding.status != "prepared" {
        return Err(PreparationVerificationError::PreparationNotPrepared);
    }
    let case_reference = u64::try_from(binding.case_id)
        .map_err(|_| PreparationVerificationError::InvalidExportIdentity)?;
    let output_digest = Sha256Digest::from_hex(&binding.output_digest_hex)
        .map_err(|_| PreparationVerificationError::InvalidExportIdentity)?;
    let policy_bundle_digest = Sha256Digest::from_hex(&binding.policy_bundle_digest_hex)
        .map_err(|_| PreparationVerificationError::InvalidExportIdentity)?;
    let bytes = store.get(output_digest)?;
    let label = format!("preparation/{}", binding.id);
    let result = if destination.exists() {
        recover_existing_export(
            destination,
            case_reference,
            &label,
            output_digest,
            policy_bundle_digest,
        )?
    } else {
        export::write_export(
            destination,
            case_reference,
            Utc::now().timestamp(),
            policy_bundle_digest,
            &[ExportArtifact {
                label: &label,
                bytes: &bytes,
            }],
        )?
    };
    let manifest_digest =
        repository::upsert_digest(&mut tx, "sha256", &result.manifest_digest.to_string()).await?;
    if !repository::mark_preparation_exported(&mut tx, binding.id, manifest_digest).await? {
        return Err(PreparationVerificationError::PreparationNotPrepared);
    }
    nexo_app::audit::append(
        &mut tx,
        actor,
        Some(case),
        "preparation.exported",
        serde_json::json!({
            "preparation_id": binding.id,
            "manifest_digest": result.manifest_digest.to_string(),
            "artifact_count": result.artifact_count,
        }),
        serde_json::json!([]),
    )
    .await
    .map_err(repository::RepoError::from)?;
    tx.commit().await.map_err(repository::RepoError::from)?;
    Ok(result)
}

fn recover_existing_export(
    destination: &Path,
    case_reference: u64,
    expected_label: &str,
    output_digest: Sha256Digest,
    policy_bundle_digest: Sha256Digest,
) -> Result<ExportResult, PreparationVerificationError> {
    let manifest_path = destination.join("manifest.json");
    let report = nexo_verifier::verify_export(&manifest_path)
        .map_err(PreparationVerificationError::ExistingExportInvalid)?;
    let manifest: Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|_| PreparationVerificationError::ExportDestinationConflict)?,
    )
    .map_err(|_| PreparationVerificationError::ExportDestinationConflict)?;
    let artifacts = manifest["artifacts"]
        .as_array()
        .ok_or(PreparationVerificationError::ExportDestinationConflict)?;
    if report.case_reference != case_reference
        || manifest["policy_bundle_digest"].as_str() != Some(&policy_bundle_digest.to_string())
        || artifacts.len() != 1
        || artifacts[0]["label"].as_str() != Some(expected_label)
        || artifacts[0]["digest"].as_str() != Some(&output_digest.to_string())
    {
        return Err(PreparationVerificationError::ExportDestinationConflict);
    }
    Ok(ExportResult {
        directory: destination.to_path_buf(),
        manifest_digest: report.manifest_digest,
        artifact_count: report.artifact_count,
    })
}

#[cfg(test)]
mod export_recovery_tests {
    use super::*;

    #[test]
    fn adopts_matching_existing_export_after_filesystem_first_failure() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("export");
        let bytes = b"prepared bytes";
        let output_digest = hash_bytes(bytes);
        let policy_digest = hash_bytes(b"policy");
        export::write_export(
            &destination,
            7,
            1_700_000_000,
            policy_digest,
            &[ExportArtifact {
                label: "preparation/42",
                bytes,
            }],
        )
        .unwrap();

        let recovered = recover_existing_export(
            &destination,
            7,
            "preparation/42",
            output_digest,
            policy_digest,
        )
        .unwrap();
        assert_eq!(recovered.artifact_count, 1);
    }

    #[test]
    fn rejects_existing_export_with_different_preparation_identity() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("export");
        let policy_digest = hash_bytes(b"policy");
        export::write_export(
            &destination,
            7,
            1_700_000_000,
            policy_digest,
            &[ExportArtifact {
                label: "preparation/99",
                bytes: b"prepared bytes",
            }],
        )
        .unwrap();

        let result = recover_existing_export(
            &destination,
            7,
            "preparation/42",
            hash_bytes(b"prepared bytes"),
            policy_digest,
        );
        assert!(matches!(
            result,
            Err(PreparationVerificationError::ExportDestinationConflict)
        ));
    }
}
