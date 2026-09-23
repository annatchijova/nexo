# Audit chain v1 contract (legacy)

This document describes the unauthenticated v1 format retained only for
historical reference. New deployments use the authenticated v2 contract in
`docs/AUDIT_CHAIN_V2_HMAC_CONTRACT.md`; v1 rows are never silently reinterpreted
as v2.

NEXO's `audit_chain/v1` records selected mutating application events. It is a
tamper-evident integrity mechanism for the declared event history; it is not a
proof that an event was true, that an actor was honest, or that local storage
was never rewritten by a privileged database operator.

## Authoritative objects

The authoritative state is the application state mutation and its audit event
committed in the same PostgreSQL transaction. `audit_chains` stores the chain
identity, explicit zero genesis, current sequence, and current tip.
`audit_log` stores the full event projection. `audit_checkpoints` stores an
append-only database checkpoint for each committed sequence. Derived
explanations and ordinary read history are not automatically audit events.

## Canonical event and digest

Version 1 uses SHA-256 over canonical typed JSON binding `chain_id`,
`chain_version`, `sequence`, `previous_digest`, and the complete event value
(`schema_version`, actor/case identifiers, event ID and kind, payload,
timestamp, and provenance references). Canonical JSON rejects floating-point
values. The genesis digest is 32 zero bytes. Legacy rows are not silently
interpreted as v1.

## Transaction and append semantics

The application locks the `mutations` chain row, allocates the next sequence,
inserts the event, advances the chain tip, and inserts the checkpoint in the
caller's transaction. If the state mutation rolls back, its event and
checkpoint roll back too. Database triggers reject update/delete of audit rows
and checkpoints, and reject direct audit inserts that do not extend the
current chain tip with the expected sequence and event ID.

This applies only where the mutation uses the audited application chokepoint.
A direct SQL writer or an uninstrumented internal mutation is not made
auditable by the existence of this schema.

## Independent export verification

`nexo-app` materializes `audit-export-v1`. `nexo-verifier` verifies it without
loading the application repository or PostgreSQL:

```text
nexo-verify audit audit-export.json
```

The verifier reports integrity, linkage, sequence, genesis, checkpoint, and
complete-history properties separately. A valid export proves consistency of
the supplied bytes with the v1 chain rules and checkpoint. It does not prove
that the export is the only history that ever existed.

## Threat-model boundary

This version detects accidental or ordinary in-place changes, insertion,
deletion, duplication, reordering, broken links, and tail omission when the
retained checkpoint/export is not rewritten with the log. A privileged actor
who can rewrite the database, checkpoint, and every dependent digest can
recompute a self-consistent history. HMAC key custody and independent
external anchoring are intentionally out of scope.

Local `occurred_at` values are application/database timestamps, not independent
temporal witnesses. A provenance reference identifies a declared source; it
does not turn a derived fact or explanation into an original artifact.
