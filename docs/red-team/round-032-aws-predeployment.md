# Security Audit — NEXO
## Red Team Round 032

**Date:** 2026-09-22  
**Method:** Abductive Engineering (A–D–I) + Red-Team Auditing  
**Scope:** Vercel UI configuration, API CORS/health changes, AWS deployment
scaffolding, credential handling, and existing export/hash invariants.  
**Base:** main @ 7da5fe8  
**Reproducible evidence:** cargo test --workspace, npm run build,
cargo check -p nexo-api

## Threat model

- The attacker can control browser-local storage and inspect or alter their
  own browser state.
- The attacker cannot modify the deployed bundle, repository, API process,
  AWS account, database, or another user's credentials.
- The audit does not assume that AWS resources or a production API exist yet.

## Epistemic legend

CODE FACT · PLAUSIBLE HYPOTHESIS · CONFIRMED BY INDUCTION · FALSIFIED

## Executive summary

| ID | Severity | Level | Bucket | Finding |
|---|---|---|---|---|
| RT-032-01 | Low | CONFIRMED BY INDUCTION | hygiene / availability | An empty saved API base overrides the build-time API base |
| RT-032-02 | Informational | CODE FACT | threat-model / hygiene | /healthz is intentionally unauthenticated and reveals only ok |
| RT-032-03 | Medium residual | CODE FACT | availability / deployment | Production evidence flow remains unavailable until API, database, object storage, and Docker extractor are deployed |

## Findings

### RT-032-01 — Empty local API base masks the Vercel API configuration

**Severity:** Low  
**Epistemic level:** CONFIRMED BY INDUCTION  
**Bucket:** Hygiene / availability

- **Surprise / expectation violated:** Setting VITE_NEXO_API_BASE should give a
  newly deployed UI a usable API default.
- **Abduction:** localStorage.getItem("nexo-api-base") ?? env treats a stored
  empty string as authoritative instead of falling back to the environment.
- **Deduction:** If a user previously saves an empty API base, a later Vercel
  deployment with VITE_NEXO_API_BASE will still construct relative /v1
  requests.
- **Induction:** JavaScript nullish-coalescing behavior was reproduced with the
  same empty-string input: empty-string ?? https://api.example evaluates to an
  empty string. The source contains this exact precedence at web/src/main.ts.
- **Causal chain:** empty API input → empty string persisted in local storage →
  ?? does not fall back → relative API request to Vercel → connection failure
  or Vercel route miss.
- **Threat-model precondition:** The user has previously saved the empty API
  field. This is not an attacker-controlled server-side bypass.
- **Remediation:** Use a non-empty fallback (stored || env || empty string) or
  provide a visible reset API base action.

### RT-032-02 — Unauthenticated health endpoint

**Severity:** Informational  
**Epistemic level:** CODE FACT  
**Bucket:** Threat-model assumption / hygiene

GET /healthz returns the constant ok without authentication. This is
appropriate for a liveness probe and reveals no database state, credentials, or
case data. It must remain a liveness signal, not be reused as a readiness or
diagnostic endpoint.

### RT-032-03 — Backend deployment is still a functional residual

**Severity:** Medium residual  
**Epistemic level:** CODE FACT  
**Bucket:** Availability / deployment

The Vercel deployment contains only the TypeScript UI. The API still requires
PostgreSQL, a persistent filesystem object store/export root, the plaintext
extractor image, and a Docker daemon. Until those are deployed behind HTTPS,
case creation, evidence ingestion, evaluation, export, artifact downloads, and
hash verification cannot work from the public UI. This is an unfinished
deployment boundary, not a claim that the code path is insecure.

## Falsified / discarded vectors

| Vector | Result | Evidence |
|---|---|---|
| Export or artifact tampering bypasses the verifier | FALSIFIED | Existing verifier and object-store tamper tests pass |
| Hash seal changes with artifact or policy digest mutation | FALSIFIED | Integrity tests pass |
| Invalid UTF-8 crashes evidence ingestion | FALSIFIED | API test returns typed rejection |
| Sandbox can reach network or write the root filesystem | FALSIFIED | Sandbox flag tests pass |
| Credential rotation leaves plaintext output in the tested paths | FALSIFIED | Repository credential rotation tests pass |
| API CORS accepts arbitrary configured origins | FALSIFIED at code level | Origin is parsed from NEXO_WEB_ORIGIN; invalid values fail startup |

## Verification

cargo test --workspace passed: 154 tests and doc-tests. cargo check -p
nexo-api, npm run build, and git diff --check also passed. No AWS resource was
created during this round.

## Recommendations

1. Fix RT-032-01 before setting the production API URL in Vercel.
2. Deploy the API only after persistent storage, Docker sandbox execution, TLS,
   and backup/restore checks are in place.
3. Add an end-to-end CORS test against the deployed API after AWS provisioning.
