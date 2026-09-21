//! Argentina policy bundle for Ley 25.326 (Protección de los Datos
//! Personales), articles 14 (derecho de acceso) and 16 (derecho de
//! rectificación, actualización o supresión).
//!
//! Contract: `docs/POLICY_BUNDLE_AR_DATA_ACCESS_CONTRACT.md`. The captured
//! source in `sources/ley_25326_texact.html` is the real bytes fetched from
//! InfoLEG (Argentina's official legislative information service) — not a
//! paraphrase or summary — and this module's fixtures always attest that
//! exact file through `nexo_policy_bridge::attest_capture`, the same bridge
//! any adapter would use, so a mutated or truncated copy of the source
//! fails to build a bundle here exactly as it would in production.

use core::num::NonZeroU64;

use nexo_core::{
    AcquisitionChannel, ActionRoute, ArtifactId, AuthorityKind, CaseProjection, CivilDate,
    ClaimSourceSupport, DigestId, FactualSupport, JurisdictionCode, JurisdictionEvidenceId,
    NodeId, NonEmptyText, NormativeClaim, NormativeClaimId, NormativeContext, NormativeSource,
    NormativeSourceId,
    PolicyBundle, PolicyBundleId, PolicyFreshnessEvidenceId, PolicySchemaVersion, PolicyVersion,
    ProvenanceId, RequirementId, SourcePolicy, SupportRole, UtcInstant, ValidityInterval,
};

pub const CAPTURED_SOURCE_BYTES: &[u8] = include_bytes!("../sources/ley_25326_texact.html");
pub const CAPTURED_SOURCE_LOCATOR: &str =
    "http://servicios.infoleg.gob.ar/infolegInternet/anexos/60000-64999/64790/texact.htm";
pub const CAPTURED_SOURCE_ISSUER: &str = "InfoLEG - Ministerio de Justicia (Boletín Oficial)";

const CLAIM_ACCESS_PROPOSITION: &str =
    "Art. 14, Ley 25.326: derecho de acceso a los propios datos personales, \
     previa acreditación de identidad, dentro de los diez días corridos de \
     intimado el responsable.";
const CLAIM_RECTIFICATION_PROPOSITION: &str =
    "Art. 16, Ley 25.326: derecho a que sean rectificados, actualizados o, \
     cuando corresponda, suprimidos los datos personales, dentro de los \
     cinco días hábiles de recibido el reclamo.";

/// The human-readable citation behind one `NormativeClaimId`: this is what
/// answers "why is this shown to me?" for a route that cites this claim.
/// `nexo_core::NormativeClaim` deliberately exposes no proposition-text
/// getter (its public surface is the minimal set `nexo-core` itself needs);
/// this crate is the one place that knows the mapping, because it is the
/// one place that wrote the text in the first place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimCitation {
    pub proposition: &'static str,
    pub source_issuer: &'static str,
    pub source_locator: &'static str,
}

/// The digest of the file this crate captured at
/// `sources/ley_25326_texact.html`, recorded once as a literal and never
/// recomputed from that same file to check itself. This is what makes the
/// capture check in `build()` meaningful: comparing a freshly hashed file
/// against a digest computed from that identical, possibly-since-tampered
/// file would always pass, no matter what the file currently contains. A
/// red-team pass confirmed this the hard way — see
/// docs/RED_TEAM_ROUND_001.md, finding RT-001-02 — by tampering a legal
/// deadline in the committed source file and observing every test still
/// pass under the old (self-referential) check. Recorded in
/// docs/POLICY_BUNDLE_AR_DATA_ACCESS_CONTRACT.md as the source capture's
/// citable digest.
pub const CAPTURED_SOURCE_SHA256_HEX: &str =
    "61548a0fba22550e19a4189c101876238501cb6a9b5821cea5e36478708a3b61";

/// This bundle's own version/currency window start. Distinct from the
/// underlying statute's effective date (`LAW_PROMULGATED`): this is when
/// *this captured bundle* was built and activated, per
/// `docs/POLICY_BUNDLE_CONTRACT.md`'s bundle-identity/currency semantics.
const BUNDLE_CAPTURED_AT_UNIX_SECONDS: i64 = 1_789_948_800; // 2026-09-21T00:00:00Z

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).expect("fixture ids are non-zero"))
}

fn text(value: &str) -> NonEmptyText {
    NonEmptyText::try_new(value.to_string()).expect("fixture text is non-empty")
}

