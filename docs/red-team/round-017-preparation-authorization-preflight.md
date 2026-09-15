# Security Audit — Round 017 preparation authorization

**Date:** 2026-09-15  **Base:** `main @ b69146c`

## Falsification

### RT-056 — public plan constructor accepts unsupported routes

**CODE FACT.** `PreparationPlan::new` currently accepts opaque action and
snapshot references without receiving an `ActionOption`. A caller can therefore
prepare material for an unavailable or conditionally supported route.

### RT-057 — opaque references are not referential proof

**CODE FACT / RESIDUAL.** Even a constructor that receives an `ActionOption`
cannot resolve a `NodeId` or digest against persistence. The application layer
must bind those references to the same durable evaluation snapshot before
calling the core factory.

### RT-058 — authorization subject and recorded identity diverge

**CONFIRMED BY API SHAPE; CLOSED ONLY AT THE FACTORY BOUNDARY.** The original
factory accepted `&ActionOption` and a separate `action_option: NodeId`. A
caller could authorize one value and record another. The remediation removes
the second subject from the factory and uses one `IdentifiedActionOption` token
carrying both values. This proves `factory authorization subject =
token.action` and `factory persisted identity = token.identity`; it does **not**
prove `token.identity = durable identity authentic for token.action`.
That latter claim remains RT-057 and belongs to application/persistence.

## Remediation contract

- Make the raw plan constructor crate-private.
- Expose a factory that requires an `IdentifiedActionOption` and rejects
  anything that is not `SUPPORTED`/available.
- Keep snapshot and digest values explicit and immutable in the resulting plan.
- Do not add send/file/transport capabilities while closing this boundary.

## Residual limits

The core can prove the action value is available at the call site. It cannot
prove that an opaque snapshot ID or digest resolves to the same database row;
nor that the caller-supplied identity names that exact action in durable
storage. Those are application/integrity invariants and must be checked there.

## Exit criteria

- unsupported and conditional actions cannot enter a public preparation plan;
- authorization and recorded action identity travel in one
  `IdentifiedActionOption` token;
- generated/exported/invalidated lifecycle remains unchanged;
- tests demonstrate the former bypass and the new fail-closed factory;
- application referential-integrity residual is documented, not implied away.

## Next boundary: four independent bindings

The next preparation red-team must establish each edge separately for
`ActionOption A`, caller-supplied identity `X`, snapshot `S`, policy digest
`P`, and input digest `I`:

```text
X ──identifies──────► A
A ──belongs to──────► S
S ──evaluated under─► P
S ──derived from────► I
```

Co-presence in `PreparationPlan` is not evidence for any edge. A future
application/integrity mechanism must provide a verifiable relation (or fail
closed) for each one; the core must not silently upgrade these references into
proof merely because their types are non-empty.
