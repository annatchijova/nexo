//! Deterministic citation rendering: turns an already-computed
//! `nexo_core::ActionEvaluation` into JSON, per `docs/ARCHITECTURE.md`:
//! "Explanation is a deterministic citation rendering computed by the API
//! from an authorized graph projection." Nothing here decides anything —
//! `nexo_core::evaluate` already decided; this module only names, in a
//! shape a client can render, exactly what that decision already was.
//!
//! `nexo_core::NodeId` is deliberately opaque (no public accessor to its
//! inner integer) — this module respects that rather than working around
//! it: every `FactualSupport` this API ever constructs is recorded in a
//! `NodeIdResolver` (built from data this API already owns) before
//! `evaluate` is called, and resolved back to a real `case_node_id` by
//! equality (`NodeId` is `Eq + Hash + Copy`) afterward, never by reading a
//! value out of the id.

use std::collections::HashMap;

use nexo_core::{
    Abstention, ActionEvaluation, ActionStatus, Contraindicated, FactualSupport,
    InsufficientFacts, NodeId, NonActionable, NormativeClaimId, OutOfJurisdiction,
    PolicyNotCurrent,
};
use nexo_policy_ar::Fixture;
use serde_json::{json, Value};

/// Maps every `NodeId` this API constructed for one evaluation back to the
/// `case_node_id` it was built from. Never claims to resolve a `NodeId`
/// this API did not itself mint for the current case.
#[derive(Default)]
pub struct NodeIdResolver {
    known: HashMap<NodeId, i64>,
}

impl NodeIdResolver {
    pub fn record(&mut self, id: NodeId, case_node_id: i64) {
        self.known.insert(id, case_node_id);
    }

    fn resolve(&self, id: NodeId) -> Value {
        match self.known.get(&id) {
            Some(case_node_id) => json!(case_node_id),
            // A NodeId this resolver never recorded — a real gap
            // (evaluator constructed or forwarded an id this API did not
            // mint) that must be visible in the response, not hidden
            // behind a fabricated number.
            None => json!(null),
        }
    }

    fn resolve_factual(&self, support: &FactualSupport) -> Value {
        let (kind, id) = match support {
            FactualSupport::Artifact(id) => ("artifact", *id),
            FactualSupport::Observation(id) => ("observation", *id),
            FactualSupport::UserAssertion(id) => ("user_assertion", *id),
            FactualSupport::DerivedFact(id) => ("derived_fact", *id),
        };
        json!({"kind": kind, "case_node_id": self.resolve(id)})
    }
}

fn render_citation(fixture: &Fixture, claim: NormativeClaimId) -> Value {
    match fixture.citation_for(claim) {
        Some(citation) => json!({
            "proposition": citation.proposition,
            "source_issuer": citation.source_issuer,
            "source_locator": citation.source_locator,
        }),
        // Every claim this API's bundle can cite has a citation recorded
        // by construction (see nexo-policy-ar's own
        // citations_cover_exactly_every_claim_the_route_requires test);
        // null here would mean the evaluator returned a claim id from a
        // bundle other than the one this response is citing — surfaced,
        // not hidden.
        None => Value::Null,
    }
}

pub fn render(evaluation: &ActionEvaluation, resolver: &NodeIdResolver, fixture: &Fixture) -> Value {
    match evaluation {
        ActionEvaluation::Actionable(option) => json!({
            "kind": "actionable",
            "status": match option.status() {
                ActionStatus::Supported => "supported",
                ActionStatus::ConditionallySupported => "conditionally_supported",
            },
            "available": option.is_available(),
            "factual_support": option
                .factual_support()
                .iter()
                .map(|f| resolver.resolve_factual(f))
                .collect::<Vec<_>>(),
            "legal_support": option
                .legal_support()
                .iter()
                .map(|c| render_citation(fixture, *c))
                .collect::<Vec<_>>(),
            "unmet_requirements": option.unmet_requirements().len(),
        }),
        ActionEvaluation::NonActionable(non_actionable) => match non_actionable {
            NonActionable::InsufficientFacts(insufficient) => {
                render_insufficient_facts(insufficient, fixture)
            }
            NonActionable::Contraindicated(contraindicated) => {
                render_contraindicated(contraindicated, resolver, fixture)
            }
            NonActionable::OutOfJurisdiction(out) => render_out_of_jurisdiction(out),
            NonActionable::PolicyNotCurrent(stale) => render_policy_not_current(stale),
            NonActionable::Abstain(abstention) => render_abstention(abstention, resolver),
        },
    }
}

fn render_insufficient_facts(insufficient: &InsufficientFacts, fixture: &Fixture) -> Value {
    json!({
        "kind": "non_actionable",
        "variant": "insufficient_facts",
        "legal_support": insufficient
            .legal_support()
            .iter()
            .map(|c| render_citation(fixture, *c))
            .collect::<Vec<_>>(),
        "missing_requirement_count": insufficient.missing_requirements().len(),
    })
}

fn render_contraindicated(
    contraindicated: &Contraindicated,
    resolver: &NodeIdResolver,
    fixture: &Fixture,
) -> Value {
    json!({
        "kind": "non_actionable",
        "variant": "contraindicated",
        "factual_support": contraindicated
            .factual_support()
            .iter()
            .map(|f| resolver.resolve_factual(f))
            .collect::<Vec<_>>(),
        "legal_support": contraindicated
            .legal_support()
            .iter()
            .map(|c| render_citation(fixture, *c))
            .collect::<Vec<_>>(),
        "evidence_count": contraindicated.evidence().len(),
    })
}

fn render_out_of_jurisdiction(out: &OutOfJurisdiction) -> Value {
    let _ = out.jurisdiction_evidence();
    let _ = out.policy_bundle();
    json!({
        "kind": "non_actionable",
        "variant": "out_of_jurisdiction",
    })
}

fn render_policy_not_current(stale: &PolicyNotCurrent) -> Value {
    let _ = stale.policy_bundle();
    let _ = stale.freshness_evidence();
    json!({
        "kind": "non_actionable",
        "variant": "policy_not_current",
    })
}

fn render_abstention(abstention: &Abstention, resolver: &NodeIdResolver) -> Value {
    match abstention {
        Abstention::NoAuthoritativeLegalSource(inner) => json!({
            "kind": "non_actionable",
            "variant": "abstain",
            "cause": "no_authoritative_legal_source",
            "factual_context": inner
                .factual_context()
                .iter()
                .map(|f| resolver.resolve_factual(f))
                .collect::<Vec<_>>(),
        }),
        Abstention::UnsupportedQuestion(inner) => json!({
            "kind": "non_actionable",
            "variant": "abstain",
            "cause": "unsupported_question",
            "factual_context": inner
                .factual_context()
                .iter()
                .map(|f| resolver.resolve_factual(f))
                .collect::<Vec<_>>(),
        }),
        Abstention::ConflictingLegalClaims(inner) => json!({
            "kind": "non_actionable",
            "variant": "abstain",
            "cause": "conflicting_legal_claims",
            "factual_context": inner
                .factual_context()
                .iter()
                .map(|f| resolver.resolve_factual(f))
                .collect::<Vec<_>>(),
            "conflicting_claim_count": inner.conflicting_claims().len(),
            "witness_count": inner.witnesses().len(),
        }),
    }
}
