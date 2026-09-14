use crate::{
    AcquisitionChannel, ArtifactId, AuthorityKind, CaptureStatus, CivilDate, DigestId,
    JurisdictionCode, NodeId, NormativeClaimId, NormativeSource, NormativeSourceId, PolicyBundle,
    PolicyBundleError, PolicyBundleId, PolicyBundleSet, PolicyBundleSetError, PolicySchemaVersion,
    PolicySelection, PolicyVersion, ProvenanceId, SourcePolicy, SourcePolicyError, UtcInstant,
    ValidityInterval,
};
use core::num::NonZeroU64;

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).unwrap())
}

fn bundle(
    id: u64,
    jurisdiction: JurisdictionCode,
    from: (i32, u8, u8),
    to: Option<(i32, u8, u8)>,
) -> PolicyBundle {
    let validity = ValidityInterval::try_new(
        CivilDate::try_new(from.0, from.1, from.2).unwrap(),
        to.map(|date| CivilDate::try_new(date.0, date.1, date.2).unwrap()),
    )
    .unwrap();
    let policy_version = PolicyVersion::try_new(format!("v{id}")).unwrap();
    let source_policy = SourcePolicy::try_new(
        vec![AuthorityKind::PrimaryOfficial],
        vec![AcquisitionChannel::OfficialApi],
    )
    .unwrap();
    PolicyBundle::try_new(
        PolicyBundleId::new(node(id)),
        PolicySchemaVersion::CURRENT,
        policy_version,
        jurisdiction,
        validity,
        vec![NormativeClaimId::new(node(id + 100))],
        source_policy,
        ArtifactId::new(node(id + 200)),
        DigestId::new(node(id + 300)),
        ProvenanceId::new(node(id + 400)),
        UtcInstant::from_unix_seconds(0),
        CaptureStatus::Verified,
    )
    .unwrap()
}

#[test]
fn selects_the_only_current_bundle() {
    let set = PolicyBundleSet::try_new(vec![bundle(
        1,
        JurisdictionCode::Argentina,
        (2026, 1, 1),
        None,
    )])
    .unwrap();
    assert!(matches!(
        set.select(
            JurisdictionCode::Argentina,
            CivilDate::try_new(2026, 1, 1).unwrap()
        ),
        PolicySelection::Selected { .. }
    ));
}

#[test]
fn selection_is_independent_of_bundle_insertion_order() {
    let first = bundle(1, JurisdictionCode::Argentina, (2026, 1, 1), None);
    let second = bundle(2, JurisdictionCode::UnitedStates, (2026, 1, 1), None);
    let left = PolicyBundleSet::try_new(vec![first.clone(), second.clone()])
        .unwrap()
        .select(
            JurisdictionCode::Argentina,
            CivilDate::try_new(2026, 4, 1).unwrap(),
        );
    let right = PolicyBundleSet::try_new(vec![second, first])
        .unwrap()
        .select(
            JurisdictionCode::Argentina,
            CivilDate::try_new(2026, 4, 1).unwrap(),
        );
    assert_eq!(left, right);
}

#[test]
fn stale_and_wrong_jurisdiction_are_distinct_results() {
    let set = PolicyBundleSet::try_new(vec![bundle(
        1,
        JurisdictionCode::Argentina,
        (2025, 1, 1),
        Some((2025, 12, 31)),
    )])
    .unwrap();
    let date = CivilDate::try_new(2026, 1, 1).unwrap();
    assert_eq!(
        set.select(JurisdictionCode::Argentina, date),
        PolicySelection::PolicyNotCurrent
    );
    assert_eq!(
        set.select(JurisdictionCode::UnitedStates, date),
        PolicySelection::OutOfJurisdiction
    );
}

#[test]
fn overlapping_current_bundles_fail_closed_as_ambiguous() {
    let set = PolicyBundleSet::try_new(vec![
        bundle(1, JurisdictionCode::Argentina, (2026, 1, 1), None),
        bundle(2, JurisdictionCode::Argentina, (2026, 1, 1), None),
    ])
    .unwrap();
    assert_eq!(
        set.select(
            JurisdictionCode::Argentina,
            CivilDate::try_new(2026, 1, 1).unwrap()
        ),
        PolicySelection::AmbiguousSelection
    );
}

