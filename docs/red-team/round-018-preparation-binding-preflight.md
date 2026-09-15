# Security Audit — Round 018 preparation binding

**Date:** 2026-09-15  **Base:** `main @ b69146c`

## Four relations to falsify

Given `ActionOption A`, identity `X`, snapshot `S`, policy digest `P`, and
input digest `I`, field co-presence proves none of these edges:

```text
X ──identifies──────► A
A ──belongs to──────► S
S ──evaluated under─► P
S ──derived from────► I
```

### RT-059 — identity authenticity

`IdentifiedActionOption` closes the factory-level split but `with_identity`
still accepts caller-supplied `X`. The core does not resolve `X` to durable
action state. This is a known application boundary, not a closed proof.

### RT-060 — snapshot membership

`from_supported_action` accepts any `S`; it does not prove that the snapshot
contains the exact authorized action value. A persistence-backed snapshot
authority must establish this relation before the plan is admitted.

### RT-061 — policy binding

`P` is an opaque digest reference. The core cannot establish that `S` was
evaluated under that policy bundle without a signed/verified evaluation record
or an equivalent application-owned relation.

### RT-062 — input binding

`I` is also opaque. The core cannot establish that the snapshot was derived from
those exact inputs; the manifest/integrity boundary must verify it.

## Required direction

Do not make a struct with five fields and call these edges proven. The current
slice introduces `VerifiedPreparationSnapshot` with private fields and a
crate-private evaluator constructor. `PreparationPlan` consumes that
capability rather than independently supplied `S`, `P`, and `I` values. This
closes the factory co-presence bypass, but not the durable-record proof.

## Residual limit

Even a private constructor inside `nexo-core` proves only that a trusted core
producer asserted the relation. Authenticity of durable records remains outside
the pure domain kernel until an independent receipt or verification protocol is
introduced.

## Open questions for the next live-code audit

- `from_verified_evaluation` is `pub(crate)`: this proves only that external
  crates cannot mint the capability. It does not prove that the evaluator is
  the sole in-crate caller. Search the module graph before naming that stronger
  authority claim.
- `from_supported_action` is a public compatibility shim that always returns
  `VerifiedSnapshotRequired`. Confirm whether downstream API compatibility
  requires retaining it; otherwise remove it in a deliberate breaking change.
- `from_supported_action_in_core` was searched across the live tree and had no
  callers or distinct contract. It was removed as dead surface (option C), not
  because `pub(crate)` alone proves a unique minting authority.
