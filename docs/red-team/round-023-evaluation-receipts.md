# Security Audit — NEXO evaluation receipts
## Red Team Round 023

**Date:** 2026-09-21  **Method:** A–D–I + Red-Team Auditing  
**Scope:** `evaluation_receipts`, the repository insertion path, the
PostgreSQL trigger, and the API's receipt gate. Preparation/export capability
was not enabled.  
**Base:** working tree derived from `0c52846`, before the trigger correction.
**Reproduction:** `./scripts/test_repository.sh` against PostgreSQL 16.

## Threat model

The attacker can reach a future in-process application/repository caller that
can supply evaluation and receipt row identifiers. They cannot modify the
binary, migrations, or database role, and they do not have direct database
credentials. The current HTTP API is an additional gate, but repository and
database invariants must not depend on that one caller remaining the only
caller.

## Finding

### RT-023-01 — Repository path accepted a receipt for a non-actionable evaluation

**Severity:** medium integrity boundary  **Level:** CONFIRMED BY INDUCTION  
**Bucket:** software invariant defect

**Expectation:** an evaluation receipt is evidence for a future preparation
capability and therefore may exist only for an available `SUPPORTED` result.

**Abduction:** the API checked this condition, but the repository function and
database trigger might rely on that caller-side guard. A second possibility was
that the unique receipt constraint would reject the probe before the result
state was tested. The latter was tested first because it was cheapest.

**Induction:** the first probe was indeed vacuous: an existing receipt caused
the insert to fail on the unique constraint. The experiment was corrected by
deleting that receipt and changing the evaluation to `non_actionable` inside a
temporary transaction. The corrected prediction was rejection by the
receipt invariant; the pre-fix code instead accepted the receipt, and the test
failed at `non_actionable_receipt.is_err()`.

**Causal chain:**

```text
non-actionable evaluation row
    ↓ caller invokes repository insert directly
receipt trigger checks identity fields only
    ↓ no result-state check
receipt row is accepted
    ↓ future preparation code could treat it as binding evidence
```

**Correction:** the trigger now requires `result_kind = actionable` and
`action_status = supported`, in addition to matching evaluation identity.
The adversarial test remains and passes after the fix.

### RT-023-02 — Policy bundle references could be mixed across route claims and evaluations

**Severity:** high integrity boundary  **Level:** CONFIRMED BY INDUCTION
**Bucket:** software invariant defect

**Expectation:** every action route and every evaluation must resolve to one
policy bundle. A route claim from another bundle changes the policy meaning of
the route; an evaluation bundle from another route makes its sealed result
ambiguous.

**Induction:** a repository test first created a route with a claim from a
different bundle and then created an evaluation whose route and bundle IDs
also differed. Before the correction, both inserts succeeded. The probes were
independent transactions and asserted failure at the exact boundary, so they
were not explained by a later receipt constraint.

**Correction:** PostgreSQL now rejects route-claim rows whose claim bundle
differs from the route bundle, and rejects action evaluations whose bundle
differs from the referenced route bundle. The test covers both mismatches and
passes after the correction.

### RT-023-03 — Durable policy objects could cross jurisdiction boundaries

**Severity:** medium integrity boundary  **Level:** CONFIRMED BY INDUCTION  
**Bucket:** software invariant defect

**Expectation:** a normative claim and an action route are scoped to the
jurisdiction declared by their policy bundle. Persisting a different scope
would leave the database able to hold a policy graph that the evaluator must
later reject.

**Induction:** repository probes attempted to insert a `UY` claim and a `UY`
route under an `AR` bundle. Before the correction both inserts were accepted;
the core evaluator's jurisdiction checks therefore did not protect durable
state from a direct repository caller.

**Correction:** database triggers now require claim and route jurisdiction to
match the referenced policy bundle. The adversarial probes pass after the
correction.

### RT-023-04 — Activated policy bundles were mutable after evidence issuance

**Severity:** high integrity boundary  **Level:** CONFIRMED BY INDUCTION  
**Bucket:** software invariant defect

**Expectation:** activation freezes the policy identity used by evaluations
and receipts; policy history is append-only.

**Induction:** a repository probe updated `policy_version` on an activated
bundle after a supported evaluation and receipt had been committed. Before
the correction, PostgreSQL accepted the update, leaving the existing receipt
anchored to a mutable policy row.

**Correction:** an activation-aware trigger rejects every update to an
activated `policy_bundles` row. The adversarial update probe now fails and the
transaction is rolled back.

### RT-023-05 — Activated route and claim definitions were mutable

**Severity:** high integrity boundary  **Level:** CONFIRMED BY INDUCTION  
**Bucket:** software invariant defect

**Expectation:** activation freezes not only the bundle header but also the
route and normative claims that give that bundle its operational meaning.

**Induction:** repository probes updated an activated route title and an
activated claim proposition after a supported evaluation and receipt existed.
Before the correction both updates succeeded, so a receipt could retain the
same row identifiers while the policy meaning changed underneath it.

**Correction:** database triggers now reject updates and deletes for routes,
route-claim edges, and claims belonging to activated bundles. The adversarial
updates are rolled back and the repository suite passes.

### RT-023-06 — Activated claim support sources were mutable

**Severity:** high integrity boundary  **Level:** CONFIRMED BY INDUCTION
**Bucket:** software invariant defect

**Expectation:** the captured normative source and its claim-support edges
remain fixed once the containing bundle is activated.

**Induction:** a repository probe updated the locator of a source referenced by
an activated claim. Before the correction, the update succeeded. The
hardening test also exposed that the seed path activated bundles before
finishing their child rows, so construction order was corrected to complete
the policy graph before activation.

**Correction:** source rows and claim-source edges used by activated claims
are immutable; the seed and repository fixtures now activate only after the
complete policy graph is constructed.

## Discarded vectors

| Vector | Result | Why |
| --- | --- | --- |
| Duplicate receipt accepted | FALSIFIED | The unique evaluation foreign key rejects a second receipt. |
| Cross-case receipt accepted | FALSIFIED | The trigger compares receipt case, route, bundle, schema, and evaluator version to the evaluation; the mismatch test is rejected. |
| API creates receipt for a non-actionable evaluation | FALSIFIED after correction | API gate and database trigger both reject the path; end-to-end tests assert no receipt exists. |
| First non-actionable probe | DISCARDED | It hit the unique constraint before testing result state; the corrected probe removed that confounder. |

## Residual risks

- The receipt stores digest row references; independently verifying that a
  supplied digest is the digest of the claimed manifest remains an
  application-capability concern, not a property of the foreign key.
- Jurisdiction equality is now database-enforced for claims and routes, but
  the digest row references in an evaluation receipt still do not prove that
  the referenced bytes are the digest of the claimed manifest/result. That
  remains an application-capability concern for the preparation boundary.
- Activation immutability is now database-enforced for policy bundles; the
  bundle's route and claim definitions are frozen as well. The deployment
  role still needs explicit update/delete privilege hardening for the broader
  append-only audit tables and source rows.
- No `VerifiedPreparationSnapshot` or preparation endpoint is enabled yet.
