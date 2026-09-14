//! Adversarial example tests for the evidence-graph slice.
//!
//! Each test names the contract guard it pins from
//! `docs/CASE_GRAPH_CONTRACT.md`; deleting or weakening that guard must fail
//! the corresponding test.

use core::num::NonZeroU64;

use crate::{
    ActorId, ArtifactId, ArtifactNode, CaseGraph, CaseGraphError, CaseNode, CaseNodeError,
    ConfidenceBound, ConfirmationState, DerivedFactNode, DigestId, InferenceNode,
    IngestionRecordId, MAX_DERIVATION_INPUTS, MAX_INFERENCE_INPUTS, NodeId, NodeKind, NonEmptyText,
    ObservationNode, ProvenanceId, TextError, ToolId, ToolVersion, UserAssertionNode, UtcInstant,
};

fn id(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).expect("test ids are non-zero"))
}

fn tool_version(seed: u64) -> ToolVersion {
    ToolVersion::new(
        ToolId::new(NonZeroU64::new(seed).expect("tool ids are non-zero")),
        NonZeroU64::new(1).expect("version is non-zero"),
    )
}

fn locator(text: &str) -> NonEmptyText {
    NonEmptyText::try_new(text.to_string()).expect("locator is non-empty")
}

fn artifact_node() -> ArtifactNode {
    ArtifactNode::new(
        DigestId::new(id(11)),
        4096,
        IngestionRecordId::new(NonZeroU64::new(12).expect("ingestion record is non-zero")),
        ProvenanceId::new(id(13)),
    )
}

fn observation(artifact: NodeId) -> ObservationNode {
    ObservationNode::new(
        ArtifactId::new(artifact),
        tool_version(21),
        locator("page=1"),
        UtcInstant::from_unix_seconds(1_000),
    )
}

fn assertion(confirmation: ConfirmationState) -> UserAssertionNode {
    UserAssertionNode::new(
        ActorId::new(NonZeroU64::new(31).expect("actor is non-zero")),
        UtcInstant::from_unix_seconds(2_000),
        confirmation,
    )
}

fn derived(inputs: Vec<NodeId>) -> DerivedFactNode {
    DerivedFactNode::try_new(inputs, tool_version(41)).expect("valid derivation payload")
}

fn inference(inputs: Vec<NodeId>) -> InferenceNode {
    InferenceNode::try_new(inputs, tool_version(51), ConfidenceBound::Low)
        .expect("valid inference payload")
}

/// Invariant: ids are sequential from one, and the graph reports size and
/// kinds faithfully.
#[test]
fn inserts_assign_sequential_ids_starting_at_one() {
    let mut graph = CaseGraph::new();
    assert!(graph.is_empty());

    let first = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let second = graph
        .insert(CaseNode::UserAssertion(assertion(
            ConfirmationState::Confirmed,
        )))
        .expect("assertion payload is always valid");

    assert_eq!(first, id(1));
    assert_eq!(second, id(2));
    assert_eq!(graph.len(), 2);
    assert!(!graph.is_empty());
    assert_eq!(graph.kind_of(first), Some(NodeKind::Artifact));
    assert_eq!(graph.kind_of(second), Some(NodeKind::UserAssertion));
}

/// Invariant: an observation's target must be an artifact, not merely an
/// existing node.
#[test]
fn observation_rejects_a_non_artifact_target() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let assertion_id = graph
        .insert(CaseNode::UserAssertion(assertion(
            ConfirmationState::Unconfirmed,
        )))
        .expect("assertion payload is always valid");

    let result = graph.insert(CaseNode::Observation(observation(assertion_id)));

    assert_eq!(result.unwrap_err(), CaseGraphError::ReferenceKindMismatch);
    // The artifact remains unused but valid: the guard is per edge.
    assert_eq!(graph.kind_of(artifact), Some(NodeKind::Artifact));
}

/// Invariant: a reference that names no inserted node fails closed.
#[test]
fn observation_rejects_a_dangling_artifact_reference() {
    let mut graph = CaseGraph::new();
    let result = graph.insert(CaseNode::Observation(observation(id(99))));

    assert_eq!(result.unwrap_err(), CaseGraphError::DanglingReference);
    assert!(graph.is_empty());
}

/// Invariant: an observation over an existing artifact is admitted.
#[test]
fn observation_on_an_artifact_is_admitted() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");

    let observation_id = graph
        .insert(CaseNode::Observation(observation(artifact)))
        .expect("the artifact target exists");

    assert_eq!(graph.kind_of(observation_id), Some(NodeKind::Observation));
}

/// ADR 0005: a derivation cannot consume an interpretation; the guard fires
/// even when the payload is otherwise valid.
#[test]
fn derivation_rejects_an_inference_input() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let interpretation = graph
        .insert(CaseNode::Inference(inference(vec![artifact])))
        .expect("an inference may cite an artifact");

    let result = graph.insert(CaseNode::DerivedFact(derived(vec![interpretation])));

    assert_eq!(
        result.unwrap_err(),
        CaseGraphError::InferenceInDerivationInputs
    );
}

/// Invariant: derivations accept every factual-support kind, including other
/// derivations.
#[test]
fn derivation_accepts_mixed_factual_support_inputs() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let assertion_id = graph
        .insert(CaseNode::UserAssertion(assertion(
            ConfirmationState::Confirmed,
        )))
        .expect("assertion payload is always valid");
    let observation_id = graph
        .insert(CaseNode::Observation(observation(artifact)))
        .expect("the artifact target exists");

    let first = graph
        .insert(CaseNode::DerivedFact(derived(vec![
            artifact,
            assertion_id,
            observation_id,
        ])))
        .expect("all three inputs are factual-support kinds");
    let second = graph
        .insert(CaseNode::DerivedFact(derived(vec![first])))
        .expect("a derivation is itself factual support");

    assert_eq!(graph.kind_of(second), Some(NodeKind::DerivedFact));
}

