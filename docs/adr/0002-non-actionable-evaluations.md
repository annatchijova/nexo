# ADR 0002: Non-actionable evaluations are proof-carrying sum types

## Status

Accepted for the first `nexo-core` layer.

## Context

The initial action-status enum placed `INSUFFICIENT_FACTS`,
`OUT_OF_JURISDICTION`, `POLICY_NOT_CURRENT`, and `ABSTAIN` beside actionable
states. The resulting `ActionOption` constructor required factual and legal
support for every status. Red Team Round 2 demonstrated legitimate outcomes
that this rejects: a stale policy can be identified before a case exists, and
an abstention can be justified precisely by the absence of authoritative legal
support.

The tempting replacement is an `EvaluationBlock` with optional factual,
policy, missing-requirement, and reason fields. That merely makes invalid
combinations representable: for example, `POLICY_NOT_CURRENT` without a policy
bundle or reference-time freshness evidence.

## Decision

The result type is closed:

```text
ActionEvaluation
├── Actionable(ActionOption)
└── NonActionable
    ├── InsufficientFacts
    ├── Contraindicated
    ├── OutOfJurisdiction
    ├── PolicyNotCurrent
    └── Abstain
```

Each non-actionable variant has a dedicated payload with private fields and a
validating constructor.

- `InsufficientFacts` requires legal route context and one or more named,
  missing requirements.
- `Contraindicated` requires factual context and legal grounds.
- `OutOfJurisdiction` requires jurisdiction evidence and policy-bundle
  identity.
- `PolicyNotCurrent` requires policy-bundle identity and freshness evidence
  evaluated at a reference time.
- `Abstain` carries one typed cause; it does not manufacture a legal claim to
  satisfy an actionable-route invariant.

`ActionOption` remains the only representation of a route that may be offered
to the person. It has only `SUPPORTED` and `CONDITIONALLY_SUPPORTED` states.

## Consequences

- The UI can explain both a proposed action and a refusal to propose one using
  an auditable graph path.
- Deterministic explanation rendering can regenerate text from either result
  without changing its epistemic status.
- Adding a future negative outcome requires declaring its proof obligation in
  the type system, tests, and policy contract.
- This core currently records typed IDs, not timestamps or source documents;
  the canonical-time and policy layers must validate those referenced nodes.

## Rejected alternatives

- Loosen factual/legal support requirements on `ActionOption`.
- A string status plus a generic map of evidence.
- A struct of optional fields selected by convention at each call site.
