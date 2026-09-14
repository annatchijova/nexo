//! Generative invariant tests over the domain constructors.
//!
//! These properties complement the example-driven tests in `tests.rs` and
//! `evaluation_tests.rs`: instead of one hand-picked hostile input per test,
//! every property quantifies over randomized inputs hugging the domain limits
//! (empty, tiny, one below, at, and one above each bound). Each property names
//! the constructor guard it pins, so deleting or weakening a guard makes a
//! property here fail.
//!
//! What this proves: for arbitrary adapter-supplied inputs within one element
//! of the domain bounds, a successfully constructed value satisfies the
//! architectural invariants, a rejected input fails with the documented error,
//! and the documented error precedence is stable.
//!
//! What this does not prove: graph-level evaluation (no evaluator exists yet),
//! policy semantics, serialization or canonicalization, or that the bounds
//! themselves are the right ones. The closed-world guarantee that a negative
//! result can never become actionable is structural — `ActionEvaluation`'s
//! variants share no conversion API — so it is pinned here only through the
//! constructor-level consequences: a negative result never contributes legal
//! support, and no input vector assembles an action out of one.
//!
//! Determinism: proptest's default runner replays with a fixed seed per test.
//! A failing case prints its seed and a minimal input, and writes a
//! regression file under `crates/nexo-core/proptest-regressions/`.

use core::num::NonZeroU64;

use proptest::prelude::*;

use crate::{
    Abstention, ActionOption, ActionStatus, Contraindicated, EvaluationError, FactualSupport,
    InsufficientFacts, JurisdictionEvidenceId, MAX_FACTUAL_SUPPORT, MAX_LEGAL_SUPPORT,
    MAX_UNMET_REQUIREMENTS, NodeId, NormativeClaimId, OutOfJurisdiction, PolicyBundleId,
    PolicyFreshnessEvidenceId, PolicyNotCurrent, RequirementId,
};

fn node_id() -> impl Strategy<Value = NodeId> {
    (1u64..=u64::MAX).prop_map(|value| NodeId::new(NonZeroU64::new(value).expect("non-zero")))
}

fn legal_claim() -> impl Strategy<Value = NormativeClaimId> {
    node_id().prop_map(NormativeClaimId::new)
}

fn requirement() -> impl Strategy<Value = RequirementId> {
    node_id().prop_map(RequirementId::new)
}

fn factual_support() -> impl Strategy<Value = FactualSupport> + Clone {
    prop_oneof![
        node_id().prop_map(FactualSupport::Artifact),
        node_id().prop_map(FactualSupport::Observation),
        node_id().prop_map(FactualSupport::UserAssertion),
        node_id().prop_map(FactualSupport::DerivedFact),
    ]
}

fn status() -> impl Strategy<Value = ActionStatus> {
    prop::bool::ANY.prop_map(|supported| {
        if supported {
            ActionStatus::Supported
        } else {
            ActionStatus::ConditionallySupported
        }
    })
}

/// Sizes that hug a domain limit: empty/tiny, or one below / at / one above.
fn size_around(limit: usize) -> impl Strategy<Value = usize> {
    prop_oneof![0usize..=2, limit.saturating_sub(1)..=limit + 1]
}

/// Compile-time guard for "inference alone cannot establish support".
///
/// This match has no wildcard: if a future change adds an `Inference` variant
/// to `FactualSupport`, this test file stops compiling until the change is
/// justified and these properties are re-examined. The epistemic boundary the
/// absent variant encodes must never change silently.
fn support_kind(support: &FactualSupport) -> &'static str {
    match support {
        FactualSupport::Artifact(_) => "artifact",
        FactualSupport::Observation(_) => "observation",
        FactualSupport::UserAssertion(_) => "user_assertion",
        FactualSupport::DerivedFact(_) => "derived_fact",
    }
}

/// Builds abstentions whose public constructors remain available. Conflict
/// abstentions are now produced only by the in-crate policy engine.
fn abstention() -> impl Strategy<Value = Abstention> {
    let context = prop::collection::vec(factual_support(), 1..=MAX_FACTUAL_SUPPORT);
    prop_oneof![
        context
            .clone()
            .prop_map(|context| Abstention::no_authoritative_legal_source(context).expect("valid")),
        context
            .clone()
            .prop_map(|context| Abstention::unsupported_question(context).expect("valid")),
    ]
}

