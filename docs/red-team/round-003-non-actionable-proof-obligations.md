# Security Audit — NEXO non-actionable proof obligations

## Red Team Round 3

**Date:** 2026-09-13  
**Method:** Abductive Engineering (A–D–I) + Red-Team Auditing  
**Base under test:** `main @ 01cce06ee761aefe4bc8e48903d1f39348be74f9`  
**Scope:** `nexo-core` evaluation values and constructors only. No persistence,
policy parser, graph resolver, HTTP adapter, cryptography, or sandbox worker
exists in scope yet.

## Threat model

- An attacker can control references passed by a future adapter or can exploit
  a defect in a trusted policy evaluator that constructs domain values.
- An attacker cannot modify the Rust binary, bypass a constructor through
  unsafe code, rewrite an already-validated graph node, or compromise a future
  policy bundle signer.
- The boundary examined is typed reference construction: a distinct Rust
  wrapper must not be mistaken for independent provenance if both wrappers can
  name the same graph node.

## Epistemic legend

`CODE FACT` · `PLAUSIBLE HYPOTHESIS` · `CONFIRMED BY INDUCTION` · `FALSIFIED`

## Executive summary

| ID | Severity | Level | Bucket | Finding |
| --- | --- | --- | --- | --- |
| RT-004 | Medium | CONFIRMED BY INDUCTION | Architectural invariant | Independent proof roles could alias one `NodeId`, allowing self-attestation. |
| RT-005 | Medium | CONFIRMED BY INDUCTION | Architectural invariant | One legal claim could be represented as a legal conflict. |
| RT-006 | None | FALSIFIED | Constructor-bypass vector | Public enum variants do not expose payload fields for external literal construction. |

## RT-004 — Proof-role wrappers permitted self-attestation

**Severity:** Medium for epistemic integrity; no external exploit path exists
until an adapter accepts attacker-controlled references.  
**Epistemic level:** CONFIRMED BY INDUCTION.  
**Bucket:** Architectural invariant.

### Surprise / expectation violated

`PolicyNotCurrent` promises a policy-bundle identity plus independent
freshness evidence. `OutOfJurisdiction` promises jurisdiction evidence plus a
policy bundle. At the tested base, both constructors accepted the same opaque
`NodeId` in each role.

### Rival hypotheses

1. Distinct newtype wrappers are sufficient to make evidence independent.
2. Distinct wrappers prevent accidental API mixing but still permit the same
   graph node to play both proof roles.
3. The future graph resolver alone should police this relation.

Hypothesis 2 was the cheapest to test: call each constructor with one node
wrapped twice. Hypothesis 3 is not an adequate substitute; the domain core
already owns the local anti-self-attestation invariant.

### Deduction

If hypothesis 2 is correct, both of these inputs will construct successfully
at the base:

```text
PolicyNotCurrent(bundle = node(1), freshness_evidence = node(1))
OutOfJurisdiction(jurisdiction_evidence = node(1), bundle = node(1))
```

### Induction

The baseline probes were executed against the unremediated working tree based
on `01cce06`:

```bash
cargo test --workspace red_team_baseline -- --nocapture
```

Observed: all three baseline probes passed; the two inputs above were accepted.
The third probe is RT-005 below. The pre-remediation constructors were
infallible `new(...) -> Self`, so acceptance was directly observable.

### Causal chain

```text
one opaque NodeId
    ↓ wrapped as two distinct semantic roles
infallible constructor accepts both roles
    ↓
negative result appears to carry two proof references
    ↓
one node can certify itself
```

### Remediation and postcondition

Both constructors are now fallible. They reject equal underlying node IDs with
`FreshnessEvidenceAliasesPolicyBundle` and
`JurisdictionEvidenceAliasesPolicyBundle`. Regression tests execute those
rejections. This prevents self-reference; it does **not** prove that the two
different nodes have correct graph kinds or a valid evidentiary relation. That
requires the future graph resolver and policy layer.

## RT-005 — A singleton citation could manufacture legal conflict

**Severity:** Medium for epistemic integrity; no external exploit path exists
until an adapter or policy evaluator accepts attacker-controlled references.  
**Epistemic level:** CONFIRMED BY INDUCTION.  
**Bucket:** Architectural invariant.

### Surprise / expectation violated

`Abstention::ConflictingLegalClaims` expresses inability to choose between
competing legal sources. A vector with one claim, or one claim repeated twice,
cannot establish conflict.

### Rival hypotheses

1. A singleton claim is enough because conflict can be implicit in omitted
   metadata.
2. At least two distinct claims are a necessary minimum proof obligation;
   fuller contradiction analysis belongs to the policy layer.

Hypothesis 2 is more economical and testable without pretending this core can
interpret legal text.

### Deduction

If the base lacks the cardinality invariant, this construction succeeds:

```text
ConflictingLegalClaims(context = [user assertion], claims = [claim A])
```

### Induction

The same baseline command above observed successful construction of the
singleton value. The prediction held.

### Causal chain

```text
non-empty vector treated as conflict
    ↓
one claim satisfies constructor
    ↓
system can abstain "because sources conflict"
    ↓
the stated reason has no competing source
```

### Remediation and postcondition

The constructor now requires at least two claims and rejects duplicate claims.
The test suite covers both one-claim and duplicated-one-claim inputs. It does
**not** assert that two distinct claims truly conflict; semantic contradiction
is explicitly deferred to the policy evaluator.

## RT-006 — External caller bypasses private payload constructors

**Severity:** None.  
**Epistemic level:** FALSIFIED.  
**Bucket:** Constructor-bypass vector.

### Hypothesis and deduction

Because `Abstention` variants are public, an external crate may be able to
literal-construct `NoAuthoritativeLegalSource` with arbitrary unbounded or
empty context, bypassing `try_new`.

### Induction

The public type now contains a `compile_fail` doctest that attempts exactly
that literal construction. `cargo test --workspace` compiles the doctest only
when the external construction fails. The observed suite passes with that
failure enforced.

### Result

FALSIFIED for this payload: public enum visibility does not expose private
payload fields. This does not justify unsafe code or future deserialization
paths; those remain outside the current core.

## Discarded vectors

| Vector | Result | Why it failed or remains bounded |
| --- | --- | --- |
| Turn `ABSTAIN` into `SUPPORTED` by supplying no legal claim | FALSIFIED by prior contract | `ActionOption` has no `ABSTAIN` status and its constructor requires legal support. |
| Use an inference as factual support | FALSIFIED by type surface | `FactualSupport` has no `Inference` variant. |
| Treat two distinct claims as semantically contradictory merely because the core accepts them | Not a finding | The core deliberately enforces only minimum cardinality; policy semantics are not implemented. |

## Residual risks and next gate

- `NodeId` is still only a typed reference. The graph layer must verify node
  existence, node kind, provenance, ownership, and the edge relation between
  two distinct IDs.
- Policy freshness requires canonical reference-time semantics and a signed,
  versioned policy bundle before it is a legal conclusion.
- The next layer remains `nexo-integrity`, but only after this remediation is
  committed and the complete test suite is rerun. No integrity hash can repair
  a semantically invalid evaluation created before sealing.
