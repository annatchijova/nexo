use crate::*;
use core::num::NonZeroU64;

fn node(value: u64) -> NodeId {
    NodeId::new(NonZeroU64::new(value).unwrap())
}

#[test]
fn conflict_witness_requires_rule_match_for_the_same_claim_pair() {
    let left = NormativeClaimId::new(node(1));
    let right = NormativeClaimId::new(node(2));
    let unrelated = NormativeClaimId::new(node(3));
    let record = PolicyRuleEvaluationRecord::new(
        PolicyBundleId::new(node(9)),
        PolicyVersion::try_new("v1".into()).unwrap(),
        PolicyRuleId::new(node(4)),
    );
    let rule = ConflictRuleMatch::from_policy_engine(record, left, unrelated).unwrap();
    assert_eq!(
        ConflictWitness::from_rule_match(left, right, rule).unwrap_err(),
        NegativeEvidenceError::WitnessClaimsMismatch
    );
}

#[test]
fn conflict_witness_accepts_reverse_pair_but_rejects_self_relation() {
    let left = NormativeClaimId::new(node(1));
    let right = NormativeClaimId::new(node(2));
    let record = PolicyRuleEvaluationRecord::new(
        PolicyBundleId::new(node(9)),
        PolicyVersion::try_new("v1".into()).unwrap(),
        PolicyRuleId::new(node(3)),
    );
    let rule = ConflictRuleMatch::from_policy_engine(record, left, right).unwrap();
    assert!(ConflictWitness::from_rule_match(right, left, rule).is_ok());
    assert_eq!(
        ConflictRuleMatch::from_policy_engine(
            PolicyRuleEvaluationRecord::new(
                PolicyBundleId::new(node(9)),
                PolicyVersion::try_new("v1".into()).unwrap(),
                PolicyRuleId::new(node(4)),
            ),
            left,
            left,
        )
        .unwrap_err(),
        NegativeEvidenceError::SelfRelation
    );
}

#[test]
fn contraindication_evidence_keeps_fact_ground_and_trigger_together() {
    let rule = ContraindicationRuleMatch::from_policy_engine(
        PolicyRuleEvaluationRecord::new(
            PolicyBundleId::new(node(9)),
            PolicyVersion::try_new("v1".into()).unwrap(),
            PolicyRuleId::new(node(1)),
        ),
        FactualSupport::Observation(node(2)),
        NormativeClaimId::new(node(3)),
        RequirementId::new(node(4)),
    );
    let evidence = ContraindicationEvidence::from_rule_match(rule);
    assert_eq!(
        evidence.factual_support(),
        FactualSupport::Observation(node(2))
    );
    assert_eq!(evidence.legal_ground(), NormativeClaimId::new(node(3)));
    assert_eq!(evidence.trigger(), RequirementId::new(node(4)));
}

#[test]
fn policy_engine_is_the_only_supported_producer_of_relational_negative_results() {
    let left = NormativeClaimId::new(node(1));
    let right = NormativeClaimId::new(node(2));
    let fact = FactualSupport::Observation(node(3));
    let trigger = RequirementId::new(node(4));
    let engine = PolicyRuleEngine::new(
        PolicyRuleEvaluationRecord::new(
            PolicyBundleId::new(node(10)),
            PolicyVersion::try_new("v7".into()).unwrap(),
            PolicyRuleId::new(node(11)),
        ),
        vec![(left, right)],
        vec![(fact, left, trigger)],
    );

    let witness =
        ConflictWitness::from_rule_match(left, right, engine.match_conflict(left, right).unwrap())
            .unwrap();
    let conflict = ConflictingLegalClaims::from_witnesses(vec![fact], vec![witness]).unwrap();
    let result = ActionEvaluation::NonActionable(NonActionable::Abstain(
        Abstention::ConflictingLegalClaims(conflict.clone()),
    ));
    assert!(matches!(
        result,
        ActionEvaluation::NonActionable(NonActionable::Abstain(
            Abstention::ConflictingLegalClaims(_)
        ))
    ));
    assert_eq!(conflict.witnesses().len(), 1);
    assert_eq!(
        conflict.witnesses()[0].rule_match().record().bundle(),
        PolicyBundleId::new(node(10))
    );
    assert_eq!(
        conflict.witnesses()[0]
            .rule_match()
            .record()
            .bundle_version()
            .as_str(),
        "v7"
    );

    let evidence = ContraindicationEvidence::from_rule_match(
        engine.match_contraindication(fact, left, trigger).unwrap(),
    );
    let contraindicated = Contraindicated::from_evidence(vec![evidence]).unwrap();
    assert_eq!(contraindicated.evidence().len(), 1);
    assert_eq!(
        contraindicated.evidence()[0].rule_match().record().rule(),
        PolicyRuleId::new(node(11))
    );
    assert_eq!(
        engine.match_conflict(left, NormativeClaimId::new(node(99))),
        Err(NegativeEvidenceError::RuleDidNotMatch)
    );
}
