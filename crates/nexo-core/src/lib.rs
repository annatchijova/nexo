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

/// A calendar date with no implied timezone or time of day.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CivilDateError {
    UnsupportedYear,
    InvalidMonth,
    InvalidDay,
}
impl CivilDate {
    pub fn try_new(year: i32, month: u8, day: u8) -> Result<Self, CivilDateError> {
        if !(1..=9999).contains(&year) {
            return Err(CivilDateError::UnsupportedYear);
        }
        if !(1..=12).contains(&month) {
            return Err(CivilDateError::InvalidMonth);
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let max = match month {
            2 if leap => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        if day == 0 || day > max {
            return Err(CivilDateError::InvalidDay);
        }
        Ok(Self { year, month, day })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidityInterval {
    effective_from: CivilDate,
    effective_to: Option<CivilDate>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidityIntervalError {
    EndBeforeStart,
}
impl ValidityInterval {
    /// A civil-date interval closed at both ends: `[effective_from, effective_to]`.
    /// `None` means an explicitly open-ended interval, not an unknown end.
    /// Unknown currency must fail closed in the policy layer as `POLICY_NOT_CURRENT`.
    pub fn try_new(
        effective_from: CivilDate,
        effective_to: Option<CivilDate>,
    ) -> Result<Self, ValidityIntervalError> {
        if effective_to.is_some_and(|end| end < effective_from) {
            return Err(ValidityIntervalError::EndBeforeStart);
        }
        Ok(Self {
            effective_from,
            effective_to,
        })
    }

    pub fn contains(&self, reference: CivilDate) -> bool {
        reference >= self.effective_from
            && match self.effective_to {
                Some(end) => reference <= end,
                None => true,
            }
    }
}
/// A UTC instant supplied by an adapter; the core never reads an ambient clock.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UtcInstant {
    seconds_since_unix_epoch: i64,
}
impl UtcInstant {
    pub const fn from_unix_seconds(seconds_since_unix_epoch: i64) -> Self {
        Self {
            seconds_since_unix_epoch,
        }
    }
}

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

/// The state of a proof-carrying route that may still become actionable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionStatus {
    Supported,
    ConditionallySupported,
}

/// Identity of the policy bundle evaluated for a jurisdiction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyBundleId(NodeId);

impl PolicyBundleId {
    pub const fn new(value: NodeId) -> Self {
        Self(value)
    }
}

/// Evidence that records how a policy bundle's validity was evaluated at a
/// reference time. The timestamp representation belongs to the future
/// canonical-time layer; this core records only the typed provenance reference.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyFreshnessEvidenceId(NodeId);

impl PolicyFreshnessEvidenceId {
    pub const fn new(value: NodeId) -> Self {
        Self(value)
    }
}

/// Evidence used to determine the jurisdiction relevant to an evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JurisdictionEvidenceId(NodeId);

impl JurisdictionEvidenceId {
    pub const fn new(value: NodeId) -> Self {
        Self(value)
    }
}

/// A result that is not a route the system may present as actionable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionEvaluation {
    Actionable(ActionOption),
    NonActionable(NonActionable),
}

/// Each variant carries only the evidence capable of explaining that negative
/// outcome. There is no optional bag of unrelated evidence fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NonActionable {
    InsufficientFacts(InsufficientFacts),
    Contraindicated(Contraindicated),
    OutOfJurisdiction(OutOfJurisdiction),
    PolicyNotCurrent(PolicyNotCurrent),
    Abstain(Abstention),
}

/// A known legal route whose mandatory factual predicates remain unproven.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsufficientFacts {
    legal_support: Vec<NormativeClaimId>,
    missing_requirements: Vec<RequirementId>,
}

impl InsufficientFacts {
    /// Constructs a negative result only when a legal route is known and at
    /// least one predicate for that route is explicitly missing.
    pub fn try_new(
        legal_support: Vec<NormativeClaimId>,
        missing_requirements: Vec<RequirementId>,
    ) -> Result<Self, EvaluationError> {
        if legal_support.len() > MAX_LEGAL_SUPPORT {
            return Err(EvaluationError::TooManyLegalContext);
        }
        if missing_requirements.len() > MAX_UNMET_REQUIREMENTS {
            return Err(EvaluationError::TooManyMissingRequirements);
        }
        if legal_support.is_empty() {
            return Err(EvaluationError::MissingLegalContext);
        }
        if missing_requirements.is_empty() {
            return Err(EvaluationError::MissingRequirement);
        }

        Ok(Self {
            legal_support,
            missing_requirements,
        })
    }