/// A fully assembled, real bundle plus everything needed to run
/// `nexo_core::evaluator::evaluate` against it: the route, the normative
/// context, and the two evidence-window ids the evaluator threads through.
pub struct Fixture {
    pub bundle: PolicyBundle,
    pub context: NormativeContext,
    pub route: ActionRoute,
    pub claim_access: NormativeClaimId,
    pub claim_rectification: NormativeClaimId,
    pub identity_requirement: RequirementId,
    pub jurisdiction_evidence: JurisdictionEvidenceId,
    pub freshness_evidence: PolicyFreshnessEvidenceId,
    /// `NormativeClaimId` derives `Eq`/`PartialEq` but not `Hash` in
    /// `nexo-core`, so citations are a small linear-scan list rather than a
    /// map — fine at this crate's scale (two claims today).
    pub citations: Vec<(NormativeClaimId, ClaimCitation)>,
}

impl Fixture {
    pub fn citation_for(&self, claim: NormativeClaimId) -> Option<&ClaimCitation> {
        self.citations
            .iter()
            .find(|(id, _)| *id == claim)
            .map(|(_, citation)| citation)
    }
}

/// Builds the real bundle: attests the actual captured InfoLEG bytes
/// through the same bridge (`nexo_policy_bridge::attest_capture`) any
/// adapter uses, then assembles the two claims (arts. 14 and 16) and the
/// "request access, rectification, or suppression of personal data" route
/// that requires both, plus one mandatory requirement mirroring art. 14.1's
/// "previa acreditación de su identidad" (identity must be proven before
/// access is granted).
pub fn build() -> Fixture {
    // The pinned literal, not a hash of CAPTURED_SOURCE_BYTES itself, is
    // the expected digest: attest_capture must compare the file against an
    // independent, previously-recorded value, or a tampered file would
    // simply attest itself.
    let expected_digest = nexo_integrity::Sha256Digest::from_hex(CAPTURED_SOURCE_SHA256_HEX)
        .expect("CAPTURED_SOURCE_SHA256_HEX is a valid 64-character hex digest");
    let capture = nexo_policy_bridge::attest_capture(
        CAPTURED_SOURCE_BYTES,
        expected_digest,
        ArtifactId::new(node(1)),
        DigestId::new(node(2)),
        ProvenanceId::new(node(3)),
        UtcInstant::from_unix_seconds(BUNDLE_CAPTURED_AT_UNIX_SECONDS),
    )
    .expect("the committed source file must match its own digest");

    let source = NormativeSource::new(
        NormativeSourceId::new(node(4)),
        AuthorityKind::PrimaryOfficial,
        text(CAPTURED_SOURCE_ISSUER),
        text(CAPTURED_SOURCE_LOCATOR),
        AcquisitionChannel::WebFetch,
        UtcInstant::from_unix_seconds(BUNDLE_CAPTURED_AT_UNIX_SECONDS),
        ArtifactId::new(node(1)),
        DigestId::new(node(2)),
        ProvenanceId::new(node(3)),
    );

    let bundle_id = PolicyBundleId::new(node(5));
    let law_promulgated = CivilDate::try_new(2000, 10, 30).expect("valid calendar date");

    let claim_access = NormativeClaim::try_new(
        NormativeClaimId::new(node(6)),
        text(CLAIM_ACCESS_PROPOSITION),
        text(JurisdictionCode::Argentina.code()),
        ValidityInterval::try_new(law_promulgated, None).expect("open-ended validity"),
        bundle_id,
        vec![ClaimSourceSupport::new(source.id(), SupportRole::Primary)],
    )
    .expect("claim satisfies nexo-core's construction invariants");

    let claim_rectification = NormativeClaim::try_new(
        NormativeClaimId::new(node(7)),
        text(CLAIM_RECTIFICATION_PROPOSITION),
        text(JurisdictionCode::Argentina.code()),
        ValidityInterval::try_new(law_promulgated, None).expect("open-ended validity"),
        bundle_id,
        vec![ClaimSourceSupport::new(source.id(), SupportRole::Primary)],
    )
    .expect("claim satisfies nexo-core's construction invariants");

    let bundle = PolicyBundle::try_new(
        bundle_id,
        PolicySchemaVersion::CURRENT,
        PolicyVersion::try_new("ar-ley25326-2026.1".into()).expect("non-empty version"),
        JurisdictionCode::Argentina,
        ValidityInterval::try_new(
            CivilDate::try_new(2026, 9, 21).expect("valid calendar date"),
            None,
        )
        .expect("open-ended validity"),
        vec![claim_access.id(), claim_rectification.id()],
        SourcePolicy::try_new(
            vec![AuthorityKind::PrimaryOfficial],
            vec![AcquisitionChannel::WebFetch, AcquisitionChannel::OfficialApi],
        )
        .expect("non-empty source policy"),
        capture,
    )
    .expect("bundle satisfies nexo-core's construction invariants");

    let context = NormativeContext::try_new(
        vec![claim_access.clone(), claim_rectification.clone()],
        vec![source],
    )
    .expect("context satisfies nexo-core's construction invariants");

    let identity_requirement = RequirementId::new(node(8));
    let route = ActionRoute::try_new(
        node(9),
        JurisdictionCode::Argentina,
        vec![claim_access.id(), claim_rectification.id()],
        vec![identity_requirement],
    )
    .expect("route satisfies nexo-core's construction invariants");

    let citations = vec![
        (
            claim_access.id(),
            ClaimCitation {
                proposition: CLAIM_ACCESS_PROPOSITION,
                source_issuer: CAPTURED_SOURCE_ISSUER,
                source_locator: CAPTURED_SOURCE_LOCATOR,
            },
        ),
        (
            claim_rectification.id(),
            ClaimCitation {
                proposition: CLAIM_RECTIFICATION_PROPOSITION,
                source_issuer: CAPTURED_SOURCE_ISSUER,
                source_locator: CAPTURED_SOURCE_LOCATOR,
            },
        ),
    ];

    Fixture {
        bundle,
        context,
        route,
        claim_access: claim_access.id(),
        claim_rectification: claim_rectification.id(),
        identity_requirement,
        jurisdiction_evidence: JurisdictionEvidenceId::new(node(10)),
        freshness_evidence: PolicyFreshnessEvidenceId::new(node(11)),
        citations,
    }
}

