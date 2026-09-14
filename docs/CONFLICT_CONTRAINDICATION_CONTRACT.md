# Conflicts and contraindications contract

## Purpose

This layer represents two different reasons not to offer a route:

- **Legal conflict:** eligible claims cannot be jointly selected and NEXO has
  no authorized rule for choosing one.
- **Contraindication:** a case fact matches a legal ground that makes this
  route unsuitable.

Neither state is a boolean or a pair of unrelated vectors. Each must carry the
relation that makes the negative conclusion intelligible and auditable.

## Threat model

An attacker can submit contradictory declarations, unrelated claim ids, fake
rule-match ids, duplicated evidence, and inference nodes presented as facts.
They cannot alter the compiled core or manufacture a valid graph reference to
an absent node.

## Evidence topology

```text
LegalConflictEvidence
  ├── left_claim ───────► NormativeClaim
  ├── right_claim ──────► NormativeClaim
  └── conflict_witness ─► deterministic policy analysis

ContraindicationEvidence
  ├── factual_support ──► Artifact | Observation | UserAssertion | DerivedFact
  ├── legal_ground ─────► NormativeClaim
  └── rule_match ───────► deterministic policy analysis
```

The witness/rule-match reference is not a prose explanation. It is an opaque,
versioned result of a deterministic policy rule evaluation whose inputs are
itself auditable. An `Inference` cannot occupy the factual edge.

## Invariants

- A conflict contains at least two distinct claim identities.
- A conflict witness names the exact claim pair; two claims merely coexisting
  is insufficient.
- A contraindication binds at least one factual-support node to one legal
  ground and one rule match. Unrelated factual and legal vectors are invalid.
- Every referenced claim is current, in-scope, source-complete, and eligible
  under the selected bundle before either negative state is constructed.
- Duplicate evidence and self-pairs are rejected.
- Adding an inference cannot create, erase, or upgrade either relation.
- A conflict or contraindication never becomes an `ActionOption`; it is a
  `NonActionable` result with its typed basis.

## Failure behavior

| Invalid shape | Required result |
| --- | --- |
| One claim presented as a conflict | reject construction |
| Two distinct but unrelated claims | reject: no conflict witness |
| Same claim on both sides | reject self-pair |
| Factual and legal vectors with no binding relation | reject construction |
| Inference used as factual support | reject construction |
| Stale/ineligible legal ground | `ABSTAIN` or `POLICY_NOT_CURRENT` before relation construction |
| Valid contraindication basis | `NonActionable::Contraindicated` |

## Non-goals

- No natural-language contradiction detection.
- No universal legal hierarchy or discretionary balancing.
- No automatic choice between conflicting claims.
- No external communication or legal act.

## Falsification tests before implementation

The current vector-based constructors must be shown to admit the unrelated
claim pair and unrelated fact/ground pair described above. The redesigned
constructors must make both shapes unrepresentable or reject them with typed
errors. Tests must also prove exact witness binding, inference exclusion, and
order-independent evidence serialization.
