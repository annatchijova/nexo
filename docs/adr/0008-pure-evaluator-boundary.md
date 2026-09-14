# ADR 0008: keep action evaluation pure and proof-carrying

## Status

Proposed for the evaluator layer after policy-bundle selection and capture
attestation.

## Decision

The evaluator accepts an already selected, verified policy bundle, an authorized
case projection, a route definition, and an explicit reference date. It performs
no I/O and returns the existing closed `ActionEvaluation` sum type. Every
actionable result is constructed through `ActionOption::try_new`; every
non-actionable result carries only the typed proof appropriate to its failure.

## Alternatives rejected

- **Let the evaluator select or fetch policy:** introduces hidden time/network
  authority and makes historical results non-reproducible.
- **Pass an `EvaluationBlock` full of optional fields:** permits semantically
  absurd combinations and repeats the failure mode already removed from
  `ActionEvaluation`.
- **Let an LLM decide factual satisfaction:** allows an inference or generated
  explanation to launder itself into factual support.

## Falsification evidence

The design is wrong if insertion order changes a result, inference-only context
creates an actionable option, a stale/ambiguous bundle yields `SUPPORTED`, or a
returned preparation can silently perform an external legal act.
