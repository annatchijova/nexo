# ADR 0006: immutable policy-bundle selection boundary

## Status

Proposed for the policy layer after the case-graph and integrity foundations.

## Decision

Represent legal policy as immutable, versioned `PolicyBundle` values. A pure
selector receives an explicit jurisdiction and reference date and returns a
typed selection result. It does not read a clock, fetch sources, or evaluate
case facts.

Schema version and legal policy version remain separate. Unknown serialized
schemas fail closed; known historical schemas may migrate forward while
preserving their original identity and digest. Source authority and acquisition
channel remain eligibility dimensions, not universal legal weight.

## Alternatives rejected

- **One mutable “current policy” record:** loses historical reproducibility and
  allows an old evaluation to change meaning after activation.
- **Choose the newest bundle in vector order:** makes selection depend on
  insertion order or wall-clock arrival and hides ambiguity.
- **Let the evaluator fetch and parse sources:** collapses acquisition and
  decision authority and makes the result non-deterministic.
- **Treat unknown schemas as the current shape:** silently invents fields or
  defaults and can convert degraded data into apparent authority.

## Falsification evidence

The implementation must include order-independence, interval-boundary,
unknown-schema, digest-mismatch, duplicate-reference, and ambiguous-selection
tests. A red-team round must attempt to turn stale, secondary, or model-provided
material into an eligible legal source.
