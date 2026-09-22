# Security Audit — hashed actor credentials
## Red Team Round 030

**Date:** 2026-09-22  **Scope:** actor authentication credential storage and
bootstrap lookup. **Base:** `12cd6dc` before the fix.

## Finding

### API-030-01 — Bearer credentials were stored as actor identity text

**Level:** CONFIRMED → REMEDIATED  **Bucket:** secret lifecycle / persistence

The prior actor lookup compared the presented bearer directly with
`actors.external_identity`, so a database read exposed a credential usable at
the HTTP boundary.

Authentication now hashes the presented credential before lookup. The schema
stores only the lowercase SHA-256 digest in `actor_credentials`, with a unique
credential identity, immutable actor/digest/issuance fields, and nullable
`revoked_at`. Multiple credentials can coexist so rotation can overlap before
the old one is revoked. The actor and case ownership model remains unchanged.

This is appropriate only for high-entropy bearer credentials; it is not a
password-hashing scheme for human secrets. Credential issuance/revocation
operations and an upgrade path for pre-existing plaintext actor rows remain
deployment work and are intentionally not implied as complete here.

