# Security Audit — NEXO canonical sealing

## Red Team Round 4

**Date:** 2026-09-13
**Method:** A–D–I canonicalization probes
**Scope:** `nexo-integrity` canonical bytes and SHA-256 sealing; no audit chain,
parser, API boundary, persistence, or key-bound anchor exists yet.

## Threat model

- Attacker CAN submit arbitrary values through a future adapter and attempt
  ordering, Unicode, type, and display-normalization ambiguity.
- Attacker CANNOT alter the compiled canonicalizer or break SHA-256.
- This round tests identity of bytes, never truth of their contents.

## Executive summary

| ID | Level | Result |
| --- | --- | --- |
| RT-007 | FALSIFIED | Map insertion order does not change a seal. |
| RT-008 | FALSIFIED | Typed values, Unicode forms, CRLF/LF, and list order do not collide. |
| RT-009 | CODE FACT | Size and nesting limits remain an adapter boundary until parsing exists. |

## RT-007 — Map iteration instability

**Prediction:** equivalent maps built in opposite insertion order produce the
same canonical bytes and seal. **Induction:**
`cargo test --workspace map_insertion_order_does_not_change_seal` passed.
`BTreeMap` order plus recursive encoding makes this vector FALSIFIED.

## RT-008 — Semantic/display collisions

**Prediction:** `U64(1)` vs text `"1"`, text vs bytes, NFC vs NFD, LF vs
CRLF, and differently ordered lists produce different seals. **Induction:**
`cargo test --workspace types_do_not_collide unicode_and_line_endings_remain_byte_distinct list_order_is_sealed` was executed through the full suite; all assertions passed. The vectors are FALSIFIED: NEXO seals byte identity and deliberately does not normalize display-equivalent Unicode.

## RT-009 — Resource exhaustion

**Level:** CODE FACT. The public recursive value model does not impose byte or
depth bounds. This is not currently an externally reachable vulnerability:
there is no parser or API. It is a mandatory boundary contract for the future
adapter, not a reason to silently truncate a value before sealing.

## Residual risks

- SHA-256 proves identity, not origin or truth.
- A future audit chain needs an independently retained receipt or key-bound
  anchor to detect whole-history replacement.
- The next integrity increment must add bounded import parsing before any
  untrusted JSON/value becomes `CanonicalValue`.