#[test]
fn unverified_capture_cannot_construct_a_bundle() {
    let result = PolicyBundle::try_new(
        bundle(1, JurisdictionCode::Argentina, (2026, 1, 1), None).id(),
        PolicySchemaVersion::CURRENT,
        PolicyVersion::try_new("v1".into()).unwrap(),
        JurisdictionCode::Argentina,
        ValidityInterval::try_new(CivilDate::try_new(2026, 1, 1).unwrap(), None).unwrap(),
        vec![NormativeClaimId::new(node(101))],
        SourcePolicy::try_new(
            vec![AuthorityKind::PrimaryOfficial],
            vec![AcquisitionChannel::OfficialApi],
        )
        .unwrap(),
        ArtifactId::new(node(201)),
        DigestId::new(node(301)),
        ProvenanceId::new(node(401)),
        UtcInstant::from_unix_seconds(0),
        CaptureStatus::Unverified,
    );
    assert_eq!(result.unwrap_err(), PolicyBundleError::CaptureNotVerified);
}

#[test]
fn source_policy_rejects_duplicates_and_empty_dimensions() {
    assert_eq!(
        SourcePolicy::try_new(vec![], vec![AcquisitionChannel::OfficialApi]).unwrap_err(),
        SourcePolicyError::Empty
    );
    assert_eq!(
        SourcePolicy::try_new(
            vec![
                AuthorityKind::PrimaryOfficial,
                AuthorityKind::PrimaryOfficial
            ],
            vec![AcquisitionChannel::OfficialApi],
        )
        .unwrap_err(),
        SourcePolicyError::DuplicateEntry
    );
    assert_eq!(
        SourcePolicy::try_new(
            vec![AuthorityKind::Unverified],
            vec![AcquisitionChannel::OfficialApi],
        )
        .unwrap_err(),
        SourcePolicyError::UnverifiedAuthority
    );
}

#[test]
fn bundle_source_eligibility_keeps_authority_and_channel_independent() {
    let source = NormativeSource::new(
        NormativeSourceId::new(node(1)),
        AuthorityKind::PrimaryOfficial,
        crate::NonEmptyText::try_new("Issuer".into()).unwrap(),
        crate::NonEmptyText::try_new("https://example.test".into()).unwrap(),
        AcquisitionChannel::OfficialApi,
        UtcInstant::from_unix_seconds(0),
        ArtifactId::new(node(2)),
        DigestId::new(node(3)),
        ProvenanceId::new(node(4)),
    );
    let policy = SourcePolicy::try_new(
        vec![AuthorityKind::PrimaryOfficial],
        vec![AcquisitionChannel::OfficialApi],
    )
    .unwrap();
    assert!(policy.source_is_eligible(&source));
}

#[test]
fn bundles_reject_duplicate_claims_and_unknown_schema() {
    let source_policy = SourcePolicy::try_new(
        vec![AuthorityKind::PrimaryOfficial],
        vec![AcquisitionChannel::OfficialApi],
    )
    .unwrap();
    let validity =
        ValidityInterval::try_new(CivilDate::try_new(2026, 1, 1).unwrap(), None).unwrap();
    let common = (
        PolicyBundleId::new(node(1)),
        PolicyVersion::try_new("v1".into()).unwrap(),
        JurisdictionCode::Argentina,
        validity,
        source_policy,
        ArtifactId::new(node(2)),
        DigestId::new(node(3)),
        ProvenanceId::new(node(4)),
        UtcInstant::from_unix_seconds(0),
    );
    let duplicate = PolicyBundle::try_new(
        common.0,
        PolicySchemaVersion::CURRENT,
        common.1.clone(),
        common.2,
        common.3,
        vec![
            NormativeClaimId::new(node(5)),
            NormativeClaimId::new(node(5)),
        ],
        common.4.clone(),
        common.5,
        common.6,
        common.7,
        common.8,
        CaptureStatus::Verified,
    );
    assert_eq!(duplicate.unwrap_err(), PolicyBundleError::DuplicateClaim);
    assert_eq!(
        PolicySchemaVersion::try_new(0).unwrap_err(),
        crate::PolicySchemaVersionError::Zero
    );
}

#[test]
fn empty_bundle_set_is_a_typed_no_bundle_result() {
    let set = PolicyBundleSet::try_new(vec![]).unwrap();
    assert_eq!(
        set.select(
            JurisdictionCode::Argentina,
            CivilDate::try_new(2026, 1, 1).unwrap()
        ),
        PolicySelection::NoBundle
    );
}

#[test]
fn bundle_set_rejects_duplicate_bundle_identity() {
    let first = bundle(1, JurisdictionCode::Argentina, (2026, 1, 1), None);
    let second = first.clone();
    assert_eq!(
        PolicyBundleSet::try_new(vec![first, second]).unwrap_err(),
        PolicyBundleSetError::DuplicateBundle
    );
}
