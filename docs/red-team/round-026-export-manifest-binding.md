# Security Audit — export manifest provenance binding
## Red Team Round 026

**Date:** 2026-09-22  **Scope:** preparation export lifecycle, manifest reads,
and authenticated download endpoints. **Base:** `main @ 108ff2a` before the
fix.

## Threat model

The attacker can tamper with files below the configured export root after an
export has been produced. They cannot modify the application binary or the
PostgreSQL schema and do not bypass bearer authentication.

## Finding

### RT-026-01 — Export state did not bind the manifest identity

**Level:** CONFIRMED BY INDUCTION → REMEDIATED  **Bucket:** software
vulnerability / integrity-boundary gap

The database recorded only `status = exported`. The verifier proved that a
manifest was internally consistent, but there was no durable digest proving
that it was the manifest produced for this preparation. A replacement
manifest could therefore be internally valid while being unrelated to the
preparation row.

The fix adds nullable `preparations.export_manifest_digest_id`, requires it
when entering `exported`, makes it immutable afterwards, and compares it on
re-export and every manifest download. The integration regression replaces
`manifest.json` after export and observes `409`; the intact export still
passes independent verification and the six-test API harness.

## Discarded vectors

| Vector | Result |
| --- | --- |
| Re-export unchanged preparation | FALSIFIED: returns the same verified digest. |
| Tamper with an artifact while leaving the manifest | FALSIFIED: independent verification/re-hash rejects it. |
| Cross-owner access to manifest or artifact | FALSIFIED: case authorization runs before export lookup. |

