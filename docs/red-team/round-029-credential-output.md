# Security Audit — credential output
## Red Team Round 029

**Date:** 2026-09-22  **Scope:** local API startup helper and bearer-token
handling. **Base:** `e888f79` before the fix.

## Finding

### API-029-01 — The development launcher printed and defaulted a bearer token

**Level:** CONFIRMED → REMEDIATED  **Bucket:** secret lifecycle / disclosure

`scripts/run_api.sh` previously supplied `dev-owner-token` when no credential
was configured and printed `NEXO_BOOTSTRAP_OWNER` to stdout. Terminal capture,
CI logs, shell scrollback, or a copied support transcript could therefore
retain a credential that authenticates as the owner.

The launcher now requires `NEXO_BOOTSTRAP_OWNER` to be supplied by the caller
and never prints its value. The token still remains a plaintext database lookup
key in the current single-owner authentication model; hashed, revocable,
rotatable credentials remain a separate Step 9 deployment residual and must be
implemented with an issuance and rotation path, not by merely hiding this log.

