# Security Audit — NEXO `nexo-core`

## Red Team Round 1

**Date:** 2026-09-13  
**Method:** adversarial invariant review + mutation induction  
**Base:** `main @ ac6986715dc9b7953731740901bc1634737a3f53`  
**Scope:** `ActionOption` construction, availability, and retained collection bounds. HTTP, parsers, storage, policy sources, and cryptographic serialization are not implemented and are out of scope.

## Threat model

- An adapter or future caller can supply arbitrary support and requirement lists.
- A caller cannot directly construct private `ActionOption` fields.
- A caller cannot make an `Inference` into `FactualSupport` through the current Rust type surface.
- This round does not claim that body-size limits exist before an adapter allocates decoded input; no adapter exists yet.

## Epistemic legend

**CODE FACT** · **PLAUSIBLE HYPOTHESIS** · **CONFIRMED BY INDUCTION** · **FALSIFIED**

## Executive summary

| ID | Severity | Level | Bucket | Result |
| --- | --- | --- | --- | --- |
| RT-001 | Low | CONFIRMED BY INDUCTION | Test quality | A test comment claimed to kill a mutation that the suite did not kill. Corrected. |
| RT-002 | Low | CODE FACT → remediated | Hardening | `try_new` retained unbounded vectors. Explicit per-action limits and hostile tests added. |

## Findings

### RT-001 — Availability mutation was not killed

**Severity:** Low  
**Epistemic level:** CONFIRMED BY INDUCTION  
**Bucket:** Test quality, not a software vulnerability.

- **Surprise:** the test comment asserted that replacing `status == Supported && unmet_requirements.is_empty()` with `status == Supported` would fail a test.
- **Abduction:** because the constructor rejects `Supported` with unmet requirements, the two expressions may be equivalent over all constructible values.
- **Prediction:** mutate the implementation to `status == Supported`; all existing tests will remain green.
- **Induction:** the mutation was applied to the base implementation and `cargo test --workspace` reported `5 passed; 0 failed`.
- **Conclusion:** confirmed. The prior wording overstated the mutation coverage. The comment was replaced; the constructor test, rather than `is_available`, is the test that defends this invariant.

### RT-002 — Retained action collections had no upper bound

**Severity:** Low  
**Epistemic level:** CODE FACT before remediation; remediation verified by tests.  
**Bucket:** Hardening, not an independently exploitable vulnerability while no untrusted adapter exists.

- **Code fact:** `ActionOption::try_new` previously accepted arbitrary-length `Vec` values for factual support, legal support, and unmet requirements.
- **Threat-model precondition:** a future adapter or caller supplies a large already-allocated decoded collection.
- **Remediation:** the core now rejects collections above 64 entries with distinct errors. Three adversarial tests were first added red (missing constants/errors), then passed after the implementation.
- **Residual limit:** this protects retained domain state; it cannot prevent an HTTP/JSON adapter from allocating a large request before calling the core. The future boundary parser must enforce byte, element-count, and nesting limits before allocation.

## Discarded vectors

| Vector | Result | Why it failed |
| --- | --- | --- |
| Turn an inference into factual support | Not reachable through this type surface | `FactualSupport` is a closed enum without an `Inference` variant; no conversion exists. This is a CODE FACT, not a claim about future adapters. |
| Make `SUPPORTED` available with known unmet requirements | Not reachable through the constructor | `try_new` rejects the combination before an `ActionOption` exists. |

## Verification after remediation

```text
cargo test --workspace
8 passed; 0 failed

cargo clippy --workspace --all-targets -- -D warnings
passed

git diff --check
passed
```
