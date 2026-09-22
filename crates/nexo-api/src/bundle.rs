//! A policy-bundle-agnostic handle this crate evaluates against.
//!
//! `nexo-policy-ar` and `nexo-policy-ar-digital-violence` each build a real
//! `nexo_core::PolicyBundle`/`NormativeContext`/`ActionRoute` plus their own
//! citation text, but as two distinct `Fixture` types (one per crate) with
//! two distinct, bundle-specific rules for when their one mandatory
//! requirement counts as satisfied. This module is the one place that
//! reduces both to a common shape the rest of this crate (`projection.rs`,
//! `explain.rs`, `handlers.rs`) can evaluate against without caring which
//! bundle it is.

use nexo_core::{
    ActionRoute, JurisdictionEvidenceId, NormativeClaimId, NormativeContext, PolicyBundle,
    PolicyFreshnessEvidenceId, RequirementId,
};

use nexo_app::repository::CaseNodeSummary;

/// See `nexo_policy_ar::ClaimCitation`'s doc comment for why this exists:
/// `nexo_core::NormativeClaim` deliberately exposes no proposition-text
/// getter. Each policy crate defines its own identically shaped type (so it
/// stays the source of truth for the text it wrote); this is the one place
/// that converts either of them into a bundle-agnostic shape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimCitation {
    pub proposition: &'static str,
    pub source_issuer: &'static str,
    pub source_locator: &'static str,
}

/// How this bundle's one mandatory requirement gets marked satisfied from
/// durable case state. Each variant is a real, bundle-specific rule (see
/// each policy crate's own contract doc for why), not a generic "any
/// evidence counts" fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequirementSignal {
    /// `nexo-policy-ar` (Ley 25.326): satisfied by at least one confirmed
    /// user assertion — see docs/API_CONTRACT.md, "Known simplifications."
    ConfirmedUserAssertion,
    /// `nexo-policy-ar-digital-violence` (Ley 27.736): satisfied by at
    /// least one artifact node — the specific content/URL must already be
    /// identified in the case, mirroring art. 12's own precondition.
    AnyArtifactPresent,
}

impl RequirementSignal {
    pub fn is_satisfied(self, nodes: &[CaseNodeSummary]) -> bool {
        match self {
            Self::ConfirmedUserAssertion => nodes
                .iter()
                .any(|node| node.kind == "user_assertion" && node.confirmed == Some(true)),
            Self::AnyArtifactPresent => nodes.iter().any(|node| node.kind == "artifact"),
        }
    }
}

pub struct PolicyBundleHandle {
    /// Stable key used in the API (`?bundle=<key>`) and as the seeding
    /// idempotency identity. Never derived from anything user-supplied.
    pub key: &'static str,
    pub display_name: &'static str,
    pub bundle: PolicyBundle,
    pub context: NormativeContext,
    pub route: ActionRoute,
    pub mandatory_requirement: RequirementId,
    pub requirement_signal: RequirementSignal,
    pub jurisdiction_evidence: JurisdictionEvidenceId,
    pub freshness_evidence: PolicyFreshnessEvidenceId,
    pub citations: Vec<(NormativeClaimId, ClaimCitation)>,
}

impl PolicyBundleHandle {
    pub fn citation_for(&self, claim: NormativeClaimId) -> Option<&ClaimCitation> {
        self.citations
            .iter()
            .find(|(id, _)| *id == claim)
            .map(|(_, citation)| citation)
    }

    pub fn from_ley_25326(fixture: &nexo_policy_ar::Fixture) -> Self {
        Self {
            key: "ley-25326",
            display_name: "Ley 25.326 — acceso, rectificación y supresión de datos personales",
            bundle: fixture.bundle.clone(),
            context: fixture.context.clone(),
            route: fixture.route.clone(),
            mandatory_requirement: fixture.identity_requirement,
            requirement_signal: RequirementSignal::ConfirmedUserAssertion,
            jurisdiction_evidence: fixture.jurisdiction_evidence,
            freshness_evidence: fixture.freshness_evidence,
            citations: fixture
                .citations
                .iter()
                .map(|(id, citation)| {
                    (
                        *id,
                        ClaimCitation {
                            proposition: citation.proposition,
                            source_issuer: citation.source_issuer,
                            source_locator: citation.source_locator,
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn from_ley_27736(fixture: &nexo_policy_ar_digital_violence::Fixture) -> Self {
        Self {
            key: "ley-27736",
            display_name: "Ley 27.736 (Ley Olimpia) — violencia digital, remoción de contenido",
            bundle: fixture.bundle.clone(),
            context: fixture.context.clone(),
            route: fixture.route.clone(),
            mandatory_requirement: fixture.content_identified_requirement,
            requirement_signal: RequirementSignal::AnyArtifactPresent,
            jurisdiction_evidence: fixture.jurisdiction_evidence,
            freshness_evidence: fixture.freshness_evidence,
            citations: fixture
                .citations
                .iter()
                .map(|(id, citation)| {
                    (
                        *id,
                        ClaimCitation {
                            proposition: citation.proposition,
                            source_issuer: citation.source_issuer,
                            source_locator: citation.source_locator,
                        },
                    )
                })
                .collect(),
        }
    }
}
