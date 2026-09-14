//! Typed evidence-graph nodes and validated insertion for NEXO.
//!
//! This module implements the smallest complete slice of
//! `docs/CASE_GRAPH_CONTRACT.md`: the five node kinds with mandatory
//! provenance, the reference rules between them, and the evidence /
//! interpretation boundary of ADR 0001 and ADR 0005.
//!
//! Absent provenance is unrepresentable rather than rejected: every payload
//! field is a required constructor argument. Reference integrity, kind
//! compatibility, the inference boundary, and derivation acyclicity are
//! checked at insertion into a [`CaseGraph`], and every violation fails
//! closed with a typed error.

use core::num::NonZeroU64;
use std::collections::BTreeSet;

use crate::{ArtifactId, DigestId, NodeId, NonEmptyText, ProvenanceId, UtcInstant};

/// Maximum inputs retained by one `DerivedFact`.
pub const MAX_DERIVATION_INPUTS: usize = 64;
/// Maximum inputs retained by one `Inference`.
pub const MAX_INFERENCE_INPUTS: usize = 64;

/// Opaque identity of a person or system actor who declared an assertion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ActorId(NonZeroU64);

impl ActorId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// Opaque identity of an extractor, transformation, or method.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToolId(NonZeroU64);

impl ToolId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// Identity and version of the tool that produced a node.
///
/// Version zero is unrepresentable: an extractor, transformation, or method
/// with no known version is a provenance gap, not a default.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolVersion {
    tool: ToolId,
    version: NonZeroU64,
}

impl ToolVersion {
    pub const fn new(tool: ToolId, version: NonZeroU64) -> Self {
        Self { tool, version }
    }

    pub const fn tool(&self) -> ToolId {
        self.tool
    }

    pub const fn version(&self) -> NonZeroU64 {
        self.version
    }
}

/// Confirmation state of a user assertion; typed rather than boolean so a
/// third state cannot be smuggled into a flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfirmationState {
    Confirmed,
    Unconfirmed,
}

/// Bounded confidence on an [`Inference`]: finite, discrete, ordered, never
/// numeric. The scale is ordinal only; `High` is not "twice" `Low`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ConfidenceBound {
    Low,
    Medium,
    High,
}

/// A reference to an ingestion record held outside the domain core.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IngestionRecordId(NonZeroU64);

impl IngestionRecordId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// Original bytes submitted to NEXO, represented without touching bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactNode {
    digest: DigestId,
    size_bytes: u64,
    ingestion: IngestionRecordId,
    source_provenance: ProvenanceId,
}

impl ArtifactNode {
    pub const fn new(
        digest: DigestId,
        size_bytes: u64,
        ingestion: IngestionRecordId,
        source_provenance: ProvenanceId,
    ) -> Self {
        Self {
            digest,
            size_bytes,
            ingestion,
            source_provenance,
        }
    }

    pub const fn digest(&self) -> DigestId {
        self.digest
    }

    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub const fn ingestion(&self) -> IngestionRecordId {
        self.ingestion
    }

    pub const fn source_provenance(&self) -> ProvenanceId {
        self.source_provenance
    }
}

/// A direct extraction from an artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationNode {
    artifact: ArtifactId,
    extractor: ToolVersion,
    locator: NonEmptyText,
    recorded_at: UtcInstant,
}

impl ObservationNode {
    pub fn new(
        artifact: ArtifactId,
        extractor: ToolVersion,
        locator: NonEmptyText,
        recorded_at: UtcInstant,
    ) -> Self {
        Self {
            artifact,
            extractor,
            locator,
            recorded_at,
        }
    }

    pub const fn artifact(&self) -> ArtifactId {
        self.artifact
    }

    pub const fn extractor(&self) -> ToolVersion {
        self.extractor
    }

    pub fn locator(&self) -> &NonEmptyText {
        &self.locator
    }

    pub const fn recorded_at(&self) -> UtcInstant {
        self.recorded_at
    }
}

/// A statement declared by a person.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserAssertionNode {
    actor: ActorId,
    recorded_at: UtcInstant,
    confirmation: ConfirmationState,
}

