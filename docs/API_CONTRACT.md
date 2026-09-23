# HTTP API contract (v1)

## Purpose

The first adapter layer per `docs/ARCHITECTURE.md`'s dependency rule
(Web -> API/application -> domain core): `nexo-api` owns transactions
through `nexo-app`, evidence ingestion through `nexo-sandbox`/
`nexo-extraction`, and deterministic explanation rendering of results
`nexo-core` already computed. It contains no domain logic of its own.

Implements plan.md Step 5, on top of the repository layer that completed
Step 1's deferred slice. Now targets two real, independently seeded policy
bundles (`nexo-policy-ar`'s Ley 25.326 data-access bundle, and
`nexo-policy-ar-digital-violence`'s Ley 27.736 bundle), selected per
evaluation via `?bundle=<key>` — see "Bundle selection" below. Still not a
generic import surface for arbitrary bundles; that remains future work.

## Authentication

A bearer token is resolved through an actor credential row:
`Authorization: Bearer <token>`. Only the SHA-256 digest is stored; the raw
token is never persisted or logged. This is real, not a hardcoded bypass — every
actor is a durable row with an ownership model already enforced by
`docs/APPLICATION_LAYER_CONTRACT.md`. Adding a second person is "create
another actor row," not a schema or auth-model rewrite; swapping the token
scheme (hashed API keys, OAuth, session cookies) later only touches
`src/auth.rs` — every handler downstream only ever sees an already
authenticated `ActorRowId`.

Credential rows support overlapping live credentials and explicit
revocation, now exposed as a minimal self-service surface:

- `POST /v1/credentials` — issues a new, high-entropy (32 random bytes,
  hex-encoded) credential for the *same actor already authenticated on
  this request*, never an actor named in the body (that would make this
  an account-creation endpoint, not a rotation one). Returns
  `{"credential": "<token>"}` exactly once — only its digest is stored,
  so this is the only chance to see the plaintext. The actor's existing
  credential(s) stay valid: issuing is additive, matching "a new device
  starts using a new token while old devices keep working" rather than a
  swap.
- `DELETE /v1/credentials/current` — revokes the exact credential
  presented on *this* request, never one named in a body or path (which
  would let one valid token revoke a different credential belonging to
  someone else). Proof of possession of the token is the only
  authorization this needs, mirroring "log this device out." `204` on
  success, `404` if it was already revoked.

There is still no endpoint to list an actor's own credentials (by id or
label) or to revoke one other than the one currently in use — only "issue
a new one" and "revoke the one I'm using right now" exist. Revoking a
*different* device's credential still requires direct database access.
This is not a password-hashing scheme for low-entropy secrets.

Every case-scoped endpoint calls `auth::authorize_case`, which loads the
case's owner and compares it to the authenticated actor — a mismatch and a
nonexistent case both return `404`, never `403`, so a case's existence is
not leaked to a non-owner.

## Bundle selection

`GET /v1/bundles` (unauthenticated — it names only which legal routes
exist, not case data) lists the seeded bundles:

```json
[
  {"key": "ley-25326", "display_name": "Ley 25.326 — acceso, rectificación y supresión de datos personales"},
  {"key": "ley-27736", "display_name": "Ley 27.736 (Ley Olimpia) — violencia digital, remoción de contenido"}
]
```

`POST /v1/cases/{case_id}/evaluate?bundle=<key>` evaluates against the
named bundle; omitting `?bundle=` defaults to `ley-25326`
(`nexo_api::DEFAULT_BUNDLE_KEY`), so every endpoint that existed before
bundle selection was added keeps working unchanged. An unknown key is
`404`.

Each bundle in `crates/nexo-api/src/bundle.rs::PolicyBundleHandle` carries
its own `RequirementSignal` — the rule for when its one mandatory
requirement counts as satisfied, since the two bundles' preconditions are
genuinely different (a confirmed identity assertion for Ley 25.326; the
specific content/URL being identified, via an `Artifact` node, for Ley
27.736). This is why the same case can be `InsufficientFacts` under one
bundle and `Actionable` under the other from the same evidence — verified
by `ley_27736_bundle_is_actionable_once_content_is_identified` in
`crates/nexo-api/tests/api_test.rs`.

`POST /v1/cases/{case_id}/preparations` does not take a `bundle` parameter:
which bundle produced an evaluation is looked up from that evaluation's own
receipt binding (`repository::find_evaluation_receipt_binding`), never
assumed or re-specified by the caller — an evaluation's bundle identity is
exactly what the receipt already pins.

