//! Generative tests over validated case-graph insertion.
//!
//! These properties quantify over random insertion plans instead of one
//! hand-picked hostile input per test. Plans are executed with repair
//! semantics, documented on [`execute_plan`]: an observation with no
//! artifact inserted so far becomes an artifact, derivation and inference
//! steps without predecessor nodes are skipped, and derivation inputs
//! rejected at the inference boundary count as honest rejections.
//!
//! What this proves: for arbitrary plans within the exercised shapes, every
//! inserted node keeps resolvable and kind-compatible references, references
//! past the last assigned id always fail closed as dangling, an observation
//! never targets a non-artifact node, and a derivation never accepts an
//! inference input, whatever the surrounding graph contains.
//!
//! What this does not prove: payload-level bounds (pinned by
//! `case_graph_tests`), evaluation semantics (no evaluator exists yet), or
//! that the cardinality bounds themselves are the right ones.

use core::num::NonZeroU64;

use proptest::prelude::*;

use crate::{
    ActorId, ArtifactId, ArtifactNode, CaseGraph, CaseGraphError, CaseNode, ConfidenceBound,
    ConfirmationState, DerivedFactNode, DigestId, InferenceNode, IngestionRecordId, NodeId,
    NodeKind, NonEmptyText, ObservationNode, ProvenanceId, ToolId, ToolVersion, UserAssertionNode,
    UtcInstant,
};

#[derive(Clone, Debug)]
enum Step {
    Artifact,
    Observation { artifact_index: usize },
    Assertion,
    Derived { input_indices: Vec<usize> },
    Inference { input_indices: Vec<usize> },
}

fn step_strategy() -> impl Strategy<Value = Step> {
    // prop_oneof! requires every branch to carry an explicit weight once any
    // branch is weighted; artifacts are biased to keep plans buildable.
    prop_oneof![
        2 => Just(Step::Artifact),
        1 => (0usize..32).prop_map(|artifact_index| Step::Observation { artifact_index }),
        1 => Just(Step::Assertion),
        1 => prop::collection::vec(0usize..32, 1..=4)
            .prop_map(|input_indices| Step::Derived { input_indices }),
        1 => prop::collection::vec(0usize..32, 1..=4)
            .prop_map(|input_indices| Step::Inference { input_indices }),
    ]
}

fn id(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).expect("test ids are non-zero"))
}

fn tool_version(seed: u64) -> ToolVersion {
    ToolVersion::new(
        ToolId::new(NonZeroU64::new(seed.max(1)).expect("tool ids are non-zero")),
        NonZeroU64::new(1).expect("version is non-zero"),
    )
}

fn locator() -> NonEmptyText {
    NonEmptyText::try_new("locator".to_string()).expect("locator is non-empty")
}

fn artifact_node(seed: u64) -> ArtifactNode {
    ArtifactNode::new(
        DigestId::new(id(seed.max(1))),
        4096,
        IngestionRecordId::new(NonZeroU64::new(seed.max(1)).expect("non-zero")),
        ProvenanceId::new(id(seed.max(1))),
    )
}

fn assertion_node() -> UserAssertionNode {
    UserAssertionNode::new(
        ActorId::new(NonZeroU64::new(31).expect("actor is non-zero")),
        UtcInstant::from_unix_seconds(2_000),
        ConfirmationState::Confirmed,
    )
}

