use crate::*;
use core::num::NonZeroU64;

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).unwrap())
}
fn digest(value: u64) -> DigestId {
    DigestId::new(node(value))
}
fn artifact(value: u64) -> ArtifactId {
    ArtifactId::new(node(value))
}
fn generator() -> ToolVersion {
    ToolVersion::new(
        ToolId::new(NonZeroU64::new(7).unwrap()),
        NonZeroU64::new(1).unwrap(),
    )
}

fn plan() -> PreparationPlan {
    PreparationPlan::new(
        node(1),
        node(2),
        digest(3),
        digest(4),
        PreparationKind::DraftRequest,
        generator(),
    )
}

#[test]
fn lifecycle_keeps_output_only_after_generation_and_export_is_not_external_act() {
    let prepared = plan().prepare(artifact(5), digest(6));
    assert_eq!(prepared.plan().action_option(), node(1));
    let exported = prepared.export();
    assert_eq!(exported.material().output_digest(), digest(6));
}

#[test]
fn invalidation_preserves_material_and_reason_without_a_send_state() {
    let invalidated = plan()
        .prepare(artifact(5), digest(6))
        .invalidate(InvalidationReason::PolicyChanged);
    assert_eq!(invalidated.reason(), InvalidationReason::PolicyChanged);
    assert_eq!(
        invalidated.material().plan().policy_bundle_digest(),
        digest(3)
    );
}

#[test]
fn exported_material_can_be_invalidated_but_cannot_become_an_external_action() {
    let invalidated = plan()
        .prepare(artifact(5), digest(6))
        .export()
        .invalidate(InvalidationReason::RouteNoLongerSupported);
    assert_eq!(
        invalidated.reason(),
        InvalidationReason::RouteNoLongerSupported
    );
}