**A real bug found and fixed while wiring the second bundle in:**
`nexo-app`'s preparation-invalidation check
(`repository::current_policy_bundle_id`) originally asked "is this the most
recently activated bundle for this **jurisdiction**?" — correct when only
one bundle per jurisdiction was ever seeded, but the moment a second,
unrelated AR bundle (Ley 27.736) was activated, it made every existing Ley
25.326 preparation appear stale (`PolicyBundleChanged`), because both
bundles share the jurisdiction string `"AR"`. Confirmed by induction: the
existing end-to-end preparation test failed immediately after the second
bundle was wired in. Fixed by adding a `bundle_key` column to
`policy_bundles` (`migrations/0001_init.sql`) — a stable identity for
"which legal instrument is this a version of," distinct from jurisdiction —
and rescoping `current_policy_bundle_id`/`lock_policy_jurisdiction` by that
column. A jurisdiction can now host several independently current bundles
without one's activation invalidating another's.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/bundles` | List the seeded policy bundles (key + display name). Unauthenticated. |
| `POST` | `/v1/credentials` | Issue a new bearer credential for the authenticated actor. Returned once. |
| `DELETE` | `/v1/credentials/current` | Revoke the credential presented on this request. |
| `POST` | `/v1/cases` | Create a case owned by the authenticated actor. |
| `GET` | `/v1/cases/{case_id}` | Read the authorized case graph node summary, including node creation timestamps. |
| `POST` | `/v1/cases/{case_id}/evidence` | Ingest plain-text evidence: object store -> sandboxed extraction -> artifact + observation nodes, or a bounded rejection reason. |
| `POST` | `/v1/cases/{case_id}/assertions` | Record a user assertion (confirmed or not). |
| `POST` | `/v1/cases/{case_id}/evaluate?bundle=<key>` | Build a `CaseProjection` from durable case state, run the real `nexo_core::evaluate` against the named bundle (default `ley-25326`), render and record the result. |
| `GET` | `/v1/cases/{case_id}/evaluations` | List recorded evaluations, returning the same rendered JSON stored at evaluation time. |
| `POST` | `/v1/cases/{case_id}/preparations` | Create a deterministic local `draft_request` from a current supported evaluation; never sends it. |
| `POST` | `/v1/cases/{case_id}/preparations/{preparation_id}/export` | Materialize an owner-authorized preparation as a verifiable export and advance it to `exported`. |
| `GET` | `/v1/cases/{case_id}/preparations/{preparation_id}/export/manifest` | Return the independently verified export manifest. |
| `GET` | `/v1/cases/{case_id}/preparations/{preparation_id}/export/artifacts/{digest}` | Download one independently verified export artifact by its SHA-256 digest. |

### `POST /v1/cases/{case_id}/preparations`

Body: `{"evaluation_id": i64, "kind": "draft_request"}`. The API accepts
no recipient, endpoint, credential, or caller-supplied output bytes. It
rebuilds the current projection, requires the selected receipt to remain
supported and bound to the same action and input manifest, then stores the
prepared material with provenance. The prepared material is a real
human-readable document — `nexo_report::render_markdown` over the same
rendered result `GET .../evaluations` returns, not a raw JSON dump: a
"draft request" a person cannot open and read was never actually a draft
of anything. The response is
`{"preparation_id": i64, "kind": "draft_request", "status": "prepared"}`.
Repeating the same request while that preparation remains current is
idempotent and returns the existing preparation id.
If that existing material is already `exported`, the response preserves and
reports `exported`; it never presents an exported material as merely prepared.
The output digest is part of that identity: the same preparation key cannot
silently reuse a row for different bytes.

An old, unsupported, or stale evaluation returns `409`; an unsupported kind
returns `422`. This endpoint creates local material only. It has no delivery,
filing, signature, recipient, or transport capability.

### Export and download endpoints

`POST /v1/cases/{case_id}/preparations/{preparation_id}/export` uses the
server-configured `NEXO_EXPORT_ROOT`; the client supplies no filesystem path.
It is owner-authenticated, idempotent for an unchanged exported preparation,
and returns `{"preparation_id": i64, "status": "exported",
"manifest_digest": string, "artifact_count": usize}`. A missing, stale,
tampered, or non-prepared material fails closed.

The manifest endpoint returns the exact JSON consumed by `nexo-verify`. The
artifact endpoint accepts only a 64-character SHA-256 digest, checks that the
digest is named by the verified manifest, confines the resolved path to the
export root, and re-hashes the bytes before returning them. Clients can
reconstruct a standalone verifier input directory from these two endpoints;
the API never accepts a caller-controlled path or filename.

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
- **Bundle seed is idempotent.** `main.rs` calls `seed_ar_bundle`
  unconditionally at startup, but the helper serializes concurrent starts and
  returns the existing activated row for the exact fixture identity. A restart
  therefore does not create a new policy version or invalidate unchanged
  preparations.
- **Bundle provenance is homed on a dedicated bootstrap case,** because
  `provenance_records.case_id` is `NOT NULL` in the schema and bundle
  provenance is not naturally case-scoped. Not a real user case; never
  returned by any case-listing endpoint (none exists yet).
- **Single-owner token model,** see Authentication above.

## Report downloads

`GET /v1/cases/{case_id}/evaluations/{evaluation_id}/report?format=md|html|pdf`
(default `html`) downloads a human-readable report over one already-recorded
evaluation — `Content-Disposition: attachment`, so a browser saves it
directly. Implemented in `crates/nexo-report`, whose design (not code) is
adapted from Anna Tchijova's `zaynor/src/zaynor/report.py`
(Apache-2.0) — confirmed by reading that file in full before writing this
one. What was reused is the *idea*, not the implementation, because the two
sealed shapes differ (ZAYNOR's `ZaynorAuthoritativeResult` carries
per-finding MITRE technique IDs and an agent-pipeline table NEXO's
`ActionEvaluation` has no equivalent of): a report is a read-only
projection over an already-sealed value, never a second decision; every
report names the exact digest of what it projects; and chain-of-custody
carries two distinct hashes — `result_sha256` (bit-for-bit deterministic,
recomputable by re-fetching `GET .../evaluations` and hashing) and
`report_hash` (folds in when the report was rendered, so two reports of
the same sealed evaluation stay distinguishable without implying the
*evaluation* changed).

**PDF** (`?format=pdf`) renders through `printpdf`'s HTML-to-PDF mode —
the pure-Rust sibling of ZAYNOR's `reportlab` choice, no external binary or
headless browser — feeding it the exact same markup `render_html` produces,
so all three formats can never structurally drift from each other; only
page setup and PDF-specific metadata are PDF-only code. Gated behind the
`pdf` Cargo feature (`nexo-api` enables it; a build that omits it returns
`501 Not Implemented` rather than failing to compile or panicking),
mirroring ZAYNOR's own optional `[report]` extra for `reportlab` — the
dependency tree (font shaping, layout) has no reason to be mandatory for
every consumer of `nexo-report`.

The hash is embedded twice in a PDF, not once: as visible text (the same
chain-of-custody block every format carries, plus a footer line printpdf
repeats on every page) and in the PDF's own document metadata (`Subject`,
`Keywords`, `Identifier`) — verified with `pdfinfo`/`pdftotext` against a
real generated file, not just asserted. One nuance worth recording: the
metadata *label* ("result_sha256") is only recoverable by a real PDF parser
(the value sits inside an encoded PDF string object) — a plain `grep` on
the raw file bytes will not find that literal word. The *digest value*
itself, however, does appear as plain ASCII in the raw bytes (confirmed by
byte search against a real generated PDF), which is what
`nexo-api/tests/api_test.rs`'s PDF test actually asserts, rather than
overclaiming what a naive byte search can recover.

**A real bug found and fixed while adding this feature, not before:**
`nexo-app::upsert_digest` used `INSERT ... ON CONFLICT (algorithm, hex) DO
UPDATE SET algorithm = EXCLUDED.algorithm` to fetch an existing digest's id
on conflict — a common Postgres idiom, but `digests` rows are immutable by
an *unconditional* trigger (`digest_rows_are_immutable_trigger`, added in
an earlier round — unlike the `tools`/`tool_versions` identity triggers,
it fires on any UPDATE, not only one that changes a value). Two evidence
uploads that hash to the same digest — a realistic scenario, not a corner
case — raced on this and one lost with a 500. Confirmed by induction: a
new report-download test happened to reuse another test's exact evidence
text, and the existing end-to-end test started failing intermittently
under concurrent test execution; isolating with `tracing_subscriber` in
the test binary showed the real Postgres error text
("digest rows are immutable") rather than a vague timeout, which is what
actually pointed at the trigger instead of resource contention. Fixed by
rewriting `upsert_digest` to a `DO NOTHING` + fallback `SELECT`, which
never issues an UPDATE against this table at all. Regression test:
`concurrent_upsert_of_the_same_digest_never_fails` (8 concurrent upserts
of byte-identical content, all resolving to the same row, none failing).

## Non-goals (this round)

- A generic multi-bundle import/selection surface.
- The web UI (Step 6).
- `evidence_package` and `export` as directly requestable `preparations`
  kinds — `nexo_core::PreparationKind` defines all three
  (`docs/PREPARATION_CONTRACT.md`), but this endpoint only accepts
  `draft_request` today; requesting `evidence_package` returns `422`.
