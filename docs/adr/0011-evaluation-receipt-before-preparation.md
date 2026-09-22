# ADR 0011 — Require an application-owned evaluation receipt before preparation

Date: 2026-09-21   Status: accepted   Reversibility: one-way-ish (persisted
receipt fields and preparation bindings require migration once shipped)

## Forces at the time

- NEXO's preparation types already fail closed unless they receive a
  `VerifiedPreparationSnapshot`.
- The HTTP adapter now has a durable evaluation row, but the row is not yet a
  proof that the action identity, case, policy bundle, and input manifest are
  the same objects the evaluator saw.
- The deadline favors a narrow vertical slice, but a public preparation
  endpoint would make an authority boundary difficult to repair after clients
  depend on it.

## Decision

The application/API composition must produce an application-owned evaluation
receipt before it can mint or expose any preparation capability. The receipt
binds the exact evaluation, action route, owning case, immutable policy-bundle
digest, and canonical input-manifest digest. The detailed contract is in
`docs/EVALUATION_RECEIPT_CONTRACT.md`.

The producer now exists as an explicit application-owned bridge, but NEXO
still does not expose preparation or export endpoints until output generation
and lifecycle persistence are implemented.

## Alternatives rejected

- **Treat the `action_evaluations` row as sufficient** — rejected because its
  current payload records the decision but does not prove reproducible input
  lineage. Best argument: it is already durable and would be the smallest
  implementation.
- **Let `nexo-core` mint the snapshot directly** — rejected because the core
  cannot see database ownership, durable evaluation identity, or manifest
  lineage. Best argument: it would keep the capability close to the pure
  preparation types.
- **Accept public IDs and digests from the API caller** — rejected because
  co-presence is not binding evidence and would turn caller assertions into
  preparation authority. Best argument: it would enable a fast demo endpoint.

## Assumption this rests on

The application layer remains the only trusted composition point for the
authorized graph projection and its durable evaluation record. If another
adapter can independently mint equivalent receipts, its authority and
verification obligations must be added to the contract before it ships.

## Consequences

Accepted now: the application can mint a verified core snapshot only after
the durable receipt and ownership checks pass. Step 7 remains partially
deferred; no preparation demo endpoint exists yet.

Deferred: public preparation/export API and UI flows, destination policy, and
download semantics. The application now has canonical manifest construction,
output-artifact generation, and an owner-checked internal export bridge.

## Revisit trigger

Reopen when the application can produce and test all four bindings in the
evaluation receipt contract, or when a second policy/action route requires a
more expressive action identity than the current route row.

## Anchored at

- `crates/nexo-core/src/preparation.rs` — explicit application bridge into the
  verified snapshot type
  boundary.
- `docs/adr/0010-preparation-binding-ownership.md` — prior decision that
  deferred production minting until application evidence exists.
