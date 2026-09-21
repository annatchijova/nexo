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
