use core::num::NonZeroU64;

use crate::{
    Abstention, ActionEvaluation, ActionOption, ActionStatus, Contraindicated, FactualSupport,
    InsufficientFacts, JurisdictionEvidenceId, NodeId, NonActionable, NormativeClaimId,
    OutOfJurisdiction, PolicyBundleId, PolicyFreshnessEvidenceId, PolicyNotCurrent, RequirementId,
};

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).expect("test IDs are non-zero"))
}

fn legal(value: u64) -> NormativeClaimId {
    NormativeClaimId::new(node(value))
}

/// An honest absence of legal authority is representable without fabricating a
/// NormativeClaim merely to satisfy ActionOption's actionable-route contract.
#[test]
fn abstention_without_legal_support_is_a_typed_non_actionable_result() {
    let abstention =
        Abstention::no_authoritative_legal_source(vec![FactualSupport::Observation(node(1))])
            .expect("bounded factual context is valid");
    let result = ActionEvaluation::NonActionable(NonActionable::Abstain(abstention));

    assert!(matches!(
        result,
        ActionEvaluation::NonActionable(NonActionable::Abstain(_))
    ));
}

/// An abstention still needs context: otherwise the system would be making an
/// unauditable statement about an unspecified question or case.
#[test]
fn abstention_requires_factual_context() {
    let result = Abstention::unsupported_question(vec![]);

    assert_eq!(
        result.unwrap_err(),
        crate::EvaluationError::MissingFactualContext
    );
}

/// Freshness is policy metadata, not case evidence. This result therefore does
/// not need an artifact or a NormativeClaim to honestly explain non-availability.
#[test]
fn stale_policy_is_typed_with_bundle_identity_and_freshness_evidence() {
    let bundle = PolicyBundleId::new(node(1));
    let freshness_evidence = PolicyFreshnessEvidenceId::new(node(2));
    let policy_not_current = PolicyNotCurrent::try_new(bundle, freshness_evidence)
        .expect("distinct bundle and freshness evidence are valid");
    let result =
        ActionEvaluation::NonActionable(NonActionable::PolicyNotCurrent(policy_not_current));

    assert!(matches!(
        result,
        ActionEvaluation::NonActionable(NonActionable::PolicyNotCurrent(_))
    ));
    assert_eq!(policy_not_current.policy_bundle(), bundle);
    assert_eq!(policy_not_current.freshness_evidence(), freshness_evidence);
}

/// Red-team remediation: distinct wrapper types alone are insufficient; the
/// constructor rejects a bundle asserted as its own freshness evidence.
#[test]
fn rejects_policy_bundle_self_attestation() {
    let same_node = node(1);
    let result = PolicyNotCurrent::try_new(
        PolicyBundleId::new(same_node),
        PolicyFreshnessEvidenceId::new(same_node),
    );

    assert_eq!(
        result.unwrap_err(),
        crate::EvaluationError::FreshnessEvidenceAliasesPolicyBundle
    );
}

/// Scope is established by jurisdiction evidence and policy-bundle scope, not
/// by pretending a case-specific normative claim has already been selected.
#[test]
fn out_of_jurisdiction_is_typed_with_scope_evidence_not_legal_action_support() {
    let jurisdiction_evidence = JurisdictionEvidenceId::new(node(1));
    let policy_bundle = PolicyBundleId::new(node(2));
    let out_of_jurisdiction = OutOfJurisdiction::try_new(jurisdiction_evidence, policy_bundle)
        .expect("distinct scope evidence and policy bundle are valid");
    let result =
        ActionEvaluation::NonActionable(NonActionable::OutOfJurisdiction(out_of_jurisdiction));

    assert!(matches!(
        result,
        ActionEvaluation::NonActionable(NonActionable::OutOfJurisdiction(_))
    ));
    assert_eq!(
        out_of_jurisdiction.jurisdiction_evidence(),
        jurisdiction_evidence
    );
    assert_eq!(out_of_jurisdiction.policy_bundle(), policy_bundle);
}

/// Red-team remediation: scope evidence must remain independent from the
/// policy bundle it constrains.
#[test]
fn rejects_policy_bundle_as_its_own_scope_evidence() {
    let same_node = node(1);
    let result = OutOfJurisdiction::try_new(
        JurisdictionEvidenceId::new(same_node),
        PolicyBundleId::new(same_node),
    );

    assert_eq!(
        result.unwrap_err(),
        crate::EvaluationError::JurisdictionEvidenceAliasesPolicyBundle
    );
}

/// A conditional route needs an actual stated condition; otherwise it is an
/// incoherent duplicate of `Supported`.
#[test]
fn conditionally_supported_action_requires_an_unmet_requirement() {
    let result = ActionOption::try_new(
        ActionStatus::ConditionallySupported,
        vec![FactualSupport::Observation(node(1))],
        vec![legal(2)],
        vec![],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::ConditionallySupportedWithoutUnmetRequirements
    );
}

/// Insufficient facts is not an unstructured "unknown": it names a legal
/// route and at least one requirement that remains unproven.
#[test]
fn insufficient_facts_requires_legal_context_and_a_missing_requirement() {
    let no_legal = InsufficientFacts::try_new(vec![], vec![RequirementId::new(node(1))]);
    let no_missing = InsufficientFacts::try_new(vec![legal(2)], vec![]);

    assert_eq!(
        no_legal.unwrap_err(),
        crate::EvaluationError::MissingLegalContext
    );
    assert_eq!(
        no_missing.unwrap_err(),
        crate::EvaluationError::MissingRequirement
    );
}

/// Contraindication is a negative legal conclusion, not an abstention; it
/// therefore retains both factual and legal support requirements.
#[test]
fn contraindication_requires_factual_and_legal_context() {
    let result = Contraindicated::try_new(vec![], vec![legal(2)]);

    assert_eq!(
        result.unwrap_err(),
        crate::EvaluationError::MissingFactualContext
    );
}

/// Red-team remediation: one source cannot be a conflict with itself.
#[test]
fn rejects_a_single_claim_as_a_legal_conflict() {
    let result = Abstention::conflicting_legal_claims(
        vec![FactualSupport::UserAssertion(node(1))],
        vec![legal(2)],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::EvaluationError::InsufficientConflictingLegalClaims
    );
}

/// A duplicated citation is still one claim; it cannot manufacture a legal
/// conflict by occupying two positions in the same vector.
#[test]
fn rejects_one_duplicated_claim_as_a_legal_conflict() {
    let claim = legal(2);
    let result = Abstention::conflicting_legal_claims(
        vec![FactualSupport::UserAssertion(node(1))],
        vec![claim, claim],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::EvaluationError::DuplicateConflictingLegalClaim
    );
}

/// The cardinality guard is not an artificial refusal: two distinct cited
/// claims remain representable for the policy layer to adjudicate later.
#[test]
fn accepts_two_distinct_claims_as_conflict_context() {
    let result = Abstention::conflicting_legal_claims(
        vec![FactualSupport::UserAssertion(node(1))],
        vec![legal(2), legal(3)],
    );

    assert!(result.is_ok());
}
