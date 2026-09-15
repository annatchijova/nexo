# Security Audit — Round 019 preparation-binding ownership

**Date:** 2026-09-15  **Base:** `main @ e9a7a15`

## Inverse search result

| Relation | Current owner with sufficient evidence | Result |
| --- | --- | --- |
| `X identifies A` | None. The evaluator returns an `ActionOption` without a durable action identity; application owns persistence identity. | **Residual** |
| `A belongs to S` | None. The evaluator has no snapshot value or snapshot record. | **Residual** |
| `S evaluated under P` | Evaluator has `PolicyBundle` during evaluation, but emits no snapshot binding or receipt. | **Insufficient evidence** |
| `S derived from I` | Integrity can hash bytes/canonical values, but no manifest-to-snapshot relation exists. | **Insufficient evidence** |

## Conclusion

No current component has authority and evidence for all four edges. Therefore
`VerifiedPreparationSnapshot` remains a consumer/future boundary. Its
`from_verified_evaluation` name describes intended provenance, not a property
currently enforced by the module graph.

The evaluator must not be wired to mint the capability merely because it sees a
bundle and returns an actionable value. A future application/evaluation receipt
must first define which component owns each edge and what independently
verifiable record discharges it.

## Red-team residuals

- `pub(crate)` limits external crates but does not identify a unique in-crate
  minting authority.
- `with_identity` keeps values together but does not authenticate caller-supplied
  identity against durable state.
- Opaque `NodeId` and `DigestId` values remain references, not proofs.
