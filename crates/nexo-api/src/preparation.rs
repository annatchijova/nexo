//! Application-owned bridge from durable receipt evidence to the core
//! preparation capability. This module intentionally does not expose an HTTP
//! preparation endpoint yet; it proves the authority boundary first.

use std::num::NonZeroU64;
use std::collections::BTreeMap;

use nexo_app::object_store::{FilesystemObjectStore, ObjectStoreError};
use nexo_app::repository::{self, ActionEvaluationRowId, CaseRowId, Pool, ProvenanceRowId};
use nexo_core::{ActionOption, NodeId, VerifiedPreparationSnapshot};
use nexo_integrity::{seal, CanonicalValue};
use serde_json::json;

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
        CanonicalValue::Text(match action.status() {
            nexo_core::ActionStatus::Supported => "supported",
            nexo_core::ActionStatus::ConditionallySupported => "conditionally_supported",
        }
        .into()),
    );
    fields.insert("factual_support".into(), CanonicalValue::List(factual_support));
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

    Ok(VerifiedPreparationSnapshot::from_application_verified_evaluation(
        action.with_identity(action_identity),
        evaluation_snapshot,
        policy_bundle_digest,
        input_manifest_digest,
    ))
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
    let mut tx = pool
        .begin()
        .await
        .map_err(repository::RepoError::from)?;
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
    if let Some(existing) = repository::find_active_preparation(
        &mut tx,
        evaluation,
        kind,
        generator_version,
    )
    .await?
    {
        return Ok(existing);
    }
    let output_digest = store.put(bytes)?;
    let output_digest = repository::upsert_digest(&mut tx, "sha256", &output_digest.to_string())
        .await?;
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
    tx.commit()
        .await
        .map_err(repository::RepoError::from)?;
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
    let mut tx = pool
        .begin()
        .await
        .map_err(repository::RepoError::from)?;
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
    if let Some(existing) = repository::find_active_preparation(
        &mut tx,
        evaluation,
        kind,
        generator_version,
    )
    .await?
    {
        return Ok(existing);
    }
    let output_digest = store.put(bytes)?;
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
    let output_digest = repository::upsert_digest(&mut tx, "sha256", &output_digest.to_string())
        .await?;
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
    tx.commit()
        .await
        .map_err(repository::RepoError::from)?;
    Ok(preparation)
}
