# ADR 0007: verify captures before policy construction

## Status

Proposed for the application/integrity bridge between `nexo-integrity` and the
dependency-free `nexo-core` policy types.

## Decision

Captured policy bytes are hashed and compared with their expected digest before
the bridge constructs a `PolicyBundle`. Serialized verification flags are
untrusted metadata and cannot authorize construction. The bridge emits an
application-owned attestation carrying the verified capture references; the
core records those references but never performs I/O or hashing.

## Alternatives rejected

- **Make `nexo-core` depend directly on storage or hashing:** breaks the inward
  dependency rule and makes domain evaluation environment-dependent.
- **Trust `CaptureStatus::Verified` from serialized input:** allows a forged
  status to masquerade as an integrity result.
- **Re-hash only at export time:** lets an unverified bundle influence policy
  selection before export and creates a temporal gap.

## Falsification evidence

The bridge is wrong if a one-byte mutation, digest mismatch, or forged verified
flag can still yield a `PolicyBundle`; if the same bytes produce different
attestations; or if a historical capture changes after a later retrieval.
