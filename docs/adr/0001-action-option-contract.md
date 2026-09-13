# ADR 0001: Action options are proof-carrying domain values

## Status

Accepted for the first `nexo-core` layer.

## Threat model

An API client, an extractor, a policy bundle, or a future LLM may attempt to
cause an action to appear available without adequate evidence or legal support.
An implementation bug may also accidentally treat a missing mandatory
requirement as a recommendation.

This layer does not establish whether an artifact, observation, or normative
claim is valid. It establishes that an action cannot be constructed without
references to both support classes, and that `SUPPORTED` cannot coexist with a
known unmet mandatory requirement.

## Decision

`ActionOption::try_new` is the only constructor. It requires non-empty factual
support and legal support. `Inference` is not a `FactualSupport` variant. The
status is a closed Rust enum, and `is_available()` is true only for a valid,
complete `SUPPORTED` value.

The first red-team pass added a 64-reference cap independently to factual
support, legal support, and unmet requirements. This bounds retained domain
state; boundary decoders must still reject oversized bodies before allocation.

## Rejected alternatives

- A UI-only check: bypassable by every other consumer and invisible to the
  verifier.
- String states such as `"available"`: typo-prone, open-ended, and unable to
  express the illegal `SUPPORTED + missing requirement` combination.
- Treating an inference as a factual support variant: violates epistemic
  monotonicity by allowing a model interpretation to manufacture support.

## Falsification

This decision must be reconsidered if a legitimate action needs to be visible
without any factual or legal reference. That would require a distinct domain
type, not weakening `ActionOption`.
