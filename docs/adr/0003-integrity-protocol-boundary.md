# ADR 0003: Integrity seals identity, not authority

## Status

Accepted as the contract for the initial `nexo-integrity` layer.

## Decision

`nexo-integrity` is a dependency-free-from-application Rust crate except for a
pinned SHA-256 implementation. It accepts only a closed `CanonicalValue` data
model: null, booleans, signed/unsigned integers, text, bytes, lists, and
ordered maps. Floats, timestamps from a clock, random values, arbitrary JSON,
and generated prose are absent from the sealing type.

Canonical encoding is binary, typed, length-delimited, recursively ordered, and
versioned. SHA-256 seals either supplied artifact bytes or canonical bytes.

An audit entry seals its exact event payload, sequence, and previous entry
digest. A receipt commits the expected entry count and tail digest. Verification
reports payload-integrity and linkage failures separately; receipt verification
detects tail truncation. Neither construction proves truth, origin, legal
validity, or protection against a writer able to replace the entire chain and
its externally retained receipt.

## Non-goals

- Encryption, signatures, key management, persistence, clocks, and network I/O.
- Parsing artifacts or policy documents.
- Treating explanatory text as seal-worthy authority.
- Claiming that a hash chain alone prevents database-history rewrites.

## Falsifiers

The layer is wrong if map insertion order changes a seal, distinct typed values
share canonical bytes, a changed event verifies, or receipt-backed tail
truncation verifies.
