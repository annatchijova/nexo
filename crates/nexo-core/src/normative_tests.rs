use crate::{
    AcquisitionChannel, ArtifactId, AuthorityKind, CivilDate, ClaimSourceSupport, DigestId, NodeId,
    NonEmptyText, NormativeClaim, NormativeClaimError, NormativeClaimId, NormativeSource,
    NormativeSourceId, PolicyBundleId, ProvenanceId, SupportRole, TextError, UtcInstant,
    ValidityInterval,
};
use core::num::NonZeroU64;
fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).unwrap())
}
#[test]
fn source_requires_non_empty_issuer() {
    assert_eq!(NonEmptyText::try_new(" ".into()), Err(TextError::Empty));
}
#[test]
fn authority_and_channel_are_independent() {
    let source = NormativeSource::new(
        NormativeSourceId::new(node(1)),
        AuthorityKind::PrimaryOfficial,
        NonEmptyText::try_new("Congress".into()).unwrap(),
        NonEmptyText::try_new("https://example.test".into()).unwrap(),
        AcquisitionChannel::WebFetch,
        UtcInstant::from_unix_seconds(0),
        ArtifactId::new(node(2)),
        DigestId::new(node(3)),
        ProvenanceId::new(node(4)),
    );
    assert_eq!(source.authority_kind(), AuthorityKind::PrimaryOfficial);
    assert_eq!(source.acquisition_channel(), AcquisitionChannel::WebFetch);
}
#[test]
fn claim_requires_distinct_source_support_and_allows_many_sources() {
    let from = CivilDate::try_new(2026, 1, 1).unwrap();
    let validity = ValidityInterval::try_new(from, None).unwrap();
    let result = NormativeClaim::try_new(
        NormativeClaimId::new(node(1)),
        NonEmptyText::try_new("Access right".into()).unwrap(),
        NonEmptyText::try_new("US".into()).unwrap(),
        validity,
        PolicyBundleId::new(node(2)),
        vec![
            ClaimSourceSupport::new(NormativeSourceId::new(node(3)), SupportRole::Primary),
            ClaimSourceSupport::new(NormativeSourceId::new(node(4)), SupportRole::Corroborating),
        ],
    );
    assert!(result.is_ok());
}
#[test]
fn claim_rejects_duplicate_source_reference() {
    let source = ClaimSourceSupport::new(NormativeSourceId::new(node(3)), SupportRole::Primary);
    let validity =
        ValidityInterval::try_new(CivilDate::try_new(2026, 1, 1).unwrap(), None).unwrap();
    let result = NormativeClaim::try_new(
        NormativeClaimId::new(node(1)),
        NonEmptyText::try_new("Right".into()).unwrap(),
        NonEmptyText::try_new("US".into()).unwrap(),
        validity,
        PolicyBundleId::new(node(2)),
        vec![source, source],
    );
    assert_eq!(result.unwrap_err(), NormativeClaimError::DuplicateSource);
}
