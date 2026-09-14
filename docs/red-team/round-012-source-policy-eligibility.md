# Security Audit — Policy source eligibility

## Red Team Round 12

**Date:** 2026-09-14  **Method:** contract-vs-code confrontation + executable
constructor probes  
**Base:** `main` working tree after `af750e5`  
**Scope:** `SourcePolicy`, `PolicyBundle::source_is_eligible`, and source-policy
tests. Cryptographic capture verification and evaluator semantics remain outside
scope.

## Threat model

- An attacker can supply source-policy entries and captured source metadata
  through an import adapter.
- They can attempt to classify an unverified lead as normative, swap the
  acquisition channel, or duplicate policy entries.
- They cannot alter the compiled kernel; capture bytes are verified by a future
  integrity adapter before the bundle attestation is accepted.

## Executive summary

| ID | Level | Bucket | Finding |
| --- | --- | --- | --- |
| RT-032 | CODE FACT → remediated by induction | Semantic vulnerability | `Unverified` was previously admissible in `SourcePolicy`, despite being lead-only. |
| RT-033 | CONFIRMED BY INDUCTION | Invariant | Authority and acquisition channel remain independent dimensions. |
| RT-034 | CONFIRMED BY INDUCTION | Fail-closed behavior | Empty and duplicate source-policy dimensions are rejected. |

## RT-032 — Unverified source could be declared eligible

The prior constructor accepted `AuthorityKind::Unverified`, and `allows()` would
then return true for a matching channel. That contradicted the policy contract:
unverified material is a lead and cannot support a normative claim.

**Deduction:** constructing a policy with only `Unverified` authority should be
rejected before any source is evaluated.

**Induction:** `source_policy_rejects_duplicates_and_empty_dimensions` now
observes `SourcePolicyError::UnverifiedAuthority`. The constructor fails closed,
and `PolicyBundle::source_is_eligible` delegates only to this constrained
policy.

## RT-033 — Authority/channel confusion

A `PrimaryOfficial` source acquired through `OfficialApi` is accepted when both
dimensions are listed. The relation is conjunctive: an allowed authority does
not upgrade a disallowed channel, and a trusted channel does not upgrade a
secondary or unverified authority. The dedicated eligibility test passes this
boundary.

## RT-034 — Malformed policy entries

Empty authority/channel lists and duplicate entries are rejected with typed
errors. Bounds are enforced before retaining the vectors. No malformed policy
is silently normalized into a broader allow-list.

## Discarded vectors

| Vector | Result | Why it failed |
| --- | --- | --- |
| Secondary material upgraded by trusted acquisition channel | FALSIFIED by design | Eligibility requires both declared dimensions; channel is not authority. |
| Unverified lead used as normative support | FALSIFIED after remediation | Constructor rejects the authority class. |
| Cryptographic truth inferred from eligibility | Out of scope | Eligibility is not proof that a proposition is true. |

## Residual limits

`CaptureStatus::Verified` remains an adapter attestation rather than an
intrinsic cryptographic capability. Source-to-claim resolution and digest
equality still belong to the integrity/application boundary.
