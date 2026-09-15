# Security Audit — Round 015 evaluator negative-state preflight

**Date:** 2026-09-15  **Base:** `main @ 602abbf`

## Falsification before implementation

### RT-047 — evaluator currently cannot carry relational negatives

**CODE FACT.** `evaluate()` currently returns `ABSTAIN`, `INSUFFICIENT_FACTS`,
or an actionable result after claim eligibility. It has no policy-rule engine
input, so conflict and contraindication cannot be emitted end-to-end.

### RT-048 — distinct IDs are not a semantic relation

**FALSIFIED DESIGN ASSUMPTION.** Two eligible claim IDs, or a factual ID plus a
legal-ground ID, do not establish conflict or contraindication. The engine must
find the exact deterministic tuple before minting a match.

### RT-049 — precedence is observable behavior

**HYPOTHESIS TO TEST.** If conflict and contraindication both match, conflict
wins because claim selection is prior to case-specific route suitability. A
test must pin this branch; changing order changes the public evaluation result.

### RT-050 — public engine construction would reintroduce authority confusion

**FALSIFIED.** A public `PolicyRuleEngine::new` would let an adapter declare an
arbitrary relation table and then obtain typed negative evidence. The
constructor is crate-private; only the policy-bundle loading path inside
`nexo-core` may create the producer.

## Residual limits

`pub(crate)` permits any future module in `nexo-core` to call the producer. The
types guarantee relation binding and bundle/version recording, not the legal
correctness of the table itself. That semantic claim remains bounded by the
bundle loader's deterministic inputs and its source-backed policy tests.

## Exit criteria

- evaluator emits both variants only through relational evidence;
- missing relations fail closed without manufacturing a negative result;
- combined conflict/contraindication case follows tested precedence;
- public API cannot construct a policy engine or witness directly;
- tests, clippy, diff-check, and this red-team record pass.
