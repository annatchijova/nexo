# Security Audit — Round 016 preparation lifecycle

**Date:** 2026-09-15  **Base:** `main @ 0047b0a`

## Findings

### RT-051 — output is absent before generation

**CONFIRMED BY TYPE SHAPE.** `PreparationPlan` contains only references and
generator identity. An output artifact and digest exist only in `Prepared`,
`Exported`, or `Invalidated`; there is no optional output field to populate
partially.

### RT-052 — export is not an external act

**CONFIRMED BY API SURFACE.** `Exported` exposes the prepared material and an
invalidation transition. It has no recipient, endpoint, credential, transport,
send, file, or submit operation. The domain cannot represent `SENT`, `FILED`,
or `AUTO_SUBMIT`.

### RT-053 — stale material remains auditable but not current

**CONFIRMED BY TRANSITION.** `Prepared` and `Exported` can become
`Invalidated`, retaining the exact material and a typed reason. No transition
returns an invalidated value to a current state.

### RT-054 — public plan construction does not prove support

**RESIDUAL / INTENTIONAL.** `PreparationPlan::new` accepts opaque references;
it does not resolve an `ActionOption`, snapshot, or digest. The application
layer must obtain and verify those references before calling it. This slice
proves lifecycle shape, not graph/database referential integrity.

### RT-055 — output digest is metadata, not a byte proof

**RESIDUAL / INTENTIONAL.** `prepare` accepts an `ArtifactId` and `DigestId`
without hashing bytes. The integrity boundary must produce and verify those
values; the core records them but does not duplicate the cryptographic
operation.

## Exit criteria

- state transitions are typestate-like and fail closed;
- no external-act state or capability exists;
- invalidation preserves evidence and reason;
- residual trust boundaries are explicit rather than implied.