    pub fn legal_support(&self) -> &[NormativeClaimId] {
        &self.legal_support
    }

    pub fn missing_requirements(&self) -> &[RequirementId] {
        &self.missing_requirements
    }
}

/// A case-specific route that should not be taken, with factual and legal
/// grounds kept separate for inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contraindicated {
    factual_support: Vec<FactualSupport>,
    legal_support: Vec<NormativeClaimId>,
}

impl Contraindicated {
    /// Constructs a negative legal conclusion with both its case facts and
    /// the legal claim that makes the route unsuitable.
    pub fn try_new(
        factual_support: Vec<FactualSupport>,
        legal_support: Vec<NormativeClaimId>,
    ) -> Result<Self, EvaluationError> {
        if factual_support.len() > MAX_FACTUAL_SUPPORT {
            return Err(EvaluationError::TooManyFactualContext);
        }
        if legal_support.len() > MAX_LEGAL_SUPPORT {
            return Err(EvaluationError::TooManyLegalContext);
        }
        if factual_support.is_empty() {
            return Err(EvaluationError::MissingFactualContext);
        }
        if legal_support.is_empty() {
            return Err(EvaluationError::MissingLegalContext);
        }

        Ok(Self {
            factual_support,
            legal_support,
        })
    }

    pub fn factual_support(&self) -> &[FactualSupport] {
        &self.factual_support
    }

    pub fn legal_support(&self) -> &[NormativeClaimId] {
        &self.legal_support
    }
}

/// Policy-bundle scope excludes the jurisdiction established by the cited
/// jurisdiction evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutOfJurisdiction {
    jurisdiction_evidence: JurisdictionEvidenceId,
    policy_bundle: PolicyBundleId,
}

impl OutOfJurisdiction {
    /// The scope evidence must be a distinct graph node from the bundle whose
    /// scope it constrains; otherwise the bundle would certify itself.
    pub fn try_new(
        jurisdiction_evidence: JurisdictionEvidenceId,
        policy_bundle: PolicyBundleId,
    ) -> Result<Self, EvaluationError> {
        if jurisdiction_evidence.0 == policy_bundle.0 {
            return Err(EvaluationError::JurisdictionEvidenceAliasesPolicyBundle);
        }
        Ok(Self {
            jurisdiction_evidence,
            policy_bundle,
        })
    }

    pub const fn jurisdiction_evidence(&self) -> JurisdictionEvidenceId {
        self.jurisdiction_evidence
    }

    pub const fn policy_bundle(&self) -> PolicyBundleId {
        self.policy_bundle
    }
}

/// A selected policy bundle cannot be used because its validity could not be
/// established at the evaluated reference time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PolicyNotCurrent {
    policy_bundle: PolicyBundleId,
    freshness_evidence: PolicyFreshnessEvidenceId,
}

impl PolicyNotCurrent {
    /// Freshness evidence must be independent from the bundle it evaluates;
    /// otherwise a bundle could be represented as validating itself.
    pub fn try_new(
        policy_bundle: PolicyBundleId,
        freshness_evidence: PolicyFreshnessEvidenceId,
    ) -> Result<Self, EvaluationError> {
        if policy_bundle.0 == freshness_evidence.0 {
            return Err(EvaluationError::FreshnessEvidenceAliasesPolicyBundle);
        }
        Ok(Self {
            policy_bundle,
            freshness_evidence,
        })
    }

    pub const fn policy_bundle(&self) -> PolicyBundleId {
        self.policy_bundle
    }

    pub const fn freshness_evidence(&self) -> PolicyFreshnessEvidenceId {
        self.freshness_evidence
    }
}

/// An explicit refusal to offer an action. Each form makes the cause visible
/// without manufacturing an authority claim that does not exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Abstention {
    NoAuthoritativeLegalSource(NoAuthoritativeLegalSource),
    UnsupportedQuestion(UnsupportedQuestion),
    ConflictingLegalClaims(ConflictingLegalClaims),
}

/// The system has case context but no primary, authoritative legal source for
/// the question asked.
///
/// ```compile_fail
/// use nexo_core::NoAuthoritativeLegalSource;
///
/// let _ = NoAuthoritativeLegalSource {
///     factual_context: Vec::new(),
/// };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoAuthoritativeLegalSource {
    factual_context: Vec<FactualSupport>,
}