/// Executes a plan sequentially against a fresh graph.
///
/// Reference indices are mapped onto already-inserted nodes modulo the
/// current graph length, so every executed reference resolves by
/// construction; the guards under test are the kind, boundary, and
/// resolution checks, not accidental dangling ids (those are pinned
/// separately below).
fn execute_plan(plan: &[Step]) -> (CaseGraph, Vec<(NodeId, NodeKind)>) {
    let mut graph = CaseGraph::new();
    let mut inserted: Vec<(NodeId, NodeKind)> = Vec::new();
    let mut artifacts: Vec<NodeId> = Vec::new();
    let mut counter = 0u64;

    for step in plan {
        counter += 1;
        match step {
            Step::Artifact => {
                let node_id = graph
                    .insert(CaseNode::Artifact(artifact_node(counter)))
                    .expect("artifact payload is always valid");
                artifacts.push(node_id);
                inserted.push((node_id, NodeKind::Artifact));
            }
            Step::Observation { artifact_index } => {
                let target = if artifacts.is_empty() {
                    None
                } else {
                    artifacts.get(artifact_index % artifacts.len()).copied()
                };
                let Some(target) = target else {
                    // Repair: an observation before any artifact becomes an artifact.
                    let node_id = graph
                        .insert(CaseNode::Artifact(artifact_node(counter)))
                        .expect("artifact payload is always valid");
                    artifacts.push(node_id);
                    inserted.push((node_id, NodeKind::Artifact));
                    continue;
                };
                let observation = ObservationNode::new(
                    ArtifactId::new(target),
                    tool_version(counter),
                    locator(),
                    UtcInstant::from_unix_seconds(1_000),
                );
                let node_id = graph
                    .insert(CaseNode::Observation(observation))
                    .expect("the artifact target is inserted");
                inserted.push((node_id, NodeKind::Observation));
            }
            Step::Assertion => {
                let node_id = graph
                    .insert(CaseNode::UserAssertion(assertion_node()))
                    .expect("assertion payload is always valid");
                inserted.push((node_id, NodeKind::UserAssertion));
            }
            Step::Derived { input_indices } => {
                if inserted.is_empty() {
                    continue;
                }
                let known: Vec<NodeId> = inserted.iter().map(|(node_id, _)| *node_id).collect();
                let inputs: Vec<NodeId> = input_indices
                    .iter()
                    .map(|index| known[index % known.len()])
                    .collect();
                let derived =
                    DerivedFactNode::try_new(inputs, tool_version(counter)).expect("bounded");
                match graph.insert(CaseNode::DerivedFact(derived)) {
                    Ok(node_id) => inserted.push((node_id, NodeKind::DerivedFact)),
                    // Honest rejection pinned separately; never a failure here.
                    Err(CaseGraphError::InferenceInDerivationInputs) => {}
                    Err(other) => panic!("unexpected insertion failure: {other:?}"),
                }
            }
            Step::Inference { input_indices } => {
                if inserted.is_empty() {
                    continue;
                }
                let known: Vec<NodeId> = inserted.iter().map(|(node_id, _)| *node_id).collect();
                let inputs: Vec<NodeId> = input_indices
                    .iter()
                    .map(|index| known[index % known.len()])
                    .collect();
                let inference =
                    InferenceNode::try_new(inputs, tool_version(counter), ConfidenceBound::Medium)
                        .expect("bounded");
                let node_id = graph
                    .insert(CaseNode::Inference(inference))
                    .expect("every input is an inserted node");
                inserted.push((node_id, NodeKind::Inference));
            }
        }
    }

    (graph, inserted)
}