impl UserAssertionNode {
    pub const fn new(
        actor: ActorId,
        recorded_at: UtcInstant,
        confirmation: ConfirmationState,
    ) -> Self {
        Self {
            actor,
            recorded_at,
            confirmation,
        }
    }

    pub const fn actor(&self) -> ActorId {
        self.actor
    }

    pub const fn recorded_at(&self) -> UtcInstant {
        self.recorded_at
    }

    pub const fn confirmation(&self) -> ConfirmationState {
        self.confirmation
    }
}

/// Why a case-node payload could not be constructed safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseNodeError {
    EmptyInputs,
    TooManyDerivationInputs,
    TooManyInferenceInputs,
}

/// A reproducible transformation over declared inputs.
///
/// The payload constructor enforces presence and bounds only; the
/// factual-support restriction on inputs (ADR 0005) is enforced at graph
/// insertion, where referenced nodes exist to be checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedFactNode {
    inputs: Vec<NodeId>,
    transformation: ToolVersion,
}

impl DerivedFactNode {
    pub fn try_new(
        inputs: Vec<NodeId>,
        transformation: ToolVersion,
    ) -> Result<Self, CaseNodeError> {
        if inputs.len() > MAX_DERIVATION_INPUTS {
            return Err(CaseNodeError::TooManyDerivationInputs);
        }
        if inputs.is_empty() {
            return Err(CaseNodeError::EmptyInputs);
        }
        Ok(Self {
            inputs,
            transformation,
        })
    }

    pub fn inputs(&self) -> &[NodeId] {
        &self.inputs
    }

    pub const fn transformation(&self) -> ToolVersion {
        self.transformation
    }
}

/// An interpretation not directly observed.
///
/// An inference may cite any graph node, including another inference; it can
/// never become factual support itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InferenceNode {
    inputs: Vec<NodeId>,
    method: ToolVersion,
    confidence: ConfidenceBound,
}

impl InferenceNode {
    pub fn try_new(
        inputs: Vec<NodeId>,
        method: ToolVersion,
        confidence: ConfidenceBound,
    ) -> Result<Self, CaseNodeError> {
        if inputs.len() > MAX_INFERENCE_INPUTS {
            return Err(CaseNodeError::TooManyInferenceInputs);
        }
        if inputs.is_empty() {
            return Err(CaseNodeError::EmptyInputs);
        }
        Ok(Self {
            inputs,
            method,
            confidence,
        })
    }

    pub fn inputs(&self) -> &[NodeId] {
        &self.inputs
    }

    pub const fn method(&self) -> ToolVersion {
        self.method
    }

    pub const fn confidence(&self) -> ConfidenceBound {
        self.confidence
    }
}

/// The kind of a graph node, independent of its payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Artifact,
    Observation,
    UserAssertion,
    DerivedFact,
    Inference,
}

impl NodeKind {
    /// Whether nodes of this kind may appear in `FactualSupport`.
    ///
    /// The load-bearing half of the evidence/interpretation boundary is the
    /// absence of an `Inference` variant on `FactualSupport` itself; this
    /// predicate mirrors that contract so graph-level code cannot drift from
    /// it. Adding a node kind forces an explicit decision here.
    pub const fn is_factual_support(self) -> bool {
        matches!(
            self,
            NodeKind::Artifact
                | NodeKind::Observation
                | NodeKind::UserAssertion
                | NodeKind::DerivedFact
        )
    }
}

/// A node of the evidence graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaseNode {
    Artifact(ArtifactNode),
    Observation(ObservationNode),
    UserAssertion(UserAssertionNode),
    DerivedFact(DerivedFactNode),
    Inference(InferenceNode),
}

impl CaseNode {
    pub const fn kind(&self) -> NodeKind {
        match self {
            CaseNode::Artifact(_) => NodeKind::Artifact,
            CaseNode::Observation(_) => NodeKind::Observation,
            CaseNode::UserAssertion(_) => NodeKind::UserAssertion,
            CaseNode::DerivedFact(_) => NodeKind::DerivedFact,
            CaseNode::Inference(_) => NodeKind::Inference,
        }
    }
}

