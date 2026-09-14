use crate::*;
use core::num::NonZeroU64;

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).unwrap())
}

fn fixture() -> (CaseProjection, PolicyBundle, NormativeContext, ActionRoute) {
    let bundle_id = PolicyBundleId::new(node(1));
    let source_id = NormativeSourceId::new(node(2));
    let claim_id = NormativeClaimId::new(node(3));
    let validity =
        ValidityInterval::try_new(CivilDate::try_new(2026, 1, 1).unwrap(), None).unwrap();
    let claim = NormativeClaim::try_new(
        claim_id,
        NonEmptyText::try_new("access right".into()).unwrap(),
        NonEmptyText::try_new("AR".into()).unwrap(),
        validity,
        bundle_id,
        vec![ClaimSourceSupport::new(source_id, SupportRole::Primary)],
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
        vec![claim_id],
        policy,
        capture,
    )
    .unwrap();
    let context = NormativeContext::try_new(vec![claim], vec![source]).unwrap();
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
