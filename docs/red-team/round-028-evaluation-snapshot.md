# Security Audit — evaluation snapshot
## Red Team Round 028

**Date:** 2026-09-22  **Scope:** evaluation projection and durable result
transaction. **Base:** `076a101` before the fix.

## Finding

### API-022-02 / RT-028-01 — Evaluation could persist a mixed graph snapshot

**Level:** PLAUSIBLE HYPOTHESIS → REMEDIATED  **Bucket:** concurrency/invariant

The handler previously built its projection through the pool, then opened a
separate transaction to persist the evaluation and receipt. A graph mutation
could commit in that interval, leaving the durable evaluation newer than the
input graph it described.

The evaluation path now begins one transaction, locks the case row before
reading `case_nodes`, builds the projection from that transaction, and keeps
the lock through evaluation and receipt insertion. Graph mutations use the
same case-row lock before inserting nodes, establishing the required
happens-before ordering. The transaction rolls back on any persistence error.

The lock is scoped to the case, not the whole service, and no network or
filesystem I/O occurs while it is held. The remaining deployment concern is
database connection/lock timeout configuration, which belongs to Step 9.

