//! Local preparation lifecycle for a supported rights route.

use crate::{ArtifactId, DigestId, NodeId, ToolVersion};

/// The material NEXO may prepare without performing an external legal act.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationKind {
    DraftRequest,
    EvidencePackage,
    Export,
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
    pub const fn new(
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
