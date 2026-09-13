# Security Audit — NEXO action-result type model

## Red Team Round 2

**Date:** 2026-09-13  
**Method:** semantic counterexamples + executable constructor probes  
**Base:** `main @ 786bc6757cd367edb25a8b4bc5884dcca468372b`  
**Scope:** whether `ActionStatus` values are all semantically valid members of `ActionOption`.

## Threat model

- A policy evaluator may run before a full case exists.
- A system must be able to state that it lacks a current policy or authoritative legal basis without falsely presenting a route as actionable.
- The UI may display an evaluation result and an actionable option in adjacent places, but must not be able to conflate them.

## Executive summary

| ID | Severity | Level | Bucket | Finding |
| --- | --- | --- | --- | --- |
| RT-003 | High | CONFIRMED BY INDUCTION | Architectural invariant | `ActionOption` conflates proof-carrying actionable routes with non-actionable evaluation results. |

## Status-by-status semantic test

| Status | Both factual + legal action support required? | Reason |
| --- | --- | --- |
| `Supported` | Yes | The system is saying a route is available now. |
| `ConditionallySupported` | Yes | The route has a basis, but a declared condition remains unresolved. |
| `InsufficientFacts` | Not universally | A case-specific candidate can have partial factual and legal context, but a pre-case eligibility prompt can exist before case facts. The latter is not an action option. |
| `Contraindicated` | Yes, for a case-specific route | The system needs facts and legal/policy grounds to explain why the route should not be taken. |
| `OutOfJurisdiction` | Not as an action-support pair | It may derive from selected jurisdiction metadata plus policy-bundle scope, before any case-specific normative claim exists. It is a scope evaluation. |
| `PolicyNotCurrent` | No factual support required | Policy freshness is evaluated from bundle metadata and a reference time; it can be known before a case exists. |
| `Abstain` | No legal action support required | The honest reason for abstention may be precisely that authoritative legal support is absent, conflicting, or unavailable. |

## RT-003 — `ActionOption` conflates two domains

**Severity:** High for epistemic correctness; no external exploit surface exists yet.  
**Epistemic level:** CONFIRMED BY INDUCTION.  
**Bucket:** Architectural invariant.

### Surprise

The constructor imposes non-empty factual and legal support on every
`ActionStatus`, while several statuses communicate that an action cannot yet
be justified or that no actionable option exists.

### Rival hypotheses

1. The statuses are all actionable options and the invariant is universally correct.
2. The statuses mix actionable options with evaluation results.
3. Missing legal support can be represented by a synthetic normative claim.

Hypothesis 3 was rejected conceptually: a synthetic claim would manufacture
the authority that `Abstain` must truthfully say is absent.

### Prediction

If hypothesis 2 is correct, the current constructor will reject legitimate
non-actionable results:

1. `Abstain` with factual case context but no authoritative legal claim.
2. `PolicyNotCurrent` from a stale policy bundle before case evidence exists.
3. `OutOfJurisdiction` from selected jurisdiction plus bundle scope before a
   case-specific normative claim exists.

### Induction

Three executable tests now construct exactly those inputs against the current
constructor. `cargo test --workspace` observed all three rejected with:

```text
Abstain + factual + no legal claim              → MissingLegalSupport
PolicyNotCurrent + legal context + no facts     → MissingFactualSupport
OutOfJurisdiction + facts + no legal claim     → MissingLegalSupport
```

The prediction held.

### Causal chain

```text
ActionStatus includes non-actionable outcomes
        ↓
ActionOption requires action support for every outcome
        ↓
honest absence/staleness/scope results cannot exist as typed values
        ↓
adapter is pressured to omit the result, mislabel it, or manufacture support
```

## Decision required before `nexo-integrity`

Do not weaken `ActionOption`. Preserve it as a proof-carrying actionable route
with non-empty factual and legal support. Introduce a separate evaluation
result type, for example:

```text
ActionEvaluation
├── Actionable(ActionOption)
└── NonActionable(EvaluationBlock)
    ├── InsufficientFacts
    ├── OutOfJurisdiction
    ├── PolicyNotCurrent
    └── Abstain
```

`EvaluationBlock` needs its own support semantics: factual context where
available, policy-bundle identity and scope/freshness evidence, explicit
missing requirements, and an abstention reason. It must not impersonate an
actionable legal claim.

`Contraindicated` should remain a case-specific `ActionOption` only if it has
both factual and legal/policy grounds. Otherwise it is an `Abstain` result.

## Discarded vector

| Vector | Result | Why it failed |
| --- | --- | --- |
| Weaken the global `ActionOption` invariant for negative statuses | Rejected | It would allow a visible route without its two explanation paths, erasing the central proof obligation. |
