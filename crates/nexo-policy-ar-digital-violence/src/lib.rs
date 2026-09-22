//! Argentina policy bundle for Ley 27.736 ("Ley Olimpia"), which amends
//! Ley 26.485 (Protección Integral para Prevenir, Sancionar y Erradicar la
//! Violencia contra las Mujeres) to define digital violence and create a
//! judicial removal-order route for it.
//!
//! **This bundle intentionally covers what the maintainer named as two
//! separate situations** (non-consensual intimate content, and digital
//! gender-based harassment/violence) **because the actual statute does
//! not separate them.** Ley 27.736's own definition of "violencia digital"
//! (incorporated as inciso i) of art. 6° of Ley 26.485) is one single
//! definition that names non-consensual intimate-content distribution as
//! one of several forms digital violence can take — obtaining,
//! reproducing, or distributing, without consent, intimate or nude digital
//! material attributed to a woman is explicitly one clause inside the same
//! sentence that also covers stalking, threats, doxxing, and misogynistic
//! harassment. Splitting this into two bundles would have meant inventing
//! a legal boundary the statute itself does not draw — the "boring"
//! explanation (one statute, one route) survived checking the actual text
//! over the assumed one (two statutes, two routes).
//!
//! Contract: `docs/POLICY_BUNDLE_AR_DIGITAL_VIOLENCE_CONTRACT.md`. The
//! captured source in `sources/ley_27736_norma.html` is the real bytes
//! fetched from InfoLEG — not a paraphrase — following the same pattern as
//! `nexo-policy-ar`.

use core::num::NonZeroU64;

use nexo_core::{
    AcquisitionChannel, ActionRoute, ArtifactId, AuthorityKind, CaseProjection, CivilDate,
    ClaimSourceSupport, DigestId, FactualSupport, JurisdictionCode, JurisdictionEvidenceId,
    NodeId, NonEmptyText, NormativeClaim, NormativeClaimId, NormativeContext, NormativeSource,
    NormativeSourceId, PolicyBundle, PolicyBundleId, PolicyFreshnessEvidenceId,
    PolicySchemaVersion, PolicyVersion, ProvenanceId, RequirementId, SourcePolicy, SupportRole,
    UtcInstant, ValidityInterval,
};

pub const CAPTURED_SOURCE_BYTES: &[u8] = include_bytes!("../sources/ley_27736_norma.html");
pub const CAPTURED_SOURCE_LOCATOR: &str =
    "http://servicios.infoleg.gob.ar/infolegInternet/anexos/390000-394999/391774/norma.htm";
pub const CAPTURED_SOURCE_ISSUER: &str = "InfoLEG - Ministerio de Justicia (Boletín Oficial)";

const CLAIM_DEFINITION_PROPOSITION: &str =
    "Art. 4°, Ley 27.736 (incorpora inciso i) al art. 6°, Ley 26.485): define \
     violencia digital o telemática como toda conducta contra las mujeres \
     basada en su género cometida mediante tecnologías de la información, \
     incluyendo expresamente la obtención, reproducción y difusión sin \
     consentimiento de material digital íntimo o de desnudez atribuido a \
     una mujer, así como acoso, amenaza, extorsión, control o espionaje de \
     su actividad virtual.";
const CLAIM_REMOVAL_ORDER_PROPOSITION: &str =
    "Art. 12°, Ley 27.736 (incorpora apartado a.9 al art. 26°, Ley 26.485): \
     la autoridad judicial puede ordenar, por auto fundado, a plataformas \
     digitales, redes sociales o páginas electrónicas la supresión de \
     contenidos que constituyan violencia digital, identificando la URL \
     específica del contenido cuya remoción se ordena.";

/// See `nexo_policy_ar::ClaimCitation` for why this type exists: `nexo_core`
/// deliberately exposes no proposition-text getter, so the crate that wrote
/// the text is the one place that can answer "why is this shown to me?"
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimCitation {
    pub proposition: &'static str,
    pub source_issuer: &'static str,
    pub source_locator: &'static str,
}

/// The digest of `sources/ley_27736_norma.html`, recorded once as a
/// literal and never recomputed from that same file to check itself — see
/// `nexo_policy_ar::CAPTURED_SOURCE_SHA256_HEX`'s doc comment
/// (RT-001-02 in docs/RED_TEAM_ROUND_001.md) for why a self-referential
/// digest would be worthless. Recorded in
/// docs/POLICY_BUNDLE_AR_DIGITAL_VIOLENCE_CONTRACT.md as the source
/// capture's citable digest.
pub const CAPTURED_SOURCE_SHA256_HEX: &str =
    "9bc6ccab30298a534900e083719c2f92f6ff51204e1adb7ccdd08a09b3219772";