/// A minimal, valid `CaseProjection` for a case that has proven identity
/// (the only mandatory requirement on this route): one factual support
/// item and the identity requirement marked satisfied.
pub fn satisfied_projection(fixture: &Fixture) -> CaseProjection {
    CaseProjection::try_new(
        vec![FactualSupport::UserAssertion(node(100))],
        vec![fixture.identity_requirement],
        fixture.jurisdiction_evidence,
        fixture.freshness_evidence,
    )
    .expect("projection satisfies nexo-core's construction invariants")
}

/// The same case, but identity has not yet been proven: this is the
/// projection that must drive the evaluator to `InsufficientFacts`, not to
/// `Actionable` with a missing check silently skipped.
pub fn unsatisfied_projection(fixture: &Fixture) -> CaseProjection {
    CaseProjection::try_new(
        vec![FactualSupport::UserAssertion(node(100))],
        Vec::new(),
        fixture.jurisdiction_evidence,
        fixture.freshness_evidence,
    )
    .expect("projection satisfies nexo-core's construction invariants")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexo_core::{evaluate, ActionEvaluation, ActionStatus, NonActionable};

    #[test]
    fn a_tampered_copy_of_the_captured_source_fails_attestation() {
        // Regression test for RT-001-02: the pinned CAPTURED_SOURCE_SHA256_HEX,
        // not a hash of the (possibly tampered) bytes themselves, must be
        // what attest_capture compares against. This proves that wiring:
        // mutate one byte of a copy of the real captured bytes and confirm
        // the pinned digest rejects it, the same way it would reject a
        // genuinely tampered commit of sources/ley_25326_texact.html.
        let mut tampered = CAPTURED_SOURCE_BYTES.to_vec();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        let expected_digest =
            nexo_integrity::Sha256Digest::from_hex(CAPTURED_SOURCE_SHA256_HEX).unwrap();
        let result = nexo_policy_bridge::attest_capture(
            &tampered,
            expected_digest,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(BUNDLE_CAPTURED_AT_UNIX_SECONDS),
        );
        assert!(result.is_err(), "a tampered source must fail attestation");
    }

    #[test]
    fn captured_source_is_the_real_infoleg_text() {
        let text = String::from_utf8_lossy(CAPTURED_SOURCE_BYTES);
        assert!(text.contains("25.326"));
        assert!(text.contains("ARTICULO 14"));
        assert!(text.contains("previa acreditaci"));
        assert!(text.contains("ARTICULO 16"));
    }

    #[test]
    fn citations_cover_exactly_every_claim_the_route_requires() {
        let fixture = build();
        assert_eq!(fixture.citations.len(), 2);
        for claim in [fixture.claim_access, fixture.claim_rectification] {
            let citation = fixture
                .citation_for(claim)
                .expect("every route claim must have a citation");
            assert!(!citation.proposition.is_empty());
            assert_eq!(citation.source_locator, CAPTURED_SOURCE_LOCATOR);
        }
    }

    #[test]
    fn actionable_when_jurisdiction_date_and_identity_all_line_up() {
        let fixture = build();
        let projection = satisfied_projection(&fixture);
        let reference_date = CivilDate::try_new(2026, 9, 21).unwrap();
        let result = evaluate(
            &projection,
            &fixture.bundle,
            &fixture.context,
            &fixture.route,
            reference_date,
        );
        match result {
            ActionEvaluation::Actionable(option) => {
                assert_eq!(option.status(), ActionStatus::Supported);
            }
            other => panic!("expected Actionable, got {other:?}"),
        }
    }

    #[test]
    fn insufficient_facts_when_identity_not_yet_proven() {
        let fixture = build();
        let projection = unsatisfied_projection(&fixture);
        let reference_date = CivilDate::try_new(2026, 9, 21).unwrap();
        let result = evaluate(
            &projection,
            &fixture.bundle,
            &fixture.context,
            &fixture.route,
            reference_date,
        );
        assert!(matches!(
            result,
            ActionEvaluation::NonActionable(NonActionable::InsufficientFacts(_))
        ));
    }

    #[test]
    fn out_of_jurisdiction_when_route_targets_a_different_jurisdiction_than_the_bundle() {
        let fixture = build();
        let projection = satisfied_projection(&fixture);
        let us_route = ActionRoute::try_new(
            node(9),
            JurisdictionCode::UnitedStates,
            vec![fixture.claim_access, fixture.claim_rectification],
            vec![fixture.identity_requirement],
        )
        .unwrap();
        let reference_date = CivilDate::try_new(2026, 9, 21).unwrap();
        let result = evaluate(
            &projection,
            &fixture.bundle,
            &fixture.context,
            &us_route,
            reference_date,
        );
        assert!(matches!(
            result,
            ActionEvaluation::NonActionable(NonActionable::OutOfJurisdiction(_))
        ));
    }

    #[test]
    fn policy_not_current_when_reference_date_precedes_bundle_activation() {
        let fixture = build();
        let projection = satisfied_projection(&fixture);
        // The bundle's own currency window starts 2026-09-21 (when this
        // bundle was captured/activated); the underlying statute has been
        // in force since 2000, but evaluating against an earlier reference
        // date means "no activated bundle version covered that date" —
        // exactly what POLICY_NOT_CURRENT records.
        let reference_date = CivilDate::try_new(2020, 1, 1).unwrap();
        let result = evaluate(
            &projection,
            &fixture.bundle,
            &fixture.context,
            &fixture.route,
            reference_date,
        );
        assert!(matches!(
            result,
            ActionEvaluation::NonActionable(NonActionable::PolicyNotCurrent(_))
        ));
    }

    #[test]
    fn abstains_when_the_only_eligible_source_authority_is_excluded_by_the_bundle() {
        let fixture = build();
        // Rebuild a bundle whose SourcePolicy does not accept the claim's
        // own source authority/channel: the claim becomes ineligible even
        // though it is still listed in bundle.claims(), which is exactly
        // the "no authoritative legal source" abstention path, not a
        // route/bundle mismatch.
        let expected_digest = nexo_integrity::Sha256Digest::from_hex(CAPTURED_SOURCE_SHA256_HEX)
            .unwrap();
        let capture = nexo_policy_bridge::attest_capture(
            CAPTURED_SOURCE_BYTES,
            expected_digest,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(BUNDLE_CAPTURED_AT_UNIX_SECONDS),
        )
        .unwrap();
        let narrow_bundle = PolicyBundle::try_new(
            fixture.bundle.id(),
            PolicySchemaVersion::CURRENT,
            PolicyVersion::try_new("ar-ley25326-2026.1-narrow".into()).unwrap(),
            JurisdictionCode::Argentina,
            fixture.bundle.validity(),
            fixture.bundle.claims().to_vec(),
            // This bundle only accepts OfficialApi-channel sources; the
            // fixture's source was acquired via WebFetch, so it becomes
            // ineligible under this (still internally valid) bundle.
            SourcePolicy::try_new(
                vec![AuthorityKind::PrimaryOfficial],
                vec![AcquisitionChannel::OfficialApi],
            )
            .unwrap(),
            capture,
        )
        .unwrap();

        let projection = satisfied_projection(&fixture);
        let reference_date = CivilDate::try_new(2026, 9, 21).unwrap();
        let result = evaluate(
            &projection,
            &narrow_bundle,
            &fixture.context,
            &fixture.route,
            reference_date,
        );
        assert!(matches!(
            result,
            ActionEvaluation::NonActionable(NonActionable::Abstain(_))
        ));
    }
}
