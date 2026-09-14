//! Pure, fail-closed evaluation of a rights route against a selected bundle.

use crate::{
    Abstention, ActionEvaluation, ActionOption, ActionStatus, FactualSupport, InsufficientFacts,
    JurisdictionCode, JurisdictionEvidenceId, MAX_FACTUAL_SUPPORT, MAX_LEGAL_SUPPORT,
    MAX_UNMET_REQUIREMENTS, NormativeClaim, NormativeClaimId, NormativeSource, OutOfJurisdiction,
    PolicyBundle, PolicyFreshnessEvidenceId, PolicyNotCurrent, RequirementId, ValidityInterval,
};

pub const MAX_ROUTE_REQUIREMENTS: usize = MAX_UNMET_REQUIREMENTS;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionRoute {
    action_id: crate::NodeId,
    jurisdiction: JurisdictionCode,
    required_claims: Vec<NormativeClaimId>,
    mandatory_requirements: Vec<RequirementId>,
}

impl ActionRoute {
    pub fn try_new(
        action_id: crate::NodeId,
        jurisdiction: JurisdictionCode,
        required_claims: Vec<NormativeClaimId>,
        mandatory_requirements: Vec<RequirementId>,
    ) -> Result<Self, RouteError> {
        if required_claims.is_empty() {
            return Err(RouteError::MissingClaim);
        }
        if required_claims.len() > MAX_LEGAL_SUPPORT {
            return Err(RouteError::TooManyClaims);
        }
        if mandatory_requirements.len() > MAX_ROUTE_REQUIREMENTS {
            return Err(RouteError::TooManyRequirements);
        }
        if has_duplicate(&required_claims) || has_duplicate(&mandatory_requirements) {
            return Err(RouteError::DuplicateReference);
        }
        Ok(Self {
            action_id,
            jurisdiction,
            required_claims,
            mandatory_requirements,
        })
    }

    pub const fn action_id(&self) -> crate::NodeId {
        self.action_id
    }
    pub const fn jurisdiction(&self) -> JurisdictionCode {
        self.jurisdiction
    }
    pub fn required_claims(&self) -> &[NormativeClaimId] {
        &self.required_claims
    }
    pub fn mandatory_requirements(&self) -> &[RequirementId] {
        &self.mandatory_requirements
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteError {
    MissingClaim,
    TooManyClaims,
    TooManyRequirements,
    DuplicateReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseProjection {
    factual_support: Vec<FactualSupport>,
    satisfied_requirements: Vec<RequirementId>,
    jurisdiction_evidence: JurisdictionEvidenceId,
    freshness_evidence: PolicyFreshnessEvidenceId,
}

impl CaseProjection {
    pub fn try_new(
        factual_support: Vec<FactualSupport>,
        satisfied_requirements: Vec<RequirementId>,
        jurisdiction_evidence: JurisdictionEvidenceId,
        freshness_evidence: PolicyFreshnessEvidenceId,
    ) -> Result<Self, ProjectionError> {
        if factual_support.is_empty() {
            return Err(ProjectionError::MissingFactualSupport);
        }
        if factual_support.len() > MAX_FACTUAL_SUPPORT {
            return Err(ProjectionError::TooManyFactualSupport);
        }
        if satisfied_requirements.len() > MAX_UNMET_REQUIREMENTS {
            return Err(ProjectionError::TooManySatisfiedRequirements);
        }
        if has_duplicate(&satisfied_requirements) {
            return Err(ProjectionError::DuplicateRequirement);
        }
        Ok(Self {
            factual_support,
            satisfied_requirements,
            jurisdiction_evidence,
            freshness_evidence,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    MissingFactualSupport,
    TooManyFactualSupport,
    TooManySatisfiedRequirements,
    DuplicateRequirement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormativeContext {
    claims: Vec<NormativeClaim>,
    sources: Vec<NormativeSource>,
}

impl NormativeContext {
    pub fn try_new(
        claims: Vec<NormativeClaim>,
        sources: Vec<NormativeSource>,
    ) -> Result<Self, ContextError> {
        if claims.len() > MAX_LEGAL_SUPPORT || sources.len() > MAX_LEGAL_SUPPORT {
            return Err(ContextError::TooManyEntries);
        }
        if has_duplicate_by(&claims, NormativeClaim::id)
            || has_duplicate_by(&sources, NormativeSource::id)
        {
            return Err(ContextError::DuplicateIdentity);
        }
        Ok(Self { claims, sources })
    }

    fn claim_is_eligible(
        &self,
        id: NormativeClaimId,
        bundle: &PolicyBundle,
        jurisdiction: JurisdictionCode,
        reference_date: crate::CivilDate,
    ) -> bool {
        let Some(claim) = self.claims.iter().find(|claim| claim.id() == id) else {
            return false;
        };
        if claim.policy_bundle() != bundle.id()
            || !claim.validity().contains(reference_date)
            || claim.jurisdiction().as_str() != jurisdiction.code()
        {
            return false;
        }
        claim.sources().iter().all(|support| {
            self.sources
                .iter()
                .find(|source| source.id() == support.source())
                .is_some_and(|source| bundle.source_is_eligible(source))
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextError {
    TooManyEntries,
    DuplicateIdentity,
}

pub fn evaluate(
    projection: &CaseProjection,
    bundle: &PolicyBundle,
    context: &NormativeContext,
    route: &ActionRoute,
    reference_date: crate::CivilDate,
) -> ActionEvaluation {
    if bundle.jurisdiction() != route.jurisdiction() {
        return ActionEvaluation::NonActionable(NonActionable::OutOfJurisdiction(
            OutOfJurisdiction::try_new(projection.jurisdiction_evidence, bundle.id())
                .expect("projection evidence must not alias bundle identity"),
        ));
    }
    if !bundle.validity().contains(reference_date) {
        return ActionEvaluation::NonActionable(NonActionable::PolicyNotCurrent(
            PolicyNotCurrent::try_new(bundle.id(), projection.freshness_evidence)
                .expect("freshness evidence must not alias bundle identity"),
        ));
    }
    if !route.required_claims().iter().all(|id| {
        bundle.claims().contains(id)
            && context.claim_is_eligible(*id, bundle, route.jurisdiction(), reference_date)
    }) {
        return ActionEvaluation::NonActionable(NonActionable::Abstain(
            Abstention::no_authoritative_legal_source(projection.factual_support.clone())
                .expect("projection guarantees factual support"),
        ));
    }
    let missing: Vec<RequirementId> = route
        .mandatory_requirements()
        .iter()
        .copied()
        .filter(|id| !projection.satisfied_requirements.contains(id))
        .collect();
    if !missing.is_empty() {
        return ActionEvaluation::NonActionable(NonActionable::InsufficientFacts(
            InsufficientFacts::try_new(route.required_claims.clone(), missing)
                .expect("route and projection are bounded and non-empty"),
        ));
    }
    ActionEvaluation::Actionable(
        ActionOption::try_new(
            ActionStatus::Supported,
            projection.factual_support.clone(),
            route.required_claims.clone(),
            Vec::new(),
        )
        .expect("route and projection satisfy ActionOption invariants"),
    )
}

use crate::NonActionable;

fn has_duplicate<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[index + 1..].contains(value))
}

fn has_duplicate_by<T, K: PartialEq>(values: &[T], key: impl Fn(&T) -> K) -> bool {
    values.iter().enumerate().any(|(index, value)| {
        values[index + 1..]
            .iter()
            .any(|other| key(other) == key(value))
    })
}

#[allow(dead_code)]
fn _validity_type_is_used(_: ValidityInterval) {}