/// This bundle's own version/currency window start — distinct from the
/// statute's own sanction date (2023-10-10) and publication date
/// (2023-10-23), which the claim validity below uses instead.
const BUNDLE_CAPTURED_AT_UNIX_SECONDS: i64 = 1_790_035_200; // 2026-09-22T00:00:00Z

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).expect("fixture ids are non-zero"))
}

fn text(value: &str) -> NonEmptyText {
    NonEmptyText::try_new(value.to_string()).expect("fixture text is non-empty")
}

pub struct Fixture {
    pub bundle: PolicyBundle,
    pub context: NormativeContext,
    pub route: ActionRoute,
    pub claim_definition: NormativeClaimId,
    pub claim_removal_order: NormativeClaimId,
    /// The one mandatory requirement on this route: the specific content
    /// or URL that violence is alleged against must be identified in the
    /// case, mirroring art. 12's own requirement that a removal order
    /// "identificar[a] la URL específica del contenido cuya remoción se
    /// ordena" — a court cannot order removal of unspecified content.
    pub content_identified_requirement: RequirementId,
    pub jurisdiction_evidence: JurisdictionEvidenceId,
    pub freshness_evidence: PolicyFreshnessEvidenceId,
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

/// Builds the real bundle: attests the actual captured InfoLEG bytes,
/// then assembles the two claims (the digital-violence definition and the
/// judicial removal-order power) and the single route that requires both.
pub fn build() -> Fixture {
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
    // Law sanctioned 2023-10-10, published (and effective) 2023-10-23.
    let law_effective = CivilDate::try_new(2023, 10, 23).expect("valid calendar date");

    let claim_definition = NormativeClaim::try_new(
        NormativeClaimId::new(node(6)),
        text(CLAIM_DEFINITION_PROPOSITION),
        text(JurisdictionCode::Argentina.code()),
        ValidityInterval::try_new(law_effective, None).expect("open-ended validity"),
        bundle_id,
        vec![ClaimSourceSupport::new(source.id(), SupportRole::Primary)],
    )
    .expect("claim satisfies nexo-core's construction invariants");

    let claim_removal_order = NormativeClaim::try_new(
        NormativeClaimId::new(node(7)),
        text(CLAIM_REMOVAL_ORDER_PROPOSITION),
        text(JurisdictionCode::Argentina.code()),
        ValidityInterval::try_new(law_effective, None).expect("open-ended validity"),
        bundle_id,
        vec![ClaimSourceSupport::new(source.id(), SupportRole::Primary)],
    )
    .expect("claim satisfies nexo-core's construction invariants");

    let bundle = PolicyBundle::try_new(
        bundle_id,
        PolicySchemaVersion::CURRENT,
        PolicyVersion::try_new("ar-ley27736-2026.1".into()).expect("non-empty version"),
        JurisdictionCode::Argentina,
        ValidityInterval::try_new(
            CivilDate::try_new(2026, 9, 22).expect("valid calendar date"),
            None,
        )
        .expect("open-ended validity"),
        vec![claim_definition.id(), claim_removal_order.id()],
        SourcePolicy::try_new(
            vec![AuthorityKind::PrimaryOfficial],
            vec![AcquisitionChannel::WebFetch, AcquisitionChannel::OfficialApi],
        )
        .expect("non-empty source policy"),
        capture,
    )
    .expect("bundle satisfies nexo-core's construction invariants");

    let context = NormativeContext::try_new(
        vec![claim_definition.clone(), claim_removal_order.clone()],
        vec![source],
    )
    .expect("context satisfies nexo-core's construction invariants");

    let content_identified_requirement = RequirementId::new(node(8));
    let route = ActionRoute::try_new(
        node(9),
        JurisdictionCode::Argentina,
        vec![claim_definition.id(), claim_removal_order.id()],
        vec![content_identified_requirement],
    )
    .expect("route satisfies nexo-core's construction invariants");

    let citations = vec![
        (
            claim_definition.id(),
            ClaimCitation {
                proposition: CLAIM_DEFINITION_PROPOSITION,
                source_issuer: CAPTURED_SOURCE_ISSUER,
                source_locator: CAPTURED_SOURCE_LOCATOR,
            },
        ),
        (
            claim_removal_order.id(),
            ClaimCitation {
                proposition: CLAIM_REMOVAL_ORDER_PROPOSITION,
                source_issuer: CAPTURED_SOURCE_ISSUER,
                source_locator: CAPTURED_SOURCE_LOCATOR,
            },
        ),
    ];

    Fixture {
        bundle,
        context,
        route,
        claim_definition: claim_definition.id(),
        claim_removal_order: claim_removal_order.id(),
        content_identified_requirement,
        jurisdiction_evidence: JurisdictionEvidenceId::new(node(10)),
        freshness_evidence: PolicyFreshnessEvidenceId::new(node(11)),
        citations,
    }
}

/// A case where the specific content/URL has been identified (an
/// artifact in the case graph, e.g. a screenshot or captured page).
pub fn satisfied_projection(fixture: &Fixture) -> CaseProjection {
    CaseProjection::try_new(
        vec![FactualSupport::Artifact(node(100))],
        vec![fixture.content_identified_requirement],
        fixture.jurisdiction_evidence,
        fixture.freshness_evidence,
    )
    .expect("projection satisfies nexo-core's construction invariants")
}

/// The same case, but no specific content has been identified yet — a
/// court cannot order removal of unspecified content.
pub fn unsatisfied_projection(fixture: &Fixture) -> CaseProjection {
    CaseProjection::try_new(
        vec![FactualSupport::Artifact(node(100))],
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
    fn captured_source_is_the_real_infoleg_text() {
        let text = String::from_utf8_lossy(CAPTURED_SOURCE_BYTES);
        assert!(text.contains("27736"));
        assert!(text.contains("LEY OLIMPIA"));
        assert!(text.contains("Violencia digital"));
        // The captured page is Latin-1 (InfoLEG's original encoding, like
        // nexo-policy-ar's Ley 25.326 source); accented characters survive
        // as their Latin-1 byte through from_utf8_lossy only as a
        // replacement character, not the original letter — so, exactly
        // like nexo-policy-ar's own test, these assertions cut the
        // substring just before each accented character rather than
        // asserting on mangled bytes.
        assert!(text.contains("reproducci"));
        assert!(text.contains("y difusi"));
        assert!(text.contains("sin consentimiento de material digital real o"));
        assert!(text.contains("intimo o de desnudez"));
        assert!(text.contains("URL espec"));
        assert!(text.contains("del contenido cuya remoci"));
    }

    #[test]
    fn a_tampered_copy_of_the_captured_source_fails_attestation() {
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
    fn citations_cover_exactly_every_claim_the_route_requires() {
        let fixture = build();
        assert_eq!(fixture.citations.len(), 2);
        for claim in [fixture.claim_definition, fixture.claim_removal_order] {
            let citation = fixture
                .citation_for(claim)
                .expect("every route claim must have a citation");
            assert!(!citation.proposition.is_empty());
            assert_eq!(citation.source_locator, CAPTURED_SOURCE_LOCATOR);
        }
    }

    #[test]
    fn actionable_when_content_is_identified() {
        let fixture = build();
        let projection = satisfied_projection(&fixture);
        let reference_date = CivilDate::try_new(2026, 9, 22).unwrap();
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
    fn insufficient_facts_when_no_content_identified_yet() {
        let fixture = build();
        let projection = unsatisfied_projection(&fixture);
        let reference_date = CivilDate::try_new(2026, 9, 22).unwrap();
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
            vec![fixture.claim_definition, fixture.claim_removal_order],
            vec![fixture.content_identified_requirement],
        )
        .unwrap();
        let reference_date = CivilDate::try_new(2026, 9, 22).unwrap();
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
        // The statute has been in force since 2023, but this bundle
        // version's own currency window starts 2026-09-22.
        let reference_date = CivilDate::try_new(2024, 1, 1).unwrap();
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
        let expected_digest =
            nexo_integrity::Sha256Digest::from_hex(CAPTURED_SOURCE_SHA256_HEX).unwrap();
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
            PolicyVersion::try_new("ar-ley27736-2026.1-narrow".into()).unwrap(),
            JurisdictionCode::Argentina,
            fixture.bundle.validity(),
            fixture.bundle.claims().to_vec(),
            SourcePolicy::try_new(
                vec![AuthorityKind::PrimaryOfficial],
                vec![AcquisitionChannel::OfficialApi],
            )
            .unwrap(),
            capture,
        )
        .unwrap();

        let projection = satisfied_projection(&fixture);
        let reference_date = CivilDate::try_new(2026, 9, 22).unwrap();
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
