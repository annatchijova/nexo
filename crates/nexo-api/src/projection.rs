//! Builds a `nexo_core::CaseProjection` from durable case-graph state, and
//! records the `NodeId <-> case_node_id` mapping needed to explain the
//! result afterward (see `explain.rs`).

use core::num::NonZeroU64;
use std::collections::BTreeMap;

use nexo_core::{CaseProjection, FactualSupport, NodeId, RequirementId};
use nexo_app::repository::{self, CaseRowId, Pool};
use nexo_integrity::{seal, CanonicalValue};

use crate::explain::NodeIdResolver;

#[derive(Debug)]
pub enum ProjectionError {
    Repo(repository::RepoError),
    /// The case has no factual-support-eligible node yet (no artifact,
    /// observation, or user assertion). `CaseProjection::try_new` requires
    /// at least one; a case with nothing in it cannot be evaluated.
    NoFactualSupportYet,
    Core(nexo_core::ProjectionError),
}

pub const INPUT_MANIFEST_SCHEMA_VERSION: i16 = 1;

pub struct InputManifest {
    nodes: Vec<repository::CaseNodeSummary>,
}

pub fn input_manifest_digest(manifest: &InputManifest) -> String {
    let nodes = manifest
        .nodes
        .iter()
        .map(|node| {
            let mut fields = BTreeMap::new();
            fields.insert("case_node_id".into(), CanonicalValue::I64(node.node_id));
            fields.insert("kind".into(), CanonicalValue::Text(node.kind.clone()));
            fields.insert(
                "confirmed".into(),
                node.confirmed
                    .map(CanonicalValue::Bool)
                    .unwrap_or(CanonicalValue::Null),
            );
            CanonicalValue::Map(fields)
        })
        .collect();
    let mut fields = BTreeMap::new();
    fields.insert(
        "schema_version".into(),
        CanonicalValue::U64(INPUT_MANIFEST_SCHEMA_VERSION as u64),
    );
    fields.insert("nodes".into(), CanonicalValue::List(nodes));
    seal(&CanonicalValue::Map(fields)).to_string()
}

impl From<repository::RepoError> for ProjectionError {
    fn from(value: repository::RepoError) -> Self {
        Self::Repo(value)
    }
}

fn node_id(case_node_id: i64) -> NodeId {
    let value = u64::try_from(case_node_id).expect("case_node_id is always positive");
    NodeId::new(NonZeroU64::new(value).expect("case_node_id is always > 0"))
}

/// Builds the projection for `case` against the fixture's single
/// `identity_requirement`: satisfied if and only if the case has at least
/// one *confirmed* user assertion. This is a deliberate simplification —
/// a real deployment would let a specific evidence item satisfy a specific
/// named requirement, not "any confirmed assertion satisfies the one
/// requirement this bundle happens to have" — recorded honestly here
/// rather than left implicit. See docs/API_CONTRACT.md, "Known
/// simplifications."
pub async fn build_projection(
    pool: &Pool,
    case: CaseRowId,
    fixture: &nexo_policy_ar::Fixture,
) -> Result<(CaseProjection, NodeIdResolver, InputManifest), ProjectionError> {
    let nodes = repository::list_case_nodes_with_kind(pool, case).await?;

    let mut resolver = NodeIdResolver::default();
    let mut factual_support = Vec::new();
    let mut identity_satisfied = false;

    for node in &nodes {
        let id = node_id(node.node_id);
        resolver.record(id, node.node_id);
        match node.kind.as_str() {
            "artifact" => factual_support.push(FactualSupport::Artifact(id)),
            "observation" => factual_support.push(FactualSupport::Observation(id)),
            "user_assertion" => {
                factual_support.push(FactualSupport::UserAssertion(id));
                if node.confirmed == Some(true) {
                    identity_satisfied = true;
                }
            }
            "derived_fact" => factual_support.push(FactualSupport::DerivedFact(id)),
            // "inference" is deliberately excluded: it is a graph node and
            // nothing more, never factual support (docs/CASE_GRAPH_CONTRACT.md).
            _ => {}
        }
    }

    if factual_support.is_empty() {
        return Err(ProjectionError::NoFactualSupportYet);
    }

    let satisfied_requirements: Vec<RequirementId> = if identity_satisfied {
        vec![fixture.identity_requirement]
    } else {
        Vec::new()
    };

    let projection = CaseProjection::try_new(
        factual_support,
        satisfied_requirements,
        fixture.jurisdiction_evidence,
        fixture.freshness_evidence,
    )
    .map_err(ProjectionError::Core)?;

    Ok((projection, resolver, InputManifest { nodes }))
}
