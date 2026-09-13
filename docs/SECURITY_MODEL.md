# Security model: Stage 0

## Threat model

An attacker can submit hostile files, malformed metadata, misleading declarations, stale or adversarial URLs, replay requests, and content intended to manipulate a future LLM narrator. An attacker may obtain application-level write access but not the configured HMAC key or an independently retained chain-tip anchor.

An attacker cannot turn a hash into proof that content is true, make a model-generated explanation authoritative, or use a plain SHA-256 chain alone as protection against an attacker able to recompute the entire database history.

## Trust boundaries

1. Browser input to API: untrusted request data.
2. Artifact ingestion to object storage: untrusted bytes.
3. Artifact parsing to sandbox worker: hostile content; no ambient application credentials.
4. Official source acquisition to policy bundle: source provenance must be reviewed and versioned before activation.
5. Domain projection to LLM: untrusted output returns as display-only text.
6. Export to independent verifier: verification must not trust the API implementation.

## Initial fail-closed rules

- Unknown jurisdiction, missing current policy, incomplete provenance, or unmet mandatory requirements produce a non-available action state.
- A parser failure produces an extraction failure record, never an invented observation.
- An LLM timeout or invalid response falls back to deterministic citation rendering; it does not block access to the underlying graph.
- An artifact is never executed, rendered in a privileged browser context, or passed to a shell command.
- Every ingestion, parsing, and export path will have bounded byte size, nesting depth, time, memory, and retry limits before implementation begins.

## Integrity claims

SHA-256 establishes byte identity for a declared payload. It does not establish origin, truth, admissibility, legal validity, or narrative quality.

Hash chains are reserved for append-only event history where detecting retrospective alteration matters. If that threat includes a database writer who can recompute the chain, NEXO must use a key-bound HMAC and retain the expected tip outside the writer's authority.
