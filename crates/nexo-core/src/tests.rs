use core::num::NonZeroU64;

use crate::{
    ActionOption, ActionStatus, FactualSupport, MAX_FACTUAL_SUPPORT, MAX_LEGAL_SUPPORT,
    MAX_UNMET_REQUIREMENTS, NodeId, NormativeClaimId, RequirementId,
};

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).expect("test IDs are non-zero"))
}

fn factual(value: u64) -> FactualSupport {
    FactualSupport::Observation(node(value))
}

fn legal(value: u64) -> NormativeClaimId {
    NormativeClaimId::new(node(value))
}

/// Invariant: a visible action must have factual support.
#[test]
fn rejects_an_action_without_factual_support() {
    let result = ActionOption::try_new(
        ActionStatus::InsufficientFacts,
        vec![],
        vec![legal(2)],
        vec![],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::MissingFactualSupport
    );
}

/// Invariant: a visible action must have legal support.
#[test]
fn rejects_an_action_without_legal_support() {
    let result = ActionOption::try_new(
        ActionStatus::InsufficientFacts,
        vec![factual(1)],
        vec![],
        vec![],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::MissingLegalSupport
    );
}

/// Invariant: unmet mandatory requirements make `SUPPORTED` illegal.
#[test]
fn rejects_supported_when_a_mandatory_requirement_is_missing() {
    let result = ActionOption::try_new(
        ActionStatus::Supported,
        vec![factual(1)],
        vec![legal(2)],
        vec![RequirementId::new(node(3))],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::SupportedWithUnmetRequirements
    );
}

/// Invariant: only a complete supported action is available.
#[test]
fn supported_action_with_complete_support_is_available() {
    let action = ActionOption::try_new(
        ActionStatus::Supported,
        vec![factual(1)],
        vec![legal(2)],
        vec![],
    )
    .expect("complete supported action is valid");

    assert!(action.is_available());
}

/// Invariant: partial factual knowledge can be displayed as a non-available
/// action whose missing requirements remain inspectable.
#[test]
fn insufficient_facts_is_not_available_and_preserves_missing_requirements() {
    let missing = RequirementId::new(node(3));
    let action = ActionOption::try_new(
        ActionStatus::InsufficientFacts,
        vec![factual(1)],
        vec![legal(2)],
        vec![missing],
    )
    .expect("an action may honestly expose incomplete requirements");

    assert!(!action.is_available());
    assert_eq!(action.unmet_requirements(), &[missing]);
}

/// Hostile-input bound: an adapter cannot turn one action into an unbounded
/// support list simply by passing a large decoded array to the core.
#[test]
fn rejects_factual_support_above_the_domain_limit() {
    let factual_support = (1..=(MAX_FACTUAL_SUPPORT as u64 + 1))
        .map(factual)
        .collect();

    let result = ActionOption::try_new(
        ActionStatus::InsufficientFacts,
        factual_support,
        vec![legal(2)],
        vec![],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::TooManyFactualSupport
    );
}

#[test]
fn rejects_legal_support_above_the_domain_limit() {
    let legal_support = (1..=(MAX_LEGAL_SUPPORT as u64 + 1)).map(legal).collect();

    let result = ActionOption::try_new(
        ActionStatus::InsufficientFacts,
        vec![factual(1)],
        legal_support,
        vec![],
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::TooManyLegalSupport
    );
}

#[test]
fn rejects_unmet_requirements_above_the_domain_limit() {
    let unmet_requirements = (1..=(MAX_UNMET_REQUIREMENTS as u64 + 1))
        .map(|value| RequirementId::new(node(value)))
        .collect();

    let result = ActionOption::try_new(
        ActionStatus::InsufficientFacts,
        vec![factual(1)],
        vec![legal(2)],
        unmet_requirements,
    );

    assert_eq!(
        result.unwrap_err(),
        crate::ActionOptionError::TooManyUnmetRequirements
    );
}
