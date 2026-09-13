# Integrity protocol contract

## Threat model

An attacker can supply arbitrary artifact bytes and values through a future
adapter, reorder maps, alter stored events, remove a tail, or replay a prior
export. The attacker cannot alter an independently retained receipt or break
SHA-256 collision resistance. A database writer able to replace both a chain
and every receipt is outside this layer's protection.

## Canonical form v1

`NEXO-C14N\x01` prefixes every value. Each value has a distinct one-byte tag;
variable-length payloads use an unsigned 64-bit big-endian byte length; map
keys are UTF-8 text sorted by their bytes. No float tag exists. Therefore the
seal is deterministic without locale, clock, map iteration, or display text.

## Audit proof

Each entry commits `sequence`, `previous_digest`, and full event value. A chain
receipt commits `length` and `tip`. Verification separately checks recomputed
entry hashes, links, sequence, and receipt. A valid result means the submitted
history matches that receipt—not that its events are true.
