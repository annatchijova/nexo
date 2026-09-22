# Security Audit — schema startup
## Red Team Round 031

**Date:** 2026-09-22  **Scope:** application startup migration and concurrent
bootstrap. **Base:** `ddf15e2` before the fix.

## Finding

### API-031-01 — A restart could re-run the baseline DDL and fail

**Level:** CONFIRMED → REMEDIATED  **Bucket:** deployment / availability

`apply_migration` previously executed the complete baseline SQL on every
startup. A second process or a normal restart against the same PostgreSQL
database would hit existing tables, types, functions, and triggers before the
idempotent policy seed could run.

The baseline is now executed inside one transaction under a PostgreSQL
advisory transaction lock. It creates and records the `0001_init` schema
version; later starts observe the marker and commit without replaying DDL. A
crash rolls back the entire first application, and a repository integration
test applies the migration twice. The API and schema harnesses also pass.

This establishes idempotence for databases created with the version marker.
Older pre-marker databases still require an explicit upgrade procedure rather
than being silently treated as current.