/// Hostile-input bound: a derivation with no declared inputs is a provenance
/// gap, not an empty default.
#[test]
fn derivation_payload_rejects_empty_inputs() {
    let result = DerivedFactNode::try_new(vec![], tool_version(41));

    assert_eq!(result.unwrap_err(), CaseNodeError::EmptyInputs);
}

/// Hostile-input bound: an adapter cannot attach unbounded fan-in to one
/// derivation.
#[test]
fn derivation_payload_rejects_inputs_above_the_bound() {
    let inputs = (1..=(MAX_DERIVATION_INPUTS as u64 + 1)).map(id).collect();

    let result = DerivedFactNode::try_new(inputs, tool_version(41));

    assert_eq!(result.unwrap_err(), CaseNodeError::TooManyDerivationInputs);
}

/// Invariant: an inference may cite any node kind, including another
/// inference; interpretations may build on interpretations.
#[test]
fn inference_accepts_every_node_kind_including_inference() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let first_interpretation = graph
        .insert(CaseNode::Inference(inference(vec![artifact])))
        .expect("an inference may cite an artifact");

    let second_interpretation = graph
        .insert(CaseNode::Inference(inference(vec![
            artifact,
            first_interpretation,
        ])))
        .expect("an inference may cite another inference");

    assert_eq!(
        graph.kind_of(second_interpretation),
        Some(NodeKind::Inference)
    );
}

/// Hostile-input bound: inference payloads respect the same presence and
/// fan-in discipline as derivations.
#[test]
fn inference_payload_rejects_empty_and_over_bound_inputs() {
    assert_eq!(
        InferenceNode::try_new(vec![], tool_version(51), ConfidenceBound::Medium).unwrap_err(),
        CaseNodeError::EmptyInputs
    );

    let inputs = (1..=(MAX_INFERENCE_INPUTS as u64 + 1)).map(id).collect();
    assert_eq!(
        InferenceNode::try_new(inputs, tool_version(51), ConfidenceBound::Medium).unwrap_err(),
        CaseNodeError::TooManyInferenceInputs
    );
}

/// Provenance presence: the locator of an observation cannot be blank text.
#[test]
fn observation_locator_rejects_blank_text() {
    assert_eq!(
        NonEmptyText::try_new("   ".to_string()),
        Err(TextError::Empty)
    );
}

/// Boundary: the factual-support membership predicate agrees with the
/// `FactualSupport` enum, whose absent `Inference` variant is pinned by the
/// compile-time guard in `property_tests::support_kind`.
#[test]
fn node_kind_factual_support_membership_matches_the_boundary() {
    assert!(NodeKind::Artifact.is_factual_support());
    assert!(NodeKind::Observation.is_factual_support());
    assert!(NodeKind::UserAssertion.is_factual_support());
    assert!(NodeKind::DerivedFact.is_factual_support());
    assert!(!NodeKind::Inference.is_factual_support());
}

/// Invariant: derivation ancestry walks the derivation subgraph in a
/// deterministic depth-first order, inputs in declared order.
#[test]
fn derivation_ancestors_walks_the_derivation_subgraph() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let first = graph
        .insert(CaseNode::DerivedFact(derived(vec![artifact])))
        .expect("the artifact input exists");
    let second = graph
        .insert(CaseNode::DerivedFact(derived(vec![first])))
        .expect("the derivation input exists");

    assert_eq!(graph.derivation_ancestors(second), vec![first, artifact]);
    assert_eq!(graph.derivation_ancestors(first), vec![artifact]);
    assert!(graph.derivation_ancestors(artifact).is_empty());
}

/// Structural guarantee: under sequential id assignment a two-node derivation
/// cycle cannot be expressed, because the first node would have to reference
/// a node that does not exist yet.
#[test]
fn derivation_cycle_is_unrepresentable_under_sequential_assignment() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");

    let attempted_cycle = graph.insert(CaseNode::DerivedFact(derived(vec![artifact, id(99)])));

    assert_eq!(
        attempted_cycle.unwrap_err(),
        CaseGraphError::DanglingReference
    );
    assert_eq!(graph.len(), 1);
}

/// Invariant: an inference-only context contributes nothing an action can be
/// assembled from; the graph itself reports the boundary.
#[test]
fn an_inference_only_graph_supports_no_factual_conclusion() {
    let mut graph = CaseGraph::new();
    let artifact = graph
        .insert(CaseNode::Artifact(artifact_node()))
        .expect("artifact payload is always valid");
    let interpretation = graph
        .insert(CaseNode::Inference(inference(vec![artifact])))
        .expect("an inference may cite an artifact");

    let inference_kinds: Vec<NodeKind> = [artifact, interpretation]
        .into_iter()
        .filter_map(|node_id| graph.kind_of(node_id))
        .filter(|kind| !kind.is_factual_support())
        .collect();

    assert_eq!(inference_kinds, vec![NodeKind::Inference]);
}

/// Hostile-input guard: unknown ids resolve to nothing rather than panicking
/// or fabricating a node.
#[test]
fn unknown_ids_resolve_to_nothing() {
    let graph = CaseGraph::new();

    assert_eq!(graph.get(id(500)), None);
    assert_eq!(graph.kind_of(id(500)), None);
}
