# Security Audit — Round 021 system-wide pre-commit

**Date:** 2026-09-16  **Base:** `main @ 13c5867`

## Confirmed remediation

### RT-066 — rule-match bundle version was not checked

Before this round, the evaluator checked a match's `PolicyBundleId` but ignored
`PolicyVersion`. A relation produced under `v0` could therefore be used with
bundle `v1`. The evaluator now requires both identity and exact version equality;
the stale-version test fails against the former implementation.

## Authority review

- `VerifiedCaptureAttestation` remains safe-callers-cannot-forge only; its
  `unsafe` constructor is intentionally isolated in the bridge.
- `PolicyRuleEngine::new` and witness minting are crate-private. External
  crates cannot construct the producer, but `pub(crate)` does not prove a unique
  in-crate authority.
- `VerifiedPreparationSnapshot` has no production minting caller. It remains a
  future consumer boundary, not evaluator evidence.
- `ActionOption::with_identity` accepts caller-supplied identity; durable
  identity binding remains an application residual.
- The public `from_supported_action` preparation shim always fails closed and
  is retained only as an explicit migration surface.

## Surface review

No public method sends, files, submits, or selects an external recipient.
Preparation state transitions preserve output evidence and typed invalidation.
Negative evaluator states require relational evidence and reject stale rule
versions.

## Residual limits

The pure core does not resolve opaque references against persistence, verify
input-manifest lineage, or prove the legal truth of a configured rule table.
Those claims remain outside this commit's authority boundary.
