//! NEXO's dependency-free domain kernel.
//!
//! This crate accepts no HTTP, database, filesystem, clock, or model authority.
//! It represents only claims and the invariants required before an action can be
//! presented as supported.

use core::num::NonZeroU64;

/// Maximum references retained in each support collection for one action.
/// A future adapter must enforce lower byte/body limits before allocating its
/// decoded input; these limits bound retained domain state.
pub const MAX_FACTUAL_SUPPORT: usize = 64;
pub const MAX_LEGAL_SUPPORT: usize = 64;
pub const MAX_UNMET_REQUIREMENTS: usize = 64;

/// An opaque reference to a durable node in a case graph.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(NonZeroU64);

impl NodeId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// A node kind that can establish factual support for an action.
///
/// `Inference` is intentionally absent. An interpretation may be displayed as
/// context, but it cannot become factual support merely by being generated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactualSupport {
    Artifact(NodeId),
    Observation(NodeId),
    UserAssertion(NodeId),
    DerivedFact(NodeId),
}

/// A source-backed legal or policy claim selected by a policy bundle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormativeClaimId(NodeId);

impl NormativeClaimId {
    pub const fn new(value: NodeId) -> Self {
        Self(value)
    }
}

/// A required condition which may be satisfied or remain explicitly missing.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequirementId(NodeId);

impl RequirementId {
    pub const fn new(value: NodeId) -> Self {
        Self(value)
    }
}

/// The epistemic/legal state of an action option.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionStatus {
    Supported,
    ConditionallySupported,
    InsufficientFacts,
    Contraindicated,
    OutOfJurisdiction,
    PolicyNotCurrent,
    Abstain,
}

/// Why an action could not be constructed as a valid graph value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionOptionError {
    MissingFactualSupport,
    MissingLegalSupport,
    SupportedWithUnmetRequirements,
    TooManyFactualSupport,
    TooManyLegalSupport,
    TooManyUnmetRequirements,
}

/// An action accompanied by the support required to explain it.
///
/// There is deliberately no public field construction. Callers crossing a
/// trust boundary must use [`ActionOption::try_new`], so the same invariants
/// apply to an HTTP adapter, a CLI import, and a future policy evaluator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionOption {
    status: ActionStatus,
    factual_support: Vec<FactualSupport>,
    legal_support: Vec<NormativeClaimId>,
    unmet_requirements: Vec<RequirementId>,
}

impl ActionOption {
    /// Constructs an action only when it has both factual and legal support.
    ///
    /// `Supported` is reserved for a complete action. Other states may expose
    /// outstanding requirements, enabling the UI to say what remains unknown
    /// without presenting the route as available.
    pub fn try_new(
        status: ActionStatus,
        factual_support: Vec<FactualSupport>,
        legal_support: Vec<NormativeClaimId>,
        unmet_requirements: Vec<RequirementId>,
    ) -> Result<Self, ActionOptionError> {
        if factual_support.len() > MAX_FACTUAL_SUPPORT {
            return Err(ActionOptionError::TooManyFactualSupport);
        }
        if legal_support.len() > MAX_LEGAL_SUPPORT {
            return Err(ActionOptionError::TooManyLegalSupport);
        }
        if unmet_requirements.len() > MAX_UNMET_REQUIREMENTS {
            return Err(ActionOptionError::TooManyUnmetRequirements);
        }
        if factual_support.is_empty() {
            return Err(ActionOptionError::MissingFactualSupport);
        }
        if legal_support.is_empty() {
            return Err(ActionOptionError::MissingLegalSupport);
        }
        if status == ActionStatus::Supported && !unmet_requirements.is_empty() {
            return Err(ActionOptionError::SupportedWithUnmetRequirements);
        }

        Ok(Self {
            status,
            factual_support,
            legal_support,
            unmet_requirements,
        })
    }

    pub const fn status(&self) -> ActionStatus {
        self.status
    }

    pub fn factual_support(&self) -> &[FactualSupport] {
        &self.factual_support
    }

    pub fn legal_support(&self) -> &[NormativeClaimId] {
        &self.legal_support
    }

    pub fn unmet_requirements(&self) -> &[RequirementId] {
        &self.unmet_requirements
    }

    /// Returns whether an action may be represented as available to act on.
    pub fn is_available(&self) -> bool {
        self.status == ActionStatus::Supported && self.unmet_requirements.is_empty()
    }
}

#[cfg(test)]
mod tests;
