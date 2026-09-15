use crate::*;
use core::num::NonZeroU64;

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).unwrap())
}

fn fixture() -> (CaseProjection, PolicyBundle, NormativeContext, ActionRoute) {
    fixture_with_claim_jurisdiction("AR")
}

fn fixture_with_claim_jurisdiction(
    claim_jurisdiction: &str,
) -> (CaseProjection, PolicyBundle, NormativeContext, ActionRoute) {
    let bundle_id = PolicyBundleId::new(node(1));
    let source_id = NormativeSourceId::new(node(2));
    let claim_id = NormativeClaimId::new(node(3));
    let second_claim_id = NormativeClaimId::new(node(15));
    let validity =
        ValidityInterval::try_new(CivilDate::try_new(2026, 1, 1).unwrap(), None).unwrap();
    let claim = NormativeClaim::try_new(
        claim_id,
        NonEmptyText::try_new("access right".into()).unwrap(),
        NonEmptyText::try_new(claim_jurisdiction.into()).unwrap(),
        validity,
        bundle_id,
        vec![ClaimSourceSupport::new(source_id, SupportRole::Primary)],
    )
    .unwrap();
    let second_source_id = NormativeSourceId::new(node(16));
    let second_claim = NormativeClaim::try_new(
        second_claim_id,
        NonEmptyText::try_new("deletion right".into()).unwrap(),
        NonEmptyText::try_new(claim_jurisdiction.into()).unwrap(),
        validity,
        bundle_id,
        vec![ClaimSourceSupport::new(
            second_source_id,
            SupportRole::Primary,
        )],
    )
    .unwrap();
    let source = NormativeSource::new(
        source_id,
        AuthorityKind::PrimaryOfficial,
        NonEmptyText::try_new("issuer".into()).unwrap(),
        NonEmptyText::try_new("https://example.test".into()).unwrap(),
        AcquisitionChannel::OfficialApi,
        UtcInstant::from_unix_seconds(0),
        ArtifactId::new(node(4)),
        DigestId::new(node(5)),
        ProvenanceId::new(node(6)),
    );
    let second_source = NormativeSource::new(
        second_source_id,
        AuthorityKind::PrimaryOfficial,
        NonEmptyText::try_new("issuer".into()).unwrap(),
        NonEmptyText::try_new("https://example.test/deletion".into()).unwrap(),
        AcquisitionChannel::OfficialApi,
        UtcInstant::from_unix_seconds(0),
        ArtifactId::new(node(17)),
        DigestId::new(node(18)),
        ProvenanceId::new(node(19)),
    );
    let policy = SourcePolicy::try_new(
        vec![AuthorityKind::PrimaryOfficial],
        vec![AcquisitionChannel::OfficialApi],
    )
    .unwrap();
    let capture = unsafe {
        VerifiedCaptureAttestation::from_verified_capture(
            ArtifactId::new(node(7)),
            DigestId::new(node(8)),
            ProvenanceId::new(node(9)),
            UtcInstant::from_unix_seconds(0),
        )
    };
    let bundle = PolicyBundle::try_new(
        bundle_id,
        PolicySchemaVersion::CURRENT,
        PolicyVersion::try_new("v1".into()).unwrap(),
        JurisdictionCode::Argentina,
        validity,
        vec![claim_id, second_claim_id],
        policy,
        capture,
    )
    .unwrap();
    let context =
        NormativeContext::try_new(vec![claim, second_claim], vec![source, second_source]).unwrap();
    let route = ActionRoute::try_new(
        node(10),
        JurisdictionCode::Argentina,
        vec![claim_id],
        vec![],
    )
    .unwrap();
    let projection = CaseProjection::try_new(
        vec![FactualSupport::UserAssertion(node(11))],
        vec![],
        JurisdictionEvidenceId::new(node(12)),
        PolicyFreshnessEvidenceId::new(node(13)),
    )
    .unwrap();
    (projection, bundle, context, route)
}

#[test]
fn evaluator_returns_supported_for_current_source_complete_route() {
    let (projection, bundle, context, route) = fixture();
    assert!(matches!(
        evaluate(&projection, &bundle, &context, &route, CivilDate::try_new(2026, 2, 1).unwrap()),
        ActionEvaluation::Actionable(action) if action.is_available()
    ));
}

#[test]
fn evaluator_returns_insufficient_facts_for_missing_requirement() {
    let (projection, bundle, context, _) = fixture();
    let requirement = RequirementId::new(node(14));
    let route = ActionRoute::try_new(
        node(10),
        JurisdictionCode::Argentina,
        vec![NormativeClaimId::new(node(3))],
        vec![requirement],
    )
    .unwrap();
    assert!(matches!(
        evaluate(
            &projection,
            &bundle,
            &context,
            &route,
            CivilDate::try_new(2026, 2, 1).unwrap()
        ),
        ActionEvaluation::NonActionable(NonActionable::InsufficientFacts(_))
    ));
}

