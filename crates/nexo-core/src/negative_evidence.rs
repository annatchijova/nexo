//! Relational evidence for legal conflicts and route contraindications.

use crate::{
    FactualSupport, NodeId, NormativeClaimId, PolicyBundleId, PolicyVersion, RequirementId,
};

/// Identity of a deterministic policy rule. The id is not itself a witness;
/// the rule engine must mint the private match value below. A valid id can be
/// supplied by an adapter, but it cannot certify that the rule actually
/// matched.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyRuleId(NodeId);

impl PolicyRuleId {
    pub const fn new(value: NodeId) -> Self {
        Self(value)
    }
}

/// Auditable identity of the policy evaluation that produced a rule match.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRuleEvaluationRecord {
    bundle: PolicyBundleId,
    bundle_version: PolicyVersion,
    rule: PolicyRuleId,
}

/// Deterministic producer for negative-evidence matches. The configured
/// relations are the policy decision; callers can request a match but cannot
/// assert one for an unconfigured tuple.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyRuleEngine {
    record: PolicyRuleEvaluationRecord,
    conflict_pairs: Vec<(NormativeClaimId, NormativeClaimId)>,
    contraindications: Vec<(FactualSupport, NormativeClaimId, RequirementId)>,
}

impl PolicyRuleEngine {
    /// Constructed by the policy-bundle loader inside the core. Keeping this
    /// crate-private prevents an adapter from declaring an arbitrary relation
    /// table and then treating it as legal policy.
    #[allow(dead_code)]
    pub(crate) fn new(
        record: PolicyRuleEvaluationRecord,
        conflict_pairs: Vec<(NormativeClaimId, NormativeClaimId)>,
        contraindications: Vec<(FactualSupport, NormativeClaimId, RequirementId)>,
    ) -> Self {
        Self {
            record,
            conflict_pairs,
            contraindications,
        }
    }

    pub fn match_conflict(
        &self,
        left_claim: NormativeClaimId,
        right_claim: NormativeClaimId,
    ) -> Result<ConflictRuleMatch, NegativeEvidenceError> {
        if !self.conflict_pairs.iter().any(|(left, right)| {
            (*left == left_claim && *right == right_claim)
                || (*left == right_claim && *right == left_claim)
        }) {
            return Err(NegativeEvidenceError::RuleDidNotMatch);
        }
        ConflictRuleMatch::from_policy_engine(self.record.clone(), left_claim, right_claim)
    }

    pub fn match_contraindication(
        &self,
        factual_support: FactualSupport,
        legal_ground: NormativeClaimId,
        trigger: RequirementId,
    ) -> Result<ContraindicationRuleMatch, NegativeEvidenceError> {
        if !self
            .contraindications
            .contains(&(factual_support, legal_ground, trigger))
        {
            return Err(NegativeEvidenceError::RuleDidNotMatch);
        }
        Ok(ContraindicationRuleMatch::from_policy_engine(
            self.record.clone(),
            factual_support,
            legal_ground,
            trigger,
        ))
    }
}

