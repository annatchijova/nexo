# Security Audit — Policy bundle selection

## Red Team Round 11

**Date:** 2026-09-14  **Method:** contract-vs-code confrontation + executable
selection probes  
**Base:** `main` working tree after `policy.rs` implementation  
**Scope:** `PolicyBundle`, `SourcePolicy`, `PolicyBundleSet`, selection tests,
and `POLICY_BUNDLE_CONTRACT.md`. Integrity integration, persistence, and the
future evaluator remain outside scope.

## Threat model

- Attacker can submit imported bundle data, reorder bundles, replay stale
  bundles, duplicate claims, and choose unsupported schema versions.
- The application adapter is responsible for verifying captured bytes before
  constructing a domain bundle.
- Attacker cannot alter the compiled core; this round does not establish that
  any legal proposition is true.

## Executive summary

| ID | Level | Bucket | Finding |
| --- | --- | --- | --- |
| RT-028 | CODE FACT | Trust-boundary assumption | `CaptureStatus::Verified` is publicly constructible; the kernel records an adapter attestation, not cryptographic proof. |
| RT-029 | CONFIRMED BY INDUCTION | Invariant | Selection is independent of bundle insertion order and rejects overlapping current bundles. |
| RT-030 | CONFIRMED BY INDUCTION | Fail-closed behavior | Empty, stale, wrong-jurisdiction, duplicate, and unknown-schema cases do not produce `Selected`. |
| RT-031 | CODE FACT | Schema evolution | Known historical schema migration is specified but not implemented in this slice; non-current schemas are rejected. |

## RT-028 — Verification status is an explicit boundary assumption

`CaptureStatus` is a public enum, so any caller can pass `Verified` together
with arbitrary artifact, digest, and provenance references. The core has no
bytes and deliberately cannot recompute the digest. This is not a break of
`nexo-integrity`; it is a trust-boundary assumption that must be enforced by the
application/integrity adapter before the constructor is called.

The contract now says “adapter attestation” rather than cryptographic proof.
The future integration must supply the attestation only after byte/digest
verification and should make that capability difficult to forge.

## RT-029 — Deterministic and ambiguity-safe selection

Two bundle sets with identical members in opposite insertion order return the
same result. Two current bundles for one jurisdiction return
`AmbiguousSelection`, never an arbitrary winner. These probes are
**CONFIRMED BY INDUCTION** by `selection_is_independent_of_bundle_insertion_order`
and `overlapping_current_bundles_fail_closed_as_ambiguous`.

## RT-030 — Failure states remain distinct

The executed tests distinguish `NoBundle`, `OutOfJurisdiction`, and
`PolicyNotCurrent`; duplicate claims, duplicate bundle identities, empty source
policy dimensions, unverified capture, and schema zero/current mismatch fail at
typed constructors. No case is silently converted into a selected policy.

## RT-031 — Migration is not yet a runtime capability

The contract permits migrating known older schema versions while rejecting
unknown/newer versions. The implementation currently accepts only
`PolicySchemaVersion::CURRENT`; older versions are rejected rather than
migrated. This is conservative and fail-closed, but the migration chain must be
implemented before historical bundles are imported.

## Discarded vectors

| Vector | Result | Why it failed |
| --- | --- | --- |
| Choose the newest bundle by vector order | FALSIFIED | Selection considers all current matches and returns ambiguity. |
| Use `INSUFFICIENT_FACTS` for missing policy | FALSIFIED | Selection has policy-specific typed results; factual insufficiency belongs to evaluation. |
| Treat a digest as proof of legal truth | Out of scope | A digest authenticates bytes, not proposition correctness. |

## Residual limits

The selector has no persistence/activation transaction, no schema migrators, and
no cryptographic capability type linking `Verified` to an actual integrity
verification. Those belong to the application/integrity boundary and must be
closed before untrusted imports reach production evaluation.