/// The question is outside NEXO's declared domain, rather than merely missing
/// a fact or a current policy bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsupportedQuestion {
    factual_context: Vec<FactualSupport>,
}

/// Multiple source-backed legal claims conflict and NEXO cannot choose one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictingLegalClaims {
    factual_context: Vec<FactualSupport>,
    conflicting_claims: Vec<NormativeClaimId>,
}

impl Abstention {
    pub fn no_authoritative_legal_source(
        factual_context: Vec<FactualSupport>,
    ) -> Result<Self, EvaluationError> {
        Ok(Self::NoAuthoritativeLegalSource(
            NoAuthoritativeLegalSource::try_new(factual_context)?,
        ))
    }

    pub fn unsupported_question(
        factual_context: Vec<FactualSupport>,
    ) -> Result<Self, EvaluationError> {
        Ok(Self::UnsupportedQuestion(UnsupportedQuestion::try_new(
            factual_context,
        )?))
    }

    pub fn conflicting_legal_claims(
        factual_context: Vec<FactualSupport>,
        conflicting_claims: Vec<NormativeClaimId>,
    ) -> Result<Self, EvaluationError> {
        Ok(Self::ConflictingLegalClaims(
            ConflictingLegalClaims::try_new(factual_context, conflicting_claims)?,
        ))
    }
}

impl NoAuthoritativeLegalSource {
    pub fn try_new(factual_context: Vec<FactualSupport>) -> Result<Self, EvaluationError> {
        validate_factual_context(&factual_context)?;
        Ok(Self { factual_context })
    }

    pub fn factual_context(&self) -> &[FactualSupport] {
        &self.factual_context
    }
}

impl UnsupportedQuestion {
    pub fn try_new(factual_context: Vec<FactualSupport>) -> Result<Self, EvaluationError> {
        validate_factual_context(&factual_context)?;
        Ok(Self { factual_context })
    }

    pub fn factual_context(&self) -> &[FactualSupport] {
        &self.factual_context
    }
}

impl ConflictingLegalClaims {
    pub fn try_new(
        factual_context: Vec<FactualSupport>,
        conflicting_claims: Vec<NormativeClaimId>,
    ) -> Result<Self, EvaluationError> {
        validate_factual_context(&factual_context)?;
        if conflicting_claims.len() > MAX_LEGAL_SUPPORT {
            return Err(EvaluationError::TooManyLegalContext);
        }
        if conflicting_claims.len() < 2 {
            return Err(EvaluationError::InsufficientConflictingLegalClaims);
        }
        if has_duplicate(&conflicting_claims) {
            return Err(EvaluationError::DuplicateConflictingLegalClaim);
        }
        Ok(Self {
            factual_context,
            conflicting_claims,
        })
    }

    pub fn factual_context(&self) -> &[FactualSupport] {
        &self.factual_context
    }

    pub fn conflicting_claims(&self) -> &[NormativeClaimId] {
        &self.conflicting_claims
    }
}

fn validate_factual_context(factual_context: &[FactualSupport]) -> Result<(), EvaluationError> {
    if factual_context.len() > MAX_FACTUAL_SUPPORT {
        return Err(EvaluationError::TooManyFactualContext);
    }
    if factual_context.is_empty() {
        return Err(EvaluationError::MissingFactualContext);
    }
    Ok(())
}

fn has_duplicate<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[index + 1..].contains(value))
}

/// Why a non-actionable evaluation payload could not be constructed safely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationError {
    MissingFactualContext,
    MissingLegalContext,
    MissingRequirement,
    InsufficientConflictingLegalClaims,
    DuplicateConflictingLegalClaim,
    TooManyFactualContext,
    TooManyLegalContext,
    TooManyMissingRequirements,
    JurisdictionEvidenceAliasesPolicyBundle,
    FreshnessEvidenceAliasesPolicyBundle,
}

/// Why an action could not be constructed as a valid graph value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionOptionError {
    MissingFactualSupport,
    MissingLegalSupport,
    SupportedWithUnmetRequirements,
    ConditionallySupportedWithoutUnmetRequirements,
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
        if status == ActionStatus::ConditionallySupported && unmet_requirements.is_empty() {
            return Err(ActionOptionError::ConditionallySupportedWithoutUnmetRequirements);
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

#[cfg(test)]
mod evaluation_tests;

#[cfg(test)]
mod property_tests;

#[cfg(test)]
mod time_tests;
