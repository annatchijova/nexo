//! Application-owned bridge from durable receipt evidence to the core
//! preparation capability. This module intentionally does not expose an HTTP
//! preparation endpoint yet; it proves the authority boundary first.

use std::num::NonZeroU64;
use std::collections::BTreeMap;

use nexo_app::repository::{self, ActionEvaluationRowId, CaseRowId, Pool};
use nexo_core::{ActionOption, NodeId, VerifiedPreparationSnapshot};
use nexo_integrity::{seal, CanonicalValue};

#[derive(Debug)]
pub enum PreparationVerificationError {
    Repository(repository::RepoError),
    CaseNotOwned,
    ReceiptNotFound,
    EvaluationNotSupported,
    InvalidDurableIdentity,
    ActionFingerprintMismatch,
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
