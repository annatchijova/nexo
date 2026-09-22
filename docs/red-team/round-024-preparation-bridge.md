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

### RT-024-15 — Process restart duplicated the active policy bundle

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The startup seed path inserted and activated a new copy of the same AR policy
bundle on every process start. Because policy activation intentionally
invalidates preparations for that jurisdiction, a restart could make unchanged
materials stale. Startup seeding now serializes on an advisory transaction lock
and returns the existing activated bundle/route for the exact fixture identity;
the API test invokes seeding twice and asserts identical durable IDs.

### RT-024-16 — Preparation retries could duplicate current materials

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The first HTTP preparation path inserted a new row on every identical retry.
The bridge now locks the case before checking the current manifest and returns
the existing non-invalidated preparation for the same evaluation, kind, and
generator version. The API test repeats the request and asserts the same
preparation id.

### RT-024-19 — Retry response could downgrade exported state

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The endpoint always serialized `status: prepared`, even when an idempotent
retry returned an already-exported row. The response now reads the durable
state and preserves `exported`; the API test marks the first row exported before
retrying and asserts that state is returned.

### RT-024-18 — Replaying the same policy activation invalidated materials

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The activation repository function inserted a second activation row even when
the exact bundle was already active. That replay fired the policy invalidation
trigger and could stale unchanged preparations. Re-activating an already
activated bundle is now an idempotent no-op; the seed test asserts exactly one
activation after two startup calls.

### RT-024-17 — Policy activation could race preparation persistence

**Level:** PLAUSIBLE HYPOTHESIS → HARDENED; concurrent induction deferred

The activation trigger invalidated preparations that already existed, but a
concurrent preparation could pass its receipt check and insert after the
activation trigger ran. Preparation and activation now share a jurisdiction
advisory lock, and preparation rechecks that its bundle is the latest active
one before inserting. A deterministic concurrent database probe remains
deferred; both repository paths now use the same serialization point.

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

### RT-024-20 — Preparation identity did not bind retry bytes

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The low-level preparation helper treated evaluation, kind, and generator as
the complete retry key. Supplying different bytes for that key returned the
old row without comparing content. The helper now compares the content digest
before returning an existing row and rejects mismatches; the API test probes a
different byte payload under the same identity.
The comparison hashes bytes before writing them to object storage, so a
rejected mismatch does not create an orphan object as a side effect.

### RT-024-21 — Preparation output accepted a non-SHA-256 digest

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The foreign key on `output_digest_id` proved only that a digest row existed;
it did not enforce the algorithm used by preparation output. A direct writer
could bind an MD5 row to a preparation. The preparation trigger now requires
the output digest algorithm to be `sha256`, and the repository probe rejects a
non-SHA-256 insertion.

### RT-024-22 — SHA-256 digest rows accepted malformed hex

**Level:** CONFIRMED BY INDUCTION → REMEDIATED

The digest table required a non-empty string but did not require the canonical
64-character lowercase hexadecimal representation for `sha256`. A malformed
row could therefore look algorithmically valid to downstream foreign keys.
The schema now enforces the representation, and the repository probe rejects
malformed SHA-256 rows before they can bind to material.

### RT-024-24 — Normative source could bind a different capture digest

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high policy-integrity boundary

`normative_sources` stored both the captured-artifact digest and the source
digest, but the foreign keys did not require those references to be equal. A
direct writer could therefore make a source appear to describe one captured
artifact while its digest named another valid row. The new database trigger
rejects that mismatch, and the repository regression test exercises the failed
insert in an isolated transaction.

### RT-024-26 — Activation history was deletable or mutable

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high policy-integrity boundary

The schema described `policy_bundle_activations` as append-only but had no
database trigger enforcing that property. A direct writer could delete the
activation that made a bundle immutable, then mutate the historical bundle,
or rewrite activation metadata in place. The new trigger rejects both update
and delete, with regression coverage for each operation.

The table also now enforces one activation event per bundle, so a direct replay
cannot create duplicate history for the same immutable policy identity.

### RT-024-27 — Activated route requirements remained mutable

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high policy-integrity boundary

The route header, claims, and claim edges were immutable after activation, but
`action_route_requirements` had no equivalent trigger. A direct writer could
therefore alter the mandatory facts of an already-evaluated route. The new
trigger rejects insertion, update, and deletion of requirement rows once the
containing bundle is activated, with regression coverage for all three paths.

### RT-024-28 — Empty normative claims were application-only enforced

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high policy-integrity boundary

The schema documented that every claim needs at least one source, but a direct
writer could commit a claim before adding any source edge. The new deferred
constraint triggers allow atomic claim construction while rejecting an empty
claim at commit, including removal of the final source edge.

### RT-024-29 — Case-node payload kind was not enforced by the database

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high case-graph integrity boundary

The payload tables had composite foreign keys to `case_nodes`, but those keys
did not compare the `kind` discriminator. A direct writer could attach an
artifact payload to a user-assertion node, violating the graph's typed-node
invariant. A shared trigger now checks every payload table against the declared
kind, with a regression test for a cross-kind insert.

### RT-024-30 — Derived facts could consume interpretations

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high epistemic-integrity boundary

The database accepted any existing `case_nodes` row as a derived-fact input,
including an `Inference`, and did not require the referenced node to precede
the derived node. That could launder an interpretation into factual support or
permit cycles through direct writes. Triggers now require a prior,
non-inference input for derivations and prior-node inputs for inferences, with
regression coverage for the forbidden derived-fact edge.

### RT-024-25 — Policy bundle could bind a different capture digest

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** high policy-integrity boundary

`policy_bundles` duplicated the captured-artifact and bundle digest references
without enforcing the equality required by the bundle contract. A direct
writer could bind a valid but unrelated digest to an otherwise valid bundle.
The new trigger rejects that mismatch, with a repository regression test using
an isolated failed insert.

### RT-024-23 — Other digest algorithms remained bindable outside preparation

**Level:** CONFIRMED BY INDUCTION → HARDENED
**Severity:** medium integrity boundary

The `digests` table previously allowed arbitrary algorithms, while foreign keys
on artifacts, normative sources, policy bundles, evaluations, receipts, and
preparations checked only row existence. A direct writer could therefore bind
an `md5` row to a persisted identity even though the integrity boundary and
all producers use SHA-256. The migration now makes the digest table's sole
accepted representation explicit: algorithm `sha256` and exactly 64 lowercase
hexadecimal characters. The repository regression test proves rejection before
the row can be referenced.
