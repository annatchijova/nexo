# Round 015 — evaluator negative-state integration

## Goal

The evaluator may emit `ConflictingLegalClaims` or `Contraindicated` only after
the policy bundle and every route claim pass the existing jurisdiction,
freshness, and source-eligibility gates. Neither result is a flag and neither
is inferred from prose.

## Evaluation order

```text
bundle/route scope
  → bundle validity
  → claim eligibility and source authority
  → configured conflict relations
  → configured contraindication relations
  → unmet route requirements
  → actionable result
```

The first failed gate wins. A conflict and a contraindication that both match
are therefore not silently combined: the documented order makes the result
deterministic and auditable. Conflict is evaluated first because it concerns
whether the legal claims can be jointly selected; contraindication then tests
case-specific facts against an otherwise eligible route.

## Required inputs

- `PolicyRuleEngine` is constructed inside the policy layer from an immutable,
  versioned bundle and deterministic relation tables.
- Conflict matching consumes the exact route claim pair.
- Contraindication matching consumes the exact projected factual-support node,
  legal ground, and trigger requirement.
- Every produced match carries bundle identity, bundle version, and rule id.

## Invariants

1. No public evaluator path can produce either negative variant without a
   `ConflictWitness` or `ContraindicationEvidence`.
2. An adapter-supplied rule id or relation table cannot become policy authority.
3. An ineligible, stale, or out-of-scope claim cannot be wrapped in negative
   evidence; the earlier gate returns `ABSTAIN`, `POLICY_NOT_CURRENT`, or
   `OUT_OF_JURISDICTION`.
4. A missing relation is not a conflict/contraindication; evaluation continues
   to the next gate.
5. The engine proves configured deterministic relation membership only. It does
   not claim universal legal truth, hierarchy, or discretionary balancing.

## Falsification targets

- Current evaluator has no negative-engine input and cannot emit these variants.
- A pair of distinct claims without an engine relation must not become a
  conflict.
- A fact and legal ground that are individually valid but absent from the
  engine relation table must not become a contraindication.
- When both relations match, the result must follow the declared precedence.
- An engine built outside the policy layer must be unconstructable through the
  public API.