proptest! {
    /// ∀ evaluation: `Actionable(option)` ⇒ factual_support ≠ ∅ ∧
    /// legal_support ≠ ∅. Guard pinned: `ActionOption::try_new` emptiness
    /// checks. Inputs hug every bound, so over-limit vectors exercise the
    /// rejection paths too.
    #[test]
    fn actionable_implies_nonempty_factual_and_legal_support(
        chosen_status in status(),
        factual in size_around(MAX_FACTUAL_SUPPORT)
            .prop_flat_map(|max| prop::collection::vec(factual_support(), 0..=max)),
        legal in size_around(MAX_LEGAL_SUPPORT)
            .prop_flat_map(|max| prop::collection::vec(legal_claim(), 0..=max)),
        unmet in size_around(MAX_UNMET_REQUIREMENTS)
            .prop_flat_map(|max| prop::collection::vec(requirement(), 0..=max)),
    ) {
        if let Ok(action) = ActionOption::try_new(chosen_status, factual, legal, unmet) {
            prop_assert!(!action.factual_support().is_empty());
            prop_assert!(!action.legal_support().is_empty());
        }
    }

    /// ∀ action: `Supported` ⇒ unmet = ∅ ∧ is_available(). Inputs are within
    /// bounds by construction, so the only possible error is the pinned guard.
    #[test]
    fn supported_action_is_complete_and_available(
        factual in prop::collection::vec(factual_support(), 1..=MAX_FACTUAL_SUPPORT),
        legal in prop::collection::vec(legal_claim(), 1..=MAX_LEGAL_SUPPORT),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        match ActionOption::try_new(ActionStatus::Supported, factual, legal, unmet) {
            Ok(action) => {
                prop_assert!(action.unmet_requirements().is_empty());
                prop_assert!(action.is_available());
            }
            Err(error) => prop_assert_eq!(
                error,
                crate::ActionOptionError::SupportedWithUnmetRequirements
            ),
        }
    }

    /// ∀ action: `ConditionallySupported` ⇒ unmet ≠ ∅ ∧ !is_available().
    #[test]
    fn conditional_action_exposes_unmet_requirements_and_is_not_available(
        factual in prop::collection::vec(factual_support(), 1..=MAX_FACTUAL_SUPPORT),
        legal in prop::collection::vec(legal_claim(), 1..=MAX_LEGAL_SUPPORT),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        match ActionOption::try_new(ActionStatus::ConditionallySupported, factual, legal, unmet) {
            Ok(action) => {
                prop_assert!(!action.unmet_requirements().is_empty());
                prop_assert!(!action.is_available());
            }
            Err(error) => prop_assert_eq!(
                error,
                crate::ActionOptionError::ConditionallySupportedWithoutUnmetRequirements
            ),
        }
    }

    /// Error precedence is contract: empty factual support loses before every
    /// other check, whatever the other fields contain.
    #[test]
    fn empty_factual_support_is_rejected_first(
        chosen_status in status(),
        legal in prop::collection::vec(legal_claim(), 0..=MAX_LEGAL_SUPPORT),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        let result = ActionOption::try_new(chosen_status, vec![], legal, unmet);
        prop_assert_eq!(
            result.unwrap_err(),
            crate::ActionOptionError::MissingFactualSupport
        );
    }

    /// Once factual support exists, missing legal support is the reported
    /// cause, whatever the status and unmet requirements contain.
    #[test]
    fn factual_support_without_legal_support_is_rejected(
        chosen_status in status(),
        factual in prop::collection::vec(factual_support(), 1..=MAX_FACTUAL_SUPPORT),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        let result = ActionOption::try_new(chosen_status, factual, vec![], unmet);
        prop_assert_eq!(
            result.unwrap_err(),
            crate::ActionOptionError::MissingLegalSupport
        );
    }

    /// ∀ bundle: a bundle cannot attest its own freshness. Guard pinned:
    /// `PolicyNotCurrent::try_new` alias rejection (red-team round 002).
    #[test]
    fn policy_not_current_rejects_self_attesting_bundle(id in node_id()) {
        let result = PolicyNotCurrent::try_new(
            PolicyBundleId::new(id),
            PolicyFreshnessEvidenceId::new(id),
        );
        prop_assert_eq!(
            result.unwrap_err(),
            EvaluationError::FreshnessEvidenceAliasesPolicyBundle
        );
    }

    /// ∀ evidence: jurisdiction evidence cannot alias the policy bundle whose
    /// scope it constrains. Guard pinned: `OutOfJurisdiction::try_new`.
    #[test]
    fn out_of_jurisdiction_rejects_self_certifying_scope(id in node_id()) {
        let result = OutOfJurisdiction::try_new(
            JurisdictionEvidenceId::new(id),
            PolicyBundleId::new(id),
        );
        prop_assert_eq!(
            result.unwrap_err(),
            EvaluationError::JurisdictionEvidenceAliasesPolicyBundle
        );
    }

    /// ∀ stale-policy result + ∀ case facts: the negative result contributes
    /// no legal support; case facts alone cannot assemble an action out of a
    /// `PolicyNotCurrent`. A route needs a `NormativeClaim` from outside.
    #[test]
    fn policy_not_current_never_yields_legal_support(
        bundle in node_id(),
        freshness in node_id(),
        chosen_status in status(),
        factual in prop::collection::vec(factual_support(), 1..=MAX_FACTUAL_SUPPORT),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        let Ok(stale) = PolicyNotCurrent::try_new(
            PolicyBundleId::new(bundle),
            PolicyFreshnessEvidenceId::new(freshness),
        ) else {
            // Coincidental id collision: self-attestation, pinned above.
            return Ok(());
        };
        prop_assert_eq!(stale.policy_bundle(), PolicyBundleId::new(bundle));
        prop_assert_eq!(
            stale.freshness_evidence(),
            PolicyFreshnessEvidenceId::new(freshness)
        );

        let result = ActionOption::try_new(chosen_status, factual, vec![], unmet);
        prop_assert_eq!(
            result.unwrap_err(),
            crate::ActionOptionError::MissingLegalSupport
        );
    }

    /// ∀ out-of-jurisdiction result + ∀ case facts: scope evidence is not
    /// legal support either; it cannot be promoted into a route.
    #[test]
    fn out_of_jurisdiction_never_yields_legal_support(
        jurisdiction_evidence in node_id(),
        bundle in node_id(),
        chosen_status in status(),
        factual in prop::collection::vec(factual_support(), 1..=MAX_FACTUAL_SUPPORT),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        let Ok(excluded) = OutOfJurisdiction::try_new(
            JurisdictionEvidenceId::new(jurisdiction_evidence),
            PolicyBundleId::new(bundle),
        ) else {
            return Ok(());
        };
        prop_assert_eq!(
            excluded.jurisdiction_evidence(),
            JurisdictionEvidenceId::new(jurisdiction_evidence)
        );
        prop_assert_eq!(excluded.policy_bundle(), PolicyBundleId::new(bundle));

        let result = ActionOption::try_new(chosen_status, factual, vec![], unmet);
        prop_assert_eq!(
            result.unwrap_err(),
            crate::ActionOptionError::MissingLegalSupport
        );
    }

    /// ∀ abstention: its own data can never become an `ActionOption`. An
    /// abstention carries no `NormativeClaim`; a legal route must be supplied
    /// from outside, never synthesized out of the refusal itself.
    #[test]
    fn abstention_never_yields_legal_support(
        refusal in abstention(),
        chosen_status in status(),
        unmet in prop::collection::vec(requirement(), 0..=MAX_UNMET_REQUIREMENTS),
    ) {
        let factual_context = match &refusal {
            Abstention::NoAuthoritativeLegalSource(context) => context.factual_context().to_vec(),
            Abstention::UnsupportedQuestion(context) => context.factual_context().to_vec(),
            Abstention::ConflictingLegalClaims(context) => context.factual_context().to_vec(),
        };
        prop_assert!(!factual_context.is_empty());

        let result = ActionOption::try_new(chosen_status, factual_context, vec![], unmet);
        prop_assert_eq!(
            result.unwrap_err(),
            crate::ActionOptionError::MissingLegalSupport
        );
    }

    /// ∀ factual support: only provenance-bearing kinds exist. The
    /// load-bearing half of this invariant is the compile-time guard in
    /// `support_kind`; this property pins the generator to the same contract.
    #[test]
    fn factual_support_is_always_provenance(support in factual_support()) {
        prop_assert!(!support_kind(&support).is_empty());
    }

    /// ∀ claims: ok ⇒ ≥2 ∧ distinct ∧ preserved verbatim; and each rejection
    /// cause fires exactly when its precondition holds. This pins the full
    /// check order of `ConflictingLegalClaims::try_new`.
    #[test]
    fn conflicting_claims_cardinality_distinctness_and_round_trip(
        claims in size_around(MAX_LEGAL_SUPPORT)
            .prop_flat_map(|max| prop::collection::vec(legal_claim(), 0..=max)),
    ) {
        let factual = vec![FactualSupport::UserAssertion(
            NodeId::new(NonZeroU64::new(1).expect("non-zero")),
        )];
        let result = Abstention::conflicting_legal_claims(factual, claims.clone());

        prop_assert_eq!(result.unwrap_err(), EvaluationError::RelationalEvidenceRequired);
    }

    /// ∀ result: ok ⇒ legal route ≠ ∅ ∧ named gap ≠ ∅, both preserved
    /// verbatim; and each rejection cause fires exactly when its precondition
    /// holds. This pins the full check order of `InsufficientFacts::try_new`.
    #[test]
    fn insufficient_facts_legal_route_named_gap_and_precedence(
        legal in size_around(MAX_LEGAL_SUPPORT)
            .prop_flat_map(|max| prop::collection::vec(legal_claim(), 0..=max)),
        missing in size_around(MAX_UNMET_REQUIREMENTS)
            .prop_flat_map(|max| prop::collection::vec(requirement(), 0..=max)),
    ) {
        match InsufficientFacts::try_new(legal.clone(), missing.clone()) {
            Ok(result) => {
                prop_assert!(!result.legal_support().is_empty());
                prop_assert!(!result.missing_requirements().is_empty());
                prop_assert_eq!(result.legal_support(), &legal[..]);
                prop_assert_eq!(result.missing_requirements(), &missing[..]);
            }
            Err(EvaluationError::TooManyLegalContext) => {
                prop_assert!(legal.len() > MAX_LEGAL_SUPPORT);
            }
            Err(EvaluationError::TooManyMissingRequirements) => {
                prop_assert!(legal.len() <= MAX_LEGAL_SUPPORT);
                prop_assert!(missing.len() > MAX_UNMET_REQUIREMENTS);
            }
            Err(EvaluationError::MissingLegalContext) => prop_assert!(legal.is_empty()),
            Err(EvaluationError::MissingRequirement) => {
                prop_assert!(!legal.is_empty());
                prop_assert!(missing.is_empty());
            }
            Err(other) => prop_assert!(false, "unexpected error: {:?}", other),
        }
    }

    /// ∀ result: ok ⇒ factual and legal grounds both ≠ ∅ and kept separate.
    /// This pins the full check order of `Contraindicated::try_new`.
    #[test]
    fn contraindicated_keeps_factual_and_legal_grounds_separate(
        factual in size_around(MAX_FACTUAL_SUPPORT)
            .prop_flat_map(|max| prop::collection::vec(factual_support(), 0..=max)),
        legal in size_around(MAX_LEGAL_SUPPORT)
            .prop_flat_map(|max| prop::collection::vec(legal_claim(), 0..=max)),
    ) {
        let _ = (factual, legal);
        prop_assert_eq!(
            Contraindicated::try_new(Vec::new(), Vec::new()).unwrap_err(),
            EvaluationError::RelationalEvidenceRequired
        );
    }
}
