# Security Audit — Round 020 binding-evidence composition

**Date:** 2026-09-15  **Base:** `main @ 13c5867`

## Falsification targets

### RT-063 — cardinality assumption

**OPEN HYPOTHESIS.** Nothing in the current code proves that one future
`EvaluationReceipt` should own all four edges. The architecture must remain
valid if evidence is composed from multiple independently verifiable records.

### RT-064 — edge substitution

Test whether a valid edge for one action, snapshot, policy, or input can be
combined with endpoints from another. Endpoint identity must be checked at
composition, not inferred from shared type names.

### RT-065 — digest overclaim

A valid digest proves byte identity only. It cannot by itself prove `S → P`,
`S → I`, or `X → A`; those relations need an asserting record and authority.

## Exit criteria

- composition cardinality remains open until evidence warrants a choice;
- each edge has an explicit owner/producer and verification mechanism;
- mismatched endpoints fail closed;
- no component is declared authoritative merely because it can hash bytes.