#[test]
fn evaluator_rejects_claim_from_another_jurisdiction() {
    let (projection, bundle, context, route) = fixture_with_claim_jurisdiction("US");
    assert_eq!(route.jurisdiction(), JurisdictionCode::Argentina);
    assert_eq!(bundle.jurisdiction(), JurisdictionCode::Argentina);
    assert!(matches!(
        evaluate(
            &projection,
            &bundle,
            &context,
            &route,
            CivilDate::try_new(2026, 2, 1).unwrap()
        ),
        ActionEvaluation::NonActionable(NonActionable::Abstain(_))
    ));
}

#[test]
fn evaluator_rejects_route_from_another_jurisdiction() {
    let (projection, bundle, context, _) = fixture();
    let route = ActionRoute::try_new(
        node(10),
        JurisdictionCode::UnitedStates,
        vec![NormativeClaimId::new(node(3))],
        vec![],
    )
    .unwrap();
    assert!(matches!(
        evaluate(
            &projection,
            &bundle,
            &context,
            &route,
            CivilDate::try_new(2026, 2, 1).unwrap()
        ),
        ActionEvaluation::NonActionable(NonActionable::OutOfJurisdiction(_))
    ));
}

#[test]
fn evaluator_emits_conflict_only_from_engine_relation() {
    let (projection, bundle, context, _) = fixture();
    let left = NormativeClaimId::new(node(3));
    let right = NormativeClaimId::new(node(15));
    let route = ActionRoute::try_new(
        node(10),
        JurisdictionCode::Argentina,
        vec![left, right],
        vec![],
    )
    .unwrap();
    let engine = PolicyRuleEngine::new(
        PolicyRuleEvaluationRecord::new(
            bundle.id(),
            bundle.policy_version().clone(),
            PolicyRuleId::new(node(20)),
        ),
        vec![(left, right)],
        vec![],
    );
    assert!(matches!(
        evaluate_with_negative_evidence(
            &projection,
            &bundle,
            &context,
            &route,
            CivilDate::try_new(2026, 2, 1).unwrap(),
            Some(&engine)
        ),
        ActionEvaluation::NonActionable(NonActionable::Abstain(
            Abstention::ConflictingLegalClaims(_)
        ))
    ));
}

#[test]
fn evaluator_conflict_precedes_contraindication_deterministically() {
    let (projection, bundle, context, _) = fixture();
    let left = NormativeClaimId::new(node(3));
    let right = NormativeClaimId::new(node(15));
    let trigger = RequirementId::new(node(21));
    let route = ActionRoute::try_new(
        node(10),
        JurisdictionCode::Argentina,
        vec![left, right],
        vec![trigger],
    )
    .unwrap();
    let engine = PolicyRuleEngine::new(
        PolicyRuleEvaluationRecord::new(
            bundle.id(),
            bundle.policy_version().clone(),
            PolicyRuleId::new(node(20)),
        ),
        vec![(left, right)],
        vec![(FactualSupport::UserAssertion(node(11)), left, trigger)],
    );
    assert!(matches!(
        evaluate_with_negative_evidence(
            &projection,
            &bundle,
            &context,
            &route,
            CivilDate::try_new(2026, 2, 1).unwrap(),
            Some(&engine)
        ),
        ActionEvaluation::NonActionable(NonActionable::Abstain(
            Abstention::ConflictingLegalClaims(_)
        ))
    ));
}

#[test]
fn evaluator_emits_contraindication_from_exact_fact_ground_trigger() {
    let (projection, bundle, context, _) = fixture();
    let trigger = RequirementId::new(node(21));
    let route = ActionRoute::try_new(
        node(10),
        JurisdictionCode::Argentina,
        vec![NormativeClaimId::new(node(3))],
        vec![trigger],
    )
    .unwrap();
    let engine = PolicyRuleEngine::new(
        PolicyRuleEvaluationRecord::new(
            bundle.id(),
            bundle.policy_version().clone(),
            PolicyRuleId::new(node(22)),
        ),
        vec![],
        vec![(
            FactualSupport::UserAssertion(node(11)),
            NormativeClaimId::new(node(3)),
            trigger,
        )],
    );
    assert!(matches!(
        evaluate_with_negative_evidence(
            &projection,
            &bundle,
            &context,
            &route,
            CivilDate::try_new(2026, 2, 1).unwrap(),
            Some(&engine)
        ),
        ActionEvaluation::NonActionable(NonActionable::Contraindicated(_))
    ));
}