proptest! {
    /// ∀ plan: after execution, every inserted node resolves with the kind
    /// it was inserted as, every observation target resolves to an artifact,
    /// and every derivation input resolves to a factual-support kind. Pins
    /// the success path plus the resolution and kind guards of
    /// `CaseGraph::insert`.
    #[test]
    fn executed_plans_keep_every_reference_resolved_and_kind_compatible(
        plan in prop::collection::vec(step_strategy(), 0..=24),
    ) {
        let (graph, inserted) = execute_plan(&plan);
        prop_assert_eq!(graph.len(), inserted.len());

        for (node_id, kind) in &inserted {
            prop_assert_eq!(graph.kind_of(*node_id), Some(*kind));
            match graph.get(*node_id).expect("inserted node resolves") {
                CaseNode::Observation(observation) => {
                    let target = observation.artifact().0;
                    prop_assert_eq!(graph.kind_of(target), Some(NodeKind::Artifact));
                }
                CaseNode::DerivedFact(derived) => {
                    for input in derived.inputs() {
                        let input_kind = graph.kind_of(*input);
                        prop_assert!(input_kind.is_some());
                        prop_assert!(input_kind.expect("checked above").is_factual_support());
                    }
                }
                CaseNode::Inference(inference) => {
                    for input in inference.inputs() {
                        prop_assert!(graph.kind_of(*input).is_some());
                    }
                }
                CaseNode::Artifact(_) | CaseNode::UserAssertion(_) => {}
            }
        }
    }

    /// ∀ graph, ∀ gap ≥ 1: an edge aimed past the last assigned id fails
    /// closed as a dangling reference for every edge shape, whatever the
    /// surrounding graph contains. Pins the resolution guard of
    /// `CaseGraph::insert`.
    #[test]
    fn references_past_the_last_assigned_id_are_dangling(
        plan in prop::collection::vec(step_strategy(), 0..=16),
        gap in 1u64..1000,
    ) {
        let (mut graph, _) = execute_plan(&plan);
        let missing = id(graph.len() as u64 + gap);

        let derived = DerivedFactNode::try_new(vec![missing], tool_version(900))
            .expect("one input is within bounds");
        prop_assert_eq!(
            graph.insert(CaseNode::DerivedFact(derived)),
            Err(CaseGraphError::DanglingReference)
        );

        let observation = ObservationNode::new(
            ArtifactId::new(missing),
            tool_version(901),
            locator(),
            UtcInstant::from_unix_seconds(0),
        );
        prop_assert_eq!(
            graph.insert(CaseNode::Observation(observation)),
            Err(CaseGraphError::DanglingReference)
        );

        let inference = InferenceNode::try_new(vec![missing], tool_version(902), ConfidenceBound::High)
            .expect("one input is within bounds");
        prop_assert_eq!(
            graph.insert(CaseNode::Inference(inference)),
            Err(CaseGraphError::DanglingReference)
        );
    }

    /// ∀ graph: an observation aimed at an existing non-artifact node fails
    /// closed as a kind mismatch, whatever the surrounding graph contains.
    /// Pins the observation kind guard of `CaseGraph::insert`, which the
    /// dangling-reference property above cannot reach.
    #[test]
    fn observations_never_target_non_artifacts(
        plan in prop::collection::vec(step_strategy(), 0..=16),
    ) {
        let (mut graph, _) = execute_plan(&plan);

        // A fresh assertion is admitted into every graph and then serves as
        // a guaranteed non-artifact probe target.
        let assertion_id = graph
            .insert(CaseNode::UserAssertion(assertion_node()))
            .expect("assertion payload is always valid");
        let probe = ObservationNode::new(
            ArtifactId::new(assertion_id),
            tool_version(920),
            locator(),
            UtcInstant::from_unix_seconds(0),
        );
        prop_assert_eq!(
            graph.insert(CaseNode::Observation(probe)),
            Err(CaseGraphError::ReferenceKindMismatch)
        );
    }

    /// ∀ graph: a derivation citing an inference input fails closed, alone
    /// and mixed with a valid factual input. Pins ADR 0005 at the graph
    /// boundary.
    #[test]
    fn derivations_never_accept_inference_inputs(
        plan in prop::collection::vec(step_strategy(), 0..=16),
    ) {
        let (mut graph, inserted) = execute_plan(&plan);
        let anchor = match inserted.first() {
            Some((node_id, _)) => *node_id,
            None => graph
                .insert(CaseNode::Artifact(artifact_node(1)))
                .expect("artifact payload is always valid"),
        };

        let interpretation = InferenceNode::try_new(
            vec![anchor],
            tool_version(910),
            ConfidenceBound::Low,
        )
        .expect("one input is within bounds");
        let inference_id = graph
            .insert(CaseNode::Inference(interpretation))
            .expect("the anchor node exists");

        let derived = DerivedFactNode::try_new(vec![inference_id], tool_version(911))
            .expect("one input is within bounds");
        prop_assert_eq!(
            graph.insert(CaseNode::DerivedFact(derived)),
            Err(CaseGraphError::InferenceInDerivationInputs)
        );

        // The first inserted node is never an inference (an inference needs
        // a predecessor), so the mixed input list contains exactly one
        // invalid edge; the guard must still fire.
        let mixed = DerivedFactNode::try_new(vec![anchor, inference_id], tool_version(912))
            .expect("two inputs are within bounds");
        prop_assert_eq!(
            graph.insert(CaseNode::DerivedFact(mixed)),
            Err(CaseGraphError::InferenceInDerivationInputs)
        );
    }
}
