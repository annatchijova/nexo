# Security model: Stage 0

## Threat model

An attacker can submit hostile files, malformed metadata, misleading declarations, stale or adversarial URLs, and replay requests. An attacker may obtain PostgreSQL write access but not the application deployment's configured HMAC keyring. This version does not claim protection when application, runtime environment, and HMAC secret are compromised together.

An attacker cannot turn a hash into proof that content is true, or use a plain SHA-256 chain alone as protection against an attacker able to recompute the entire database history.

## Trust boundaries

1. Browser input to API: untrusted request data.
2. Artifact ingestion to object storage: untrusted bytes.
3. Artifact parsing to sandbox worker: hostile content; no ambient application credentials.
4. Official source acquisition to policy bundle: source provenance must be reviewed and versioned before activation.
5. Domain projection to interface: explanation is deterministic rendering of authorized support; nothing external is consulted and nothing generated feeds back into evaluation.
6. Export to independent verifier: verification must not trust the API implementation.

## Initial fail-closed rules

- Unknown jurisdiction, missing current policy, incomplete provenance, or unmet mandatory requirements produce a non-available action state.
- A parser failure produces an extraction failure record, never an invented observation.
- Explanation rendering is deterministic and reproducible from the graph projection; no model, generator, or external inference service participates in any path.
- An artifact is never executed, rendered in a privileged browser context, or passed to a shell command.
- Every ingestion, parsing, and export path will have bounded byte size, nesting depth, time, memory, and retry limits before implementation begins.

## Integrity claims

SHA-256 establishes byte identity for a declared payload. It does not establish origin, truth, admissibility, legal validity, or explanatory quality.

Hash chains are reserved for append-only event history where detecting retrospective alteration matters. `audit_chain/v2` adds mandatory key-bound HMAC for a PostgreSQL-only writer threat. Independent checkpoint retention and external anchoring remain a later authority-bearing phase.