/// Why a node could not be admitted into a [`CaseGraph`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaseGraphError {
    /// A reference names a node id that is not in the graph.
    DanglingReference,
    /// A reference resolves to a node of a kind the edge does not permit.
    ReferenceKindMismatch,
    /// A `DerivedFact` input is an `Inference` (ADR 0005).
    InferenceInDerivationInputs,
    /// A cycle is reachable through derivation inputs. Unreachable under
    /// sequential id assignment; retained for future bulk-load paths.
    DerivedInputCycle,
}

/// A growing evidence graph with validated insertion.
///
/// Node ids are assigned sequentially from one; references may only point at
/// already-inserted nodes, which makes dangling edges, kind mismatches, and
/// derivation cycles detectable at insertion time.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaseGraph {
    nodes: Vec<CaseNode>,
}

impl CaseGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: NodeId) -> Option<&CaseNode> {
        let index = usize::try_from(id.0.get()).ok()?.checked_sub(1)?;
        self.nodes.get(index)
    }

    pub fn kind_of(&self, id: NodeId) -> Option<NodeKind> {
        self.get(id).map(CaseNode::kind)
    }

    /// Inserts a node after validating every reference it carries.
    ///
    /// Absent payload provenance cannot reach this point: it is
    /// unrepresentable in the node constructors.
    pub fn insert(&mut self, node: CaseNode) -> Result<NodeId, CaseGraphError> {
        match &node {
            CaseNode::Artifact(_) | CaseNode::UserAssertion(_) => {}
            CaseNode::Observation(observation) => {
                let target = observation.artifact().0;
                match self.kind_of(target) {
                    None => return Err(CaseGraphError::DanglingReference),
                    Some(NodeKind::Artifact) => {}
                    Some(_) => return Err(CaseGraphError::ReferenceKindMismatch),
                }
            }
            CaseNode::DerivedFact(derived) => {
                for input in derived.inputs() {
                    match self.kind_of(*input) {
                        None => return Err(CaseGraphError::DanglingReference),
                        Some(NodeKind::Inference) => {
                            return Err(CaseGraphError::InferenceInDerivationInputs);
                        }
                        Some(_) => {}
                    }
                }
                let next_id = self.next_id();
                for input in derived.inputs() {
                    if self.derivation_reaches(*input, next_id) {
                        return Err(CaseGraphError::DerivedInputCycle);
                    }
                }
            }
            CaseNode::Inference(inference) => {
                for input in inference.inputs() {
                    if self.kind_of(*input).is_none() {
                        return Err(CaseGraphError::DanglingReference);
                    }
                }
            }
        }
        let id = self.next_id();
        self.nodes.push(node);
        Ok(id)
    }

    /// Node ids reachable from `id` through derivation-input edges.
    ///
    /// Discovery order is deterministic: depth-first, inputs in declared
    /// order. Only `DerivedFact` nodes carry derivation edges.
    pub fn derivation_ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        self.push_derivation_inputs(id, &mut stack);
        while let Some(current) = stack.pop() {
            if visited.insert(current) {
                order.push(current);
                self.push_derivation_inputs(current, &mut stack);
            }
        }
        order
    }

    fn next_id(&self) -> NodeId {
        let value =
            NonZeroU64::new(self.nodes.len() as u64 + 1).expect("graph length cannot exhaust u64");
        NodeId::new(value)
    }

    fn push_derivation_inputs(&self, id: NodeId, stack: &mut Vec<NodeId>) {
        if let Some(CaseNode::DerivedFact(derived)) = self.get(id) {
            stack.extend(derived.inputs().iter().rev().copied());
        }
    }

    /// Whether `target` is reachable from `start` through derivation edges.
    fn derivation_reaches(&self, start: NodeId, target: NodeId) -> bool {
        let mut stack = vec![start];
        let mut visited = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if current == target {
                return true;
            }
            if visited.insert(current) {
                self.push_derivation_inputs(current, &mut stack);
            }
        }
        false
    }
}
