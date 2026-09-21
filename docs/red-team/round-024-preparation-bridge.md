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

### RT-024-01 — Action payload substitution remains an internal composition obligation

**Level:** CODE FACT / ACCEPTED RESIDUAL

The bridge receives an `ActionOption` from the application composition and
checks only that it is available. The durable receipt stores the rendered
evaluation digest, not a normalized action-support vector, so the bridge does
not independently recompute that vector and compare it with the supplied
action. A future public preparation handler must pass the exact action returned
by the same evaluation call; it must never deserialize or accept an action from
the request. No preparation endpoint is exposed while this remains an
application-owned obligation.

This is not currently an HTTP exploit because the bridge is not routed and the
API request cannot supply an `ActionOption`. Before enabling preparation, add a
canonical action identity/payload check or make the evaluator-to-bridge handoff
an unforgeable in-process token.

## Exit decision

The bridge is safe to keep internal and unexposed for the current evaluation
slice. Preparation/export endpoints remain deferred pending the residual's
resolution and deterministic output generation.
