# Security Audit — NEXO audit receipts

## Red Team Round 5

**Date:** 2026-09-13
**Method:** A–D–I tamper probes
**Scope:** in-memory `AuditTrail`, `AuditReceipt`, and independent verification.

## Threat model

An attacker can alter, reorder, or truncate entries supplied for verification.
They cannot alter the independently retained receipt or recompute SHA-256
collisions. A writer able to replace both history and receipt remains outside
this construction.

## Results

| ID | Level | Result |
| --- | --- | --- |
| RT-010 | FALSIFIED | Editing an event makes its entry digest invalid. |
| RT-011 | FALSIFIED | Truncation, reordering, and predecessor edits fail receipt/link verification. |
| RT-012 | CODE FACT | A receipt stored beside the writable chain is not an independent anchor. |

`cargo test --workspace` executed the probes. Payload alteration returns
`Digest { index: 1 }`; tail truncation returns `ReceiptLength`; reordering
returns `ReceiptTip`; and predecessor manipulation returns `Link { index: 1 }`.

The implementation claims exactly this: submitted history matches an external
receipt. It does not claim event truth, causal validity, or resistance to a
writer who can replace every mutable copy of history and receipt.
