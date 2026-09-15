//! Local preparation lifecycle for a supported rights route.

use crate::{ActionOption, ArtifactId, DigestId, NodeId, ToolVersion};

/// The material NEXO may prepare without performing an external legal act.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationKind {
    DraftRequest,
    EvidencePackage,
    Export,
}

/// The authorization value and the identity recorded for it as one token.
/// This prevents the preparation factory from receiving two independently
/// supplied representations of the action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentifiedActionOption {
    action: ActionOption,
    identity: NodeId,
}

/// Capability intended to be emitted by the evaluation/application boundary.
/// Its fields are private so preparation cannot be assembled from
/// independently supplied action, snapshot, policy, and input references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPreparationSnapshot {
    action: IdentifiedActionOption,
    evaluation_snapshot: NodeId,
    policy_bundle_digest: DigestId,
    input_manifest_digest: DigestId,
}

impl VerifiedPreparationSnapshot {
    #[allow(dead_code)]
    pub(crate) fn from_verified_evaluation(
        action: IdentifiedActionOption,
        evaluation_snapshot: NodeId,
        policy_bundle_digest: DigestId,
        input_manifest_digest: DigestId,
    ) -> Self {
        Self {
            action,
            evaluation_snapshot,
            policy_bundle_digest,
            input_manifest_digest,
        }
    }
    pub const fn action(&self) -> &IdentifiedActionOption {
        &self.action
    }
    pub const fn evaluation_snapshot(&self) -> NodeId {
        self.evaluation_snapshot
    }
    pub const fn policy_bundle_digest(&self) -> DigestId {
        self.policy_bundle_digest
    }
    pub const fn input_manifest_digest(&self) -> DigestId {
        self.input_manifest_digest
    }
}

impl ActionOption {
    pub fn with_identity(self, identity: NodeId) -> IdentifiedActionOption {
        IdentifiedActionOption {
            action: self,
            identity,
        }
    }
}

impl IdentifiedActionOption {
    pub const fn action(&self) -> &ActionOption {
        &self.action
    }
    pub const fn identity(&self) -> NodeId {
        self.identity
    }
}

/// Inputs that identify the evaluation from which a preparation is derived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationPlan {
    action_option: NodeId,
    evaluation_snapshot: NodeId,
    policy_bundle_digest: DigestId,
    input_manifest_digest: DigestId,
    kind: PreparationKind,
    generator: ToolVersion,
}

impl PreparationPlan {
    pub(crate) const fn new(
        action_option: NodeId,
        evaluation_snapshot: NodeId,
        policy_bundle_digest: DigestId,
        input_manifest_digest: DigestId,
        kind: PreparationKind,
        generator: ToolVersion,
    ) -> Self {
        Self {
            action_option,
            evaluation_snapshot,
            policy_bundle_digest,
            input_manifest_digest,
            kind,
            generator,
        }
    }

    /// Compatibility shim for callers migrating to
    /// [`PreparationPlan::from_verified_snapshot`]. It intentionally never
    /// succeeds: a public method cannot mint a plan from independently
    /// supplied references.
    pub fn from_supported_action(
        identified_action: &IdentifiedActionOption,
        evaluation_snapshot: NodeId,
        policy_bundle_digest: DigestId,
        input_manifest_digest: DigestId,
        kind: PreparationKind,
        generator: ToolVersion,
    ) -> Result<Self, PreparationPlanError> {
        let _ = (
            identified_action,
            evaluation_snapshot,
            policy_bundle_digest,
            input_manifest_digest,
            kind,
            generator,
        );
        Err(PreparationPlanError::VerifiedSnapshotRequired)
    }

    pub fn from_verified_snapshot(
        snapshot: &VerifiedPreparationSnapshot,
        kind: PreparationKind,
        generator: ToolVersion,
    ) -> Result<Self, PreparationPlanError> {
        if !snapshot.action.action.is_available() {
            return Err(PreparationPlanError::ActionNotAvailable);
        }
        Ok(Self::new(
            snapshot.action.identity,
            snapshot.evaluation_snapshot,
            snapshot.policy_bundle_digest,
            snapshot.input_manifest_digest,
            kind,
            generator,
        ))
    }

    pub const fn action_option(&self) -> NodeId {
        self.action_option
    }
    pub const fn evaluation_snapshot(&self) -> NodeId {
        self.evaluation_snapshot
    }
    pub const fn policy_bundle_digest(&self) -> DigestId {
        self.policy_bundle_digest
    }
    pub const fn input_manifest_digest(&self) -> DigestId {
        self.input_manifest_digest
    }
    pub const fn kind(&self) -> PreparationKind {
        self.kind
    }
    pub const fn generator(&self) -> ToolVersion {
        self.generator
    }

    pub fn prepare(self, output_artifact: ArtifactId, output_digest: DigestId) -> Prepared {
        Prepared {
            plan: self,
            output_artifact,
            output_digest,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationPlanError {
    ActionNotAvailable,
    VerifiedSnapshotRequired,
}

/// A generated material, not yet made available to the person.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Prepared {
    plan: PreparationPlan,
    output_artifact: ArtifactId,
    output_digest: DigestId,
}

impl Prepared {
    pub const fn plan(&self) -> &PreparationPlan {
        &self.plan
    }
    pub const fn output_artifact(&self) -> ArtifactId {
        self.output_artifact
    }
    pub const fn output_digest(&self) -> DigestId {
        self.output_digest
    }
    pub fn export(self) -> Exported {
        Exported { material: self }
    }
    pub fn invalidate(self, reason: InvalidationReason) -> Invalidated {
        Invalidated {
            material: self,
            reason,
        }
    }
}

/// A material made available locally; this does not mean sent or filed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Exported {
    material: Prepared,
}

impl Exported {
    pub const fn material(&self) -> &Prepared {
        &self.material
    }
    pub fn invalidate(self, reason: InvalidationReason) -> Invalidated {
        Invalidated {
            material: self.material,
            reason,
        }
    }
}

/// A preserved material that must not be presented as current.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invalidated {
    material: Prepared,
    reason: InvalidationReason,
}

impl Invalidated {
    pub const fn material(&self) -> &Prepared {
        &self.material
    }
    pub const fn reason(&self) -> InvalidationReason {
        self.reason
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidationReason {
    RouteNoLongerSupported,
    PolicyChanged,
    InputsNotReproducible,
}
