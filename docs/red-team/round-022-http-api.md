# Security Audit — NEXO HTTP API
## Red Team Round 022

**Date:** 2026-09-21  **Method:** A–D–I + Red-Team Auditing
**Scope:** `nexo-api` routes, authentication, ownership checks, evidence
ingestion orchestration, projection/evaluation persistence, and API tests.
**Base:** `780cb39` (Claude's HTTP adapter commit).
**Verification:** `./scripts/test_api.sh`, `cargo test --workspace`, and
`cargo clippy --workspace --all-targets -- -D warnings`.

## Threat model

The attacker can send HTTP requests and control bearer-token input and
request bodies. They cannot modify the binary, PostgreSQL schema, Docker
daemon, object-store root, or policy fixture. Direct database compromise and
single-host deployment hardening remain out of scope.

## Findings

### API-022-01 — Required case-read endpoint was absent

**Severity:** medium functional gap  **Level:** CODE FACT
**Bucket:** contract/implementation defect

The build plan requires create/read case operations, while the base router had
`POST /v1/cases` and case-scoped mutation/evaluation routes but no
`GET /v1/cases/{case_id}` route. The live router source proves the route was
absent. The gap is now closed with an authorized node-summary response, plus
an owner/other-actor test executed against the new code.

### API-022-02 — Evaluation snapshot may race with concurrent mutation

**Severity:** medium residual risk  **Level:** PLAUSIBLE HYPOTHESIS
**Bucket:** invariant/concurrency

`evaluate_case` builds its projection in one or more reads, then starts a
separate transaction only to insert the result. If evidence or an assertion
is committed between those phases, the persisted evaluation can describe an
older graph while being recorded after the newer mutation. The application
contract says a command such as evaluation executes atomically, but the
current code does not yet provide a single read/write snapshot for the whole
operation.

This remains a hypothesis: a deterministic interleaving test was not added in
this round. The next audit should introduce a barrier-controlled concurrent
test and either confirm the mixed snapshot or falsify it under the chosen
PostgreSQL isolation level.

## Discarded vectors

| Vector | Result | Why |
| --- | --- | --- |
| Cross-owner case read through evaluations and the new case-read route | FALSIFIED | Both routes reuse `authorize_case`; end-to-end tests return 404 for the other actor. |
| Invalid UTF-8 evidence causing an API panic | FALSIFIED | The typed rejection path is covered by the API test and passed end-to-end. |
| Oversized JSON bypassing the body limit | FALSIFIED for the tested boundary | Axum's default body limit rejects oversized requests before the handler; the explicit 2 MiB boundary remains documented by Claude's contract. |
| Token scheme suitable for hostile multi-tenant deployment | NOT CLAIMED | Plain external identities are a documented single-owner limitation, not silently treated as production authentication. |

## Residual recommendations

- Make evaluation projection and sealed-result insertion use one explicit
  database snapshot or transaction boundary, then prove it with a controlled
  concurrency test.
- Before Step 9, replace plaintext external identities as bearer credentials
  with a scoped, hashed, revocable credential mechanism.
- Keep the read-case response deliberately a summary until the graph payload
  and export contracts define which artifact metadata may be returned.
