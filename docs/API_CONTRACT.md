# HTTP API contract (v1)

## Purpose

The first adapter layer per `docs/ARCHITECTURE.md`'s dependency rule
(Web -> API/application -> domain core): `nexo-api` owns transactions
through `nexo-app`, evidence ingestion through `nexo-sandbox`/
`nexo-extraction`, and deterministic explanation rendering of results
`nexo-core` already computed. It contains no domain logic of its own.

Implements plan.md Step 5, on top of the repository layer that completed
Step 1's deferred slice. Targets exactly one policy bundle end to end
(`nexo-policy-ar`'s Ley 25.326 data-access bundle) rather than a generic
multi-bundle import surface — the generic surface is future work once a
second bundle exists to design against.

## Authentication

A bearer token that *is* the actor's `external_identity`:
`Authorization: Bearer <token>`. Real, not a hardcoded bypass — every
actor is a durable row with an ownership model already enforced by
`docs/APPLICATION_LAYER_CONTRACT.md`. Adding a second person is "create
another actor row," not a schema or auth-model rewrite; swapping the token
scheme (hashed API keys, OAuth, session cookies) later only touches
`src/auth.rs` — every handler downstream only ever sees an already
authenticated `ActorRowId`.

**Known limitation, not hidden:** the token is compared to the stored
`external_identity` directly (not a separately hashed credential) —
acceptable for the single-owner personal deployment this round targets,
not yet a credential system to expose to untrusted multi-tenant traffic.
Revisit before Step 9 (personal deployment) if the deployment is ever
reachable by more than the owner.

Every case-scoped endpoint calls `auth::authorize_case`, which loads the
case's owner and compares it to the authenticated actor — a mismatch and a
nonexistent case both return `404`, never `403`, so a case's existence is
not leaked to a non-owner.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/v1/cases` | Create a case owned by the authenticated actor. |
| `GET` | `/v1/cases/{case_id}` | Read the authorized case graph node summary. |
| `POST` | `/v1/cases/{case_id}/evidence` | Ingest plain-text evidence: object store -> sandboxed extraction -> artifact + observation nodes, or a bounded rejection reason. |
| `POST` | `/v1/cases/{case_id}/assertions` | Record a user assertion (confirmed or not). |
| `POST` | `/v1/cases/{case_id}/evaluate` | Build a `CaseProjection` from durable case state, run the real `nexo_core::evaluate`, render and record the result. |
| `GET` | `/v1/cases/{case_id}/evaluations` | List recorded evaluations, returning the same rendered JSON stored at evaluation time. |

### `POST /v1/cases/{case_id}/evidence`

Body: `{"filename": string | null, "text": string}`. Runs the real
`nexo-sandbox` + `nexo-extractor-plaintext` pipeline (Docker, hardened per
`docs/SANDBOX.md`) on a blocking-safe task so a slow or hostile artifact
cannot stall the async runtime. Response:
`{"artifact_node_id": i64, "observation_count": usize, "rejection_reason": string | null}`.
The artifact is always durably recorded, even when extraction rejects it —
evidence is never silently dropped because it failed extraction.

### `POST /v1/cases/{case_id}/assertions`

Body: `{"confirmed": bool}`. Deliberately has no free-text field:
`nexo_core::UserAssertionNode` (and this schema's `user_assertion_nodes`
table) carries no content column — only actor, time, and confirmation
state, per `docs/CASE_GRAPH_CONTRACT.md`'s ontology table. An API-only text
field here would be modeling drift this contract does not introduce.

### `POST /v1/cases/{case_id}/evaluate`

Builds a `CaseProjection` from every factual-support-eligible node
currently in the case (artifacts, observations, user assertions, derived
facts — never inferences, per the evidence/interpretation boundary), runs
`nexo_core::evaluate` against the seeded AR bundle's single route, and
persists the result with its full rendered JSON. Returns
`{"evaluation_id": i64, "result": <rendered evaluation>}`.

`404 case has no evidence yet` (as `422 UNPROCESSABLE_ENTITY`) when the
case has no factual-support-eligible node at all — `CaseProjection`
requires at least one.

### Rendered evaluation shape

Produced by `explain.rs`, which is deliberately the *only* place that
turns an already-decided `nexo_core::ActionEvaluation` into JSON — per
`docs/ARCHITECTURE.md`: "Explanation is a deterministic citation rendering
computed by the API from an authorized graph projection." Nothing in this
module decides anything.

```json
{
  "kind": "actionable",
  "status": "supported",
  "available": true,
  "factual_support": [{"kind": "artifact", "case_node_id": 1}, ...],
  "legal_support": [
    {"proposition": "Art. 14, Ley 25.326: ...", "source_issuer": "InfoLEG - ...", "source_locator": "http://servicios.infoleg.gob.ar/..."},
    ...
  ],
  "unmet_requirements": 0
}
```

or, for a non-actionable result:

```json
{"kind": "non_actionable", "variant": "insufficient_facts", "legal_support": [...], "missing_requirement_count": 1}
```

Every `NonActionable` variant serializes with its full typed reason — an
`abstain` result names its specific `cause`
(`no_authoritative_legal_source` / `unsupported_question` /
`conflicting_legal_claims`), never collapsed into a generic "no result."

`nexo_core::NodeId` is intentionally opaque (no public accessor to its
inner integer — see `crates/nexo-policy-ar` for the same pattern applied
to `NormativeClaimId`/proposition text). `explain::NodeIdResolver` respects
that: every `NodeId` this API constructs for an evaluation is recorded
against its `case_node_id` before `evaluate` runs, then resolved back by
equality afterward — never by reading a value out of the id.

## Known simplifications (recorded, not hidden)

- **One case's identity requirement, one signal.** The AR fixture's
  single mandatory requirement (art. 14.1's identity-proof precondition)
  is satisfied by "the case has at least one confirmed user assertion" —
  not tied to a *specific* piece of evidence proving identity
  specifically. A real deployment would model which requirement a given
  assertion satisfies explicitly.
- **One shared jurisdiction/freshness evidence pair for every case.**
  `nexo_policy_ar::build()`'s `jurisdiction_evidence`/`freshness_evidence`
  ids are fixed, not case-specific durable evidence nodes. `nexo-core`
  only compares these ids for identity in the paths this bundle
  exercises today (not their content), so reuse is safe for now; a real
  per-case jurisdiction determination is future work.
- **Bundle re-seeded on every process start.** `main.rs` calls
  `seed_ar_bundle` unconditionally at startup, and
  `policy_bundle_activations` is append-only by design — so every restart
  creates a new bundle row and activation. Fine for one long-running
  process; a multi-instance deployment needs an idempotent "seed if not
  already active" check.
- **Bundle provenance is homed on a dedicated bootstrap case,** because
  `provenance_records.case_id` is `NOT NULL` in the schema and bundle
  provenance is not naturally case-scoped. Not a real user case; never
  returned by any case-listing endpoint (none exists yet).
- **Single-owner token model,** see Authentication above.

## Non-goals (this round)

- Preparation/export endpoints (`docs/ARCHITECTURE.md`'s "Preparation
  boundary" — `Preparation` repository functions do not exist yet either).
- A generic multi-bundle import/selection surface.
- The web UI (Step 6) and independent verifier (Step 8).
