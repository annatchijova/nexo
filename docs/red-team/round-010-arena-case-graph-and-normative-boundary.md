# Security Audit — Arena contribution and normative boundary

## Red Team Round 10

**Date:** 2026-09-13  **Method:** contract-vs-code confrontation + executable
induction probes  
**Base:** `main @ eddf6aa` (Arena PR #1), with the normative-source
remediation in the working tree  
**Scope:** Arena's `CaseGraph` slice, its contract/tests, and the previously
landed `NormativeClaim` source-support constructor. CI, persistence, adapters,
and the future evaluator remain outside scope.

## Threat model

- Attacker CAN submit hostile graph payloads through a future adapter: missing
  references, wrong node kinds, inference laundering attempts, oversized
  fan-in, blank locators, and contradictory assertions.
- Attacker CANNOT alter the compiled core or bypass private payload fields.
- This round tests structural admission and provenance boundaries, not the
  truth of a declaration or the authority of a legal source.

## Epistemic legend

CODE FACT · PLAUSIBLE HYPOTHESIS · CONFIRMED BY INDUCTION · FALSIFIED

## Executive summary

| ID | Level | Bucket | Finding |
| --- | --- | --- | --- |
| RT-023 | CODE FACT → remediated by induction | Semantic vulnerability | `NormativeClaim` compared `(source, role)`, allowing one source to occupy both roles. |
| RT-024 | FALSIFIED | Invariant | An `Inference` cannot enter factual support or a `DerivedFact`. |
| RT-025 | FALSIFIED | Invariant | Observation and derivation reference checks reject dangling and incompatible nodes. |
| RT-026 | CODE FACT | Adapter hardening | Text size, artifact size, and inference depth are intentionally unbounded in the core. |
| RT-027 | CODE FACT | Operational hygiene | The repository pins Rust 1.98.0; local verification requires that toolchain or an explicit offline override. |

## RT-023 — Source uniqueness crossed the role boundary

**Surprise:** the constructor documentation promised duplicate-source rejection.

**Abduction:** `ClaimSourceSupport` equality includes `SupportRole`; therefore a
`Primary` and `Corroborating` record for the same `NormativeSourceId` could be
treated as distinct. A rival explanation was that role duplication was
intentional, but that would contradict the many-to-many source relation and the
`DuplicateSource` error name.

**Deduction:** submit one source id twice with different roles; the constructor
must reject it.

**Induction:** the new test
`claim_rejects_same_source_even_when_roles_differ` executes this input and
observes `NormativeClaimError::DuplicateSource`. The implementation now compares
source identity independently of role.

**Causal chain:** same source id → two role-bearing wrappers → whole-wrapper
equality misses duplication → one source could count twice. The source-id
comparison closes that path without changing role semantics.

## RT-024 — Inference laundering

The absent `Inference` variant on `FactualSupport`, the `NodeKind` predicate,
and insertion rejection for `DerivedFact` inputs were exercised by Arena's
example and property tests. The predicted laundering path did not occur;
**FALSIFIED** for the implemented boundary.

## RT-025 — Dangling and wrong-kind references

Property tests generate references beyond the assigned id range and non-artifact
observation targets. The graph rejects both with typed errors. The derivation
input checks likewise reject absent nodes. These vectors are **FALSIFIED** for
sequential insertion.

## RT-026 — Adapter resource bounds

`NonEmptyText`, artifact byte size, and inference-over-inference depth have no
core-level resource ceiling. This is a **CODE FACT**, not an exploit at this
boundary: the contract assigns byte/body and adapter allocation limits to the
application edge. Any adapter that allocates attacker-controlled values without
limits would turn this residual into a resource-exhaustion issue.

## RT-027 — Toolchain availability

`rust-toolchain.toml` pins Rust 1.98.0 and the CI workflow tests with that pin.
The current sandbox cannot download it, while the installed stable toolchain
passes the suite. This is operational hygiene, not a domain-integrity defect.

## Discarded vectors

| Vector | Result | Why it failed |
| --- | --- | --- |
| Future-node cycle through public insertion | FALSIFIED | Sequential ids make the future reference dangling. |
| Inference-only action support | FALSIFIED | `Inference` is absent from `FactualSupport`. |
| Hash/provenance truth guarantee | Out of scope | A digest proves byte identity, not legal truth. |

## Residual limits

The graph still has no persistence transaction boundary, evaluator, adapter
resource budget, or independent external review. Arena's red-team report is
valuable evidence, but it was produced by the same contribution stream that
implemented the slice; a later independent review remains desirable.
