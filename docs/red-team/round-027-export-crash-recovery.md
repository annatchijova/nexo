# Security Audit — export crash recovery
## Red Team Round 027

**Date:** 2026-09-22  **Scope:** filesystem-first export followed by durable
preparation transition. **Base:** `main @ c0138fb` before the fix.

## Finding

### RT-027-01 — A filesystem-first failure could strand a valid export

**Level:** PLAUSIBLE HYPOTHESIS → REMEDIATED  **Bucket:** recovery/invariant

The export directory is materialized before PostgreSQL records
`export_manifest_digest_id`. If the database commit failed after the rename,
the preparation could remain `prepared` while a retry saw an existing
destination and failed closed without a recovery path.

The producer now adopts an existing destination only when independent
verification and all durable identities match: case, policy digest, output
artifact digest, and `preparation/{id}` label. Any mismatch remains a hard
failure. Unit tests cover both adoption and rejection; the database transition
still records the manifest digest in the same transaction.

