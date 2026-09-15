# ADR 0010: ownership of preparation binding

## Status

Accepted for the current layers; implementation deferred until the application
and evaluation-receipt boundary exists.

## Context

`PreparationPlan` needs four relations, not merely four fields:

```text
X identifies A
A belongs to S
S evaluated under P
S derived from I
```

The current evaluator returns an `ActionEvaluation` and uses a `PolicyBundle`,
but emits no durable action identity, evaluation snapshot, or input manifest
receipt. The integrity crate can seal values but does not own case or policy
semantics.

## Decision

Do not make the evaluator mint `VerifiedPreparationSnapshot` yet. Keep its
constructor crate-private and treat the type as a future consumer boundary.
Define the evaluation-receipt/application boundary and its binding evidence
first. The producing path may be one authority or a composition of authorities;
it must assemble sufficient evidence for all required relations and expose
exactly the verified inputs needed by preparation.

## Alternatives rejected

- **Evaluator-only minting:** it sees `ActionOption` and `PolicyBundle`, but not
  durable identity or manifest lineage.
- **Public snapshot constructor:** makes caller assertions look authoritative.
- **Integrity-only minting:** a digest proves byte identity, not action/snapshot
  or legal evaluation relationships.
- **Struct co-presence:** four fields together do not prove four edges.

## Consequences

Preparation remains safely conservative: no current production path can create
a verified snapshot from independently supplied references. The next layer must
define the owner or owners of the required binding evidence, persistence
semantics, and independent verification mechanism before enabling minting.
