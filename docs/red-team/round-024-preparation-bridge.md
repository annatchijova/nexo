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

### RT-024-08 — Preparations remained current after case inputs changed

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

After a preparation was persisted, inserting another case-graph node left its
status as `prepared`. The database now invalidates all preparations derived
from that case while preserving their output and invalidation reason. The API
integration test covers the transition.

### RT-024-09 — Preparations remained current after policy activation

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

Activating a newer bundle for the same jurisdiction left preparations derived
from the prior bundle in `prepared`. An activation trigger now invalidates
those materials while preserving their audit record; the repository probe
covers the policy replacement.

### RT-024-10 — Receipt removal could leave a preparation current

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

Deleting the receipt that justified a preparation removed the evidence while
leaving the material apparently current. The database now invalidates every
non-invalidated preparation tied to the deleted evaluation, with an explicit
reason. The preparation integrity trigger permits this fail-closed transition
without permitting inserts or ordinary updates that lack a supported receipt.
The API integration test covers receipt deletion and verifies that a later
case mutation does not revive the material.

### RT-024-11 — Preparation bindings and output identity could be rewritten

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The lifecycle guard previously froze only the evaluation, kind, and timestamp.
A direct writer could therefore rewrite route, policy/input bindings, generator
version, or output references after preparation. Those fields are now immutable
for the lifetime of the row, including after invalidation, preserving the
identity of the material that was actually produced.

### RT-024-12 — Receipt binding fields could be rewritten after issuance

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The receipt trigger checked that a new or updated receipt matched the
evaluation's case, route, policy, and result status, but it did not freeze the
receipt's digest bindings. A direct writer could replace the action or input
manifest digest after a preparation had consumed the receipt. Receipts are now
immutable on update; explicit deletion remains the only revocation path and
invalidates dependent preparations.

### RT-024-13 — Receipted evaluation result could be rewritten

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The evaluation row remained mutable after its receipt had been issued. A
writer could replace the sealed result payload or status while the receipt and
dependent preparation continued to refer to the old decision. Evaluations are
now immutable once a receipt exists; deletion of the receipt remains the
explicit revocation path and invalidates dependent preparations.

### RT-024-14 — Preparation could consume a stale input manifest

**Level:** PLAUSIBLE HYPOTHESIS → HARDENED; runtime induction deferred

The evaluation projection was read before the preparation transaction, while
the preparation bridge revalidated the receipt but not the current case graph.
An intervening graph mutation could therefore make the receipt's manifest stale
before material persistence. The bridge now locks the case row, recomputes the
manifest, and rejects persistence when it differs from the receipt. A public
preparation endpoint does not exist yet, so the concurrent external induction
remains deferred; the repository node writers all use the same case-row lock.