impl PolicyRuleEvaluationRecord {
    pub fn new(bundle: PolicyBundleId, bundle_version: PolicyVersion, rule: PolicyRuleId) -> Self {
        Self {
            bundle,
            bundle_version,
            rule,
        }
    }
    pub const fn bundle(&self) -> PolicyBundleId {
        self.bundle
    }
    pub fn bundle_version(&self) -> &PolicyVersion {
        &self.bundle_version
    }
    pub const fn rule(&self) -> PolicyRuleId {
        self.rule
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictRuleMatch {
    record: PolicyRuleEvaluationRecord,
    left_claim: NormativeClaimId,
    right_claim: NormativeClaimId,
}

impl ConflictRuleMatch {
    /// Minted only by the in-crate policy rule engine. The engine's
    /// deterministic input tuple is `(rule, left_claim, right_claim)`; this
    /// constructor enforces only identity shape, not the legal truth of the
    /// rule. That semantic obligation remains with the engine and its policy
    /// bundle tests.
    #[allow(dead_code)] // consumed by the forthcoming policy rule engine
    pub(crate) fn from_policy_engine(
        record: PolicyRuleEvaluationRecord,
        left_claim: NormativeClaimId,
        right_claim: NormativeClaimId,
    ) -> Result<Self, NegativeEvidenceError> {
        if left_claim == right_claim {
            return Err(NegativeEvidenceError::SelfRelation);
        }
        Ok(Self {
            record,
            left_claim,
            right_claim,
        })
    }

    pub const fn record(&self) -> &PolicyRuleEvaluationRecord {
        &self.record
    }
    pub const fn left_claim(&self) -> NormativeClaimId {
        self.left_claim
    }
    pub const fn right_claim(&self) -> NormativeClaimId {
        self.right_claim
    }
}

/// A claim pair certified by a deterministic conflict rule.
///
/// Fields are private: external adapters cannot fabricate a witness by
/// assembling valid-looking ids. Minting is restricted to the in-crate policy
/// engine through `ConflictRuleMatch::from_policy_engine` and
/// `ConflictWitness::from_rule_match`.
///
/// ```compile_fail
/// use nexo_core::{ConflictWitness, NormativeClaimId, NodeId};
/// # use core::num::NonZeroU64;
/// let id = NormativeClaimId::new(NodeId::new(NonZeroU64::new(1).unwrap()));
/// let _ = ConflictWitness { left_claim: id, right_claim: id, rule_match: todo!() };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictWitness {
    left_claim: NormativeClaimId,
    right_claim: NormativeClaimId,
    rule_match: ConflictRuleMatch,
}

impl ConflictWitness {
    /// Bind a witness to the exact pair certified by its rule match. This
    /// rejects valid-but-unrelated ids and permits only pair reversal.
    #[allow(dead_code)] // consumed by the forthcoming policy rule engine
    pub(crate) fn from_rule_match(
        left_claim: NormativeClaimId,
        right_claim: NormativeClaimId,
        rule_match: ConflictRuleMatch,
    ) -> Result<Self, NegativeEvidenceError> {
        if left_claim == right_claim {
            return Err(NegativeEvidenceError::SelfRelation);
        }
        let same_order =
            rule_match.left_claim() == left_claim && rule_match.right_claim() == right_claim;
        let reverse_order =
            rule_match.left_claim() == right_claim && rule_match.right_claim() == left_claim;
        if !same_order && !reverse_order {
            return Err(NegativeEvidenceError::WitnessClaimsMismatch);
        }
        Ok(Self {
            left_claim,
            right_claim,
            rule_match,
        })
    }

    pub const fn left_claim(&self) -> NormativeClaimId {
        self.left_claim
    }
    pub const fn right_claim(&self) -> NormativeClaimId {
        self.right_claim
    }
    pub fn rule_match(&self) -> &ConflictRuleMatch {
        &self.rule_match
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContraindicationRuleMatch {
    record: PolicyRuleEvaluationRecord,
    factual_support: FactualSupport,
    legal_ground: NormativeClaimId,
    trigger: RequirementId,
}

impl ContraindicationRuleMatch {
    /// Minted only by the in-crate policy rule engine. Its deterministic input
    /// tuple is `(rule, factual_support, legal_ground, trigger)`; the type
    /// proves identity binding, while the engine must prove that the trigger
    /// actually matched the fact under the selected policy.
    #[allow(dead_code)] // consumed by the forthcoming policy rule engine
    pub(crate) const fn from_policy_engine(
        record: PolicyRuleEvaluationRecord,
        factual_support: FactualSupport,
        legal_ground: NormativeClaimId,
        trigger: RequirementId,
    ) -> Self {
        Self {
            record,
            factual_support,
            legal_ground,
            trigger,
        }
    }

    pub const fn record(&self) -> &PolicyRuleEvaluationRecord {
        &self.record
    }
    pub const fn factual_support(&self) -> FactualSupport {
        self.factual_support
    }
    pub const fn legal_ground(&self) -> NormativeClaimId {
        self.legal_ground
    }
    pub const fn trigger(&self) -> RequirementId {
        self.trigger
    }
}

/// A fact/ground/trigger relation certified by a deterministic policy rule.
/// The private field prevents callers from fabricating evidence without a
/// rule match produced by the policy engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContraindicationEvidence {
    rule_match: ContraindicationRuleMatch,
}

impl ContraindicationEvidence {
    #[allow(dead_code)] // consumed by the forthcoming policy rule engine
    pub(crate) const fn from_rule_match(rule_match: ContraindicationRuleMatch) -> Self {
        Self { rule_match }
    }

    pub fn rule_match(&self) -> &ContraindicationRuleMatch {
        &self.rule_match
    }
    pub const fn factual_support(&self) -> FactualSupport {
        self.rule_match.factual_support()
    }
    pub const fn legal_ground(&self) -> NormativeClaimId {
        self.rule_match.legal_ground()
    }
    pub const fn trigger(&self) -> RequirementId {
        self.rule_match.trigger()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NegativeEvidenceError {
    SelfRelation,
    WitnessClaimsMismatch,
    RuleDidNotMatch,
}
