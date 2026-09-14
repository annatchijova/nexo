# Pure action evaluator contract

## Purpose

The evaluator is the deterministic boundary that turns an authorized case
projection and a selected policy bundle into an `ActionEvaluation`. It does not
acquire sources, call models, read clocks, or perform external legal acts.

## Inputs

```text
evaluate(
  case_projection,
  selected_policy_bundle,
  route_definition,
  reference_date,
)
```

The caller must provide a bundle selected by `PolicyBundleSet::select`; the
evaluator never silently selects a different bundle. `reference_date` is
explicit and must be the same date used for bundle selection.

## Route definition

An `ActionRoute` is immutable policy data describing:

```text
action_id
required_claims          one or more NormativeClaimId references
mandatory_requirements  bounded RequirementId references
contraindications       bounded factual predicates + legal claim references
jurisdiction_scope       explicit jurisdiction code
```

It names a legal or rights route. It does not contain user prose, prompts,
model output, recipient addresses, credentials, or delivery instructions.

## Evaluation order

The evaluator proceeds in a fixed order:

1. Verify the selected bundle is current at `reference_date` and matches the
   route jurisdiction.
2. Resolve every required claim and every supporting source.
3. Reject absent, stale, out-of-scope, or ineligible normative support as a
   typed non-actionable result; never reinterpret it as missing case facts.
4. Project only `Artifact`, `Observation`, `UserAssertion`, and `DerivedFact`
   nodes into factual support. `Inference` is never admitted.
5. Evaluate mandatory requirements and contraindications deterministically.
6. Construct `ActionOption` only through its invariant-preserving constructor.

No LLM, prompt, source summary, or explanation text may enter this path.

## Result invariants

```text
SUPPORTED
  factual support non-empty
  legal support non-empty
  no unmet mandatory requirements

CONDITIONALLY_SUPPORTED
  factual support non-empty
  legal support non-empty
  at least one named unmet requirement

INSUFFICIENT_FACTS
  current, in-scope, source-complete legal route exists
  at least one mandatory factual requirement remains unproven

CONTRAINDICATED
  factual context and legal grounds both present

OUT_OF_JURISDICTION / POLICY_NOT_CURRENT / ABSTAIN
  typed non-actionable proof appropriate to the failure
```

Adding an `Inference` can change explanatory context only. It cannot change an
action from unsupported to supported, satisfy a mandatory factual requirement,
or erase a contraindication.

## Determinism and failure behavior

- Same inputs, regardless of vector insertion order, produce equal evaluation
  values and equal canonical projections.
- Missing or duplicate references fail closed before construction.
- Conflicting eligible claims produce `ABSTAIN`, never an arbitrary winner.
- A route that lacks legal support is not represented as an `ActionOption`.
- The evaluator returns no network request, filing, or delivery effect.

## Non-goals

- No universal legal interpretation or advice.
- No source acquisition, digest computation, persistence, or authorization.
- No natural-language generation. Explanation is a later deterministic view of
  the returned graph support.

## Falsification tests

The implementation must try to make an inference satisfy a requirement, make a
stale claim current, reorder inputs to change a result, omit legal support,
select a conflicting claim arbitrarily, and cross the preparation/external-act
boundary. Each attempt must fail closed with the typed result above.
