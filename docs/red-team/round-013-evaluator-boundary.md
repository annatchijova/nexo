# Security Audit — Pure action evaluator

## Red Team Round 13

**Date:** 2026-09-14  **Method:** contract-vs-code confrontation + executable
route probes  
**Base:** `main` working tree after `f3ad59a` and evaluator slice
**Scope:** `ActionRoute`, `CaseProjection`, `NormativeContext`, and `evaluate`.
No persistence, HTTP, model, or external-action adapter is in scope.

## Threat model

- An attacker can submit route references, factual-support ids, requirement
  states, claims, and source metadata through an application adapter.
- They can attempt jurisdiction confusion, stale claims, missing sources,
  inference laundering, and requirement bypasses.
- They cannot alter compiled constructors or make the evaluator read a clock.

## Executive summary

| ID | Level | Bucket | Finding |
| --- | --- | --- | --- |
| RT-035 | CODE FACT → remediated by induction | Semantic vulnerability | Claim jurisdiction was initially not compared with route jurisdiction. |
| RT-036 | CONFIRMED BY INDUCTION | Invariant | A current source-complete route yields `SUPPORTED` with non-empty proof. |
| RT-037 | CONFIRMED BY INDUCTION | Fail-closed behavior | Missing requirements yield `INSUFFICIENT_FACTS`, not an available action. |
| RT-038 | CONFIRMED BY INDUCTION | Trust boundary | Bundle and claim mismatch yields `OUT_OF_JURISDICTION` or `ABSTAIN`, never `SUPPORTED`. |
| RT-039 | CONFIRMED BY INDUCTION | Jurisdiction invariant | An `AR` bundle + `AR` route + explicit `US` claim reaches `claim_is_eligible` and returns `ABSTAIN`. |
| RT-040 | CODE FACT | Capability boundary | `VerifiedCaptureAttestation` is safe-to-hold but can be minted by another crate only through an explicit `unsafe` call. |

## RT-035 — Jurisdiction mismatch in legal claims

The first evaluator draft checked bundle jurisdiction and claim membership but
did not compare the claim's explicit jurisdiction text with the route. A claim
labelled `US` could therefore support an `AR` route if both referenced the same
bundle. The evaluator now uses canonical `JurisdictionCode::code()` values and
rejects mismatches before constructing an action.

The regression `evaluator_rejects_claim_from_another_jurisdiction` executes the
exact `AR` bundle + `AR` route + `US` claim case and observes
`NonActionable::Abstain`. The bundle/route guard is proven separately by
`evaluator_rejects_route_from_another_jurisdiction`, so the claim-text case
reaches `claim_is_eligible` rather than failing earlier.

## RT-040 — Attestation minting capability

The public API exposes `unsafe fn from_verified_capture`, so another crate can
mint an attestation if it explicitly accepts an unsafe block. This means “only
the bridge can mint” is not a true construction property. The supported claim
is narrower and tested: safe callers cannot invoke the constructor without an
unsafe operation, and serialized verification flags are never accepted. The
trust model therefore treats unsafe bridge code as the integrity boundary; a
future stronger “bridge-only” property would require moving the bundle factory
and attestation type into a crate topology where the minting constructor is not
publicly reachable.

## RT-036 — Supported route proof obligations

With a current bundle, eligible source, matching claim, and factual projection,
`evaluate` constructs `ActionOption` through its invariant-preserving
constructor. The supported test observes an available action with both support
paths non-empty.

## RT-037 — Missing factual requirements

When a legal route is current and source-complete but a mandatory requirement is
not satisfied, the evaluator returns `InsufficientFacts` carrying legal support
and the named missing requirement. It never represents that route as
`AVAILABLE`.

## RT-038 — Bundle and source mismatch

The evaluator requires every route claim to be present in the selected bundle,
bound to that bundle, current, jurisdiction-matching, and supported by sources
allowed by the bundle's authority/channel policy. Failure returns a typed
abstention; no optional field can silently downgrade the proof requirement.

## Discarded vectors

| Vector | Result | Why it failed |
| --- | --- | --- |
| Inference-only factual projection | FALSIFIED by construction | `CaseProjection` accepts only `FactualSupport`, whose enum has no inference variant. |
| Stale bundle treated as current | FALSIFIED by construction | Bundle interval is checked against explicit reference date. |
| Missing requirement shown as available | FALSIFIED by induction | `InsufficientFacts` path precedes `ActionOption` construction. |

## Residual limits

The evaluator does not yet model legal-claim conflicts, contraindications, or
conditional actions as first-class route semantics. Source digest verification
remains the bridge's responsibility. The unsafe attestation constructor is an
explicit capability boundary, not a proof that only one crate can mint. Case
projection authorization and persistence transactionality remain
application-layer obligations.
