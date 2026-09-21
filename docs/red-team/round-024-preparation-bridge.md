# Security Audit — NEXO preparation bridge
## Red Team Round 024

**Date:** 2026-09-21  **Scope:** `nexo-api::preparation`, the durable receipt
binding query, and the `nexo-core` application bridge. No HTTP preparation or
export endpoint is enabled.

## Threat model

The attacker can influence an HTTP caller and any future adapter inputs, but
cannot modify the binary, migrations, or database role. The bridge must not
turn request-supplied identifiers into a preparation capability.

## Probes and results

| Probe | Result | Evidence |
| --- | --- | --- |
| Foreign actor presents a valid evaluation id | FALSIFIED | Integration test returns `CaseNotOwned` before minting. |
| Evaluation has no receipt | FALSIFIED BY QUERY SHAPE | The bridge requires an inner join to `evaluation_receipts`. |
| Receipt is non-actionable or not `SUPPORTED` | FALSIFIED BY INDUCTION | The bridge checks both durable result fields and `ActionOption::is_available()`. |
| Caller swaps case/evaluation identifiers | FALSIFIED BY QUERY SHAPE | The binding query scopes both identifiers and resolves route/digests from joined rows. |
| Caller supplies route or digest identifiers directly | FALSIFIED BY API SHAPE | The bridge derives them from the receipt/bundle join. |

## Residual

### RT-024-01 — Action payload substitution was an internal composition gap

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The bridge receives an `ActionOption` from the application composition and
now compares its canonical action fingerprint with the digest stored in the
receipt. A mismatched available action is rejected before minting.

The integration test obtains the action from the same evaluator/projection path
as the HTTP evaluation and verifies owner success plus foreign-owner failure.
The fingerprint is not accepted from the request; the API computes it during
evaluation and persists it in the receipt transaction.

## Exit decision

The bridge is safe to keep internal and unexposed for the current evaluation
slice. Preparation/export endpoints remain deferred pending deterministic
output generation and lifecycle persistence.

### RT-024-02 — Preparation rows could bypass receipt evidence

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The database preparation trigger originally checked only that the referenced
evaluation was `actionable`. A direct insert could therefore create a
preparation after deleting or never creating its evaluation receipt. The
adversarial repository test reproduced this path. The trigger now requires an
existing receipt and `SUPPORTED` status, and rolls back the probe.

### RT-024-03 — Preparation lifecycle could be rewound or deleted

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

Direct SQL could move an exported preparation back to `prepared` and delete
the row entirely. The lifecycle trigger now preserves identity fields, allows
only forward transitions or invalidation with a reason, and rejects deletes.
The repository probe covers both rewind and deletion attempts.

### RT-024-04 — Preparation output provenance could cross cases

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

A direct insert could attach output provenance from another case to a valid
supported evaluation. The preparation trigger now resolves both case IDs and
rejects that mismatch; the repository probe covers the cross-case insertion.

### RT-024-05 — Preparation records omitted binding identities

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The schema originally stored only the evaluation reference and lifecycle
metadata. It did not require the route, policy digest, input-manifest digest,
or generator version that the preparation contract requires. Those fields are
now mandatory, and the trigger compares route, bundle digest, and input
manifest against the receipt before insertion or update.

### RT-024-06 — Prepared rows could omit output evidence

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The preparation table allowed a `prepared` row with no output digest or
provenance. The output artifact contract now makes both references mandatory;
the object store remains responsible for proving that the bytes match the
digest before the row is created.

### RT-024-07 — Output digest could bypass content-addressed storage

**Level:** RESOLVED FOR APPLICATION PATH / RESIDUAL AT RAW REPOSITORY API

The low-level repository function accepts a digest row because it is a
storage-neutral persistence layer. The application helper now writes bytes
through `FilesystemObjectStore::put`, derives the SHA-256 digest internally,
and only then inserts the preparation in a transaction. No HTTP preparation
endpoint exposes the lower-level function; future adapters must use the
application helper rather than caller-supplied digest rows.
