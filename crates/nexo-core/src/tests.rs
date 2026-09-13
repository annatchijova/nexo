use core::num::NonZeroU64;

use crate::{ActionOption, ActionStatus, FactualSupport, NodeId, NormativeClaimId, RequirementId};

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

/// Mutation target: replacing `is_available` with `status == Supported` must fail
/// this test because a constructor can never create that illegal combination.
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
