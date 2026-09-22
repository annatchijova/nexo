# Application layer contract

## Purpose

The application layer (`nexo-app`) is where `nexo-core`'s pure, dependency-free
domain types become durable state. It owns transactions, actor identity and
ownership, storage capabilities, and the recording of every policy-bundle
identity against the evaluation it produced. It contains no domain logic of
its own: every invariant already enforced by `nexo-core` (reference
integrity, the evidence/interpretation boundary, bounded fan-in, closed
evaluation sum types) is enforced once, in the core, and never re-implemented
or weakened here. This layer's job is narrower and just as strict: make core
state durable without adding a second, looser copy of its rules.

This contract governs `nexo-app` only. It does not evaluate actions, select
policy, acquire sources, or parse artifact bytes; those remain the
responsibility of `nexo-core`, the policy layer, and the sandbox worker
(`docs/SANDBOX.md`) respectively.

## Status

Schema in `crates/nexo-app/migrations/` implements this contract's
structural rules. `crates/nexo-app/src/repository.rs` implements the
transaction contract (§ below) against that schema: actor/case creation,
case-graph node insertion with in-transaction sequential id allocation
under a locked case row, policy bundle/claim/route/evaluation/
insertion. Covered by `crates/nexo-app/tests/repository_test.rs`
(`scripts/test_repository.sh`), including a concurrency test that fires 16
simultaneous node-insertion transactions against one case and asserts the
resulting ids are exactly `1..=16` with no collision and no gap.

Not yet implemented: authorization *enforcement* (the repository layer
exposes `case_owner` so a caller can check it, but does not itself refuse
a query from a non-owning actor — that belongs to the HTTP API layer,
which sees the authenticated caller); preparation/export repository
functions; and revoking `UPDATE`/`DELETE` from the runtime database role
(deployment-time hardening, Step 9).

## Threat model

An attacker with network access to the API (not yet built; see Step 5 of
`plan.md`) can attempt to:

- read or write a case belonging to another actor;
- commit a partial graph mutation by aborting mid-command (client
  disconnect, crash, concurrent conflicting write);
- cause two evaluations to be persisted against different policy-bundle
  identities while appearing to reference "the same" evaluation;
- replay an old, already-superseded policy-bundle activation;
- insert a node whose id collides with, or is assigned out of order from,
  an existing node in the same case, corrupting reference integrity that
  `nexo-core` already validated in memory.

An attacker with direct database access is out of scope for this layer
(covered by deployment hardening in Step 9 of `plan.md`); this contract
assumes the database is reached only through the repository functions
described below.

## Actor and ownership model

Every case has exactly one owning `Actor` from creation. There is no
ownerless case, and no case is ever readable or writable by an actor other
than its owner without an explicit, auditable sharing grant — which does not
exist yet and is not implied by any code in this layer. Single-user today
does not mean unauthenticated: the owner is recorded and checked on every
command, so adding a second actor later is a matter of issuing a second
identity, not rewriting the access model.

An actor's `external_identity` is the authentication lookup key and is
immutable after creation; identity rotation requires a separate audited
account-management operation. Actor and case creation timestamps are likewise
immutable historical fields.

The database also rejects reassignment of `owner_actor_id` after case
creation; changing access requires an explicit future sharing model, not a
mutation of the case root.

User assertions are likewise bound to that immutable case owner; another
actor cannot be attributed as the declaring identity without a future audited
sharing model.

`ActorId` here is the same opaque identity type referenced by
`UserAssertionNode` in `nexo-core`; the application layer is what gives that
id a durable row and an external identity (how the actor authenticates),
which the core deliberately does not know about.

## Transaction contract

Every application command — create case, add evidence, record an ingestion
result, activate a policy bundle, run an evaluation, record a preparation,
build an export — executes inside exactly one database transaction and
either commits every write it makes or leaves none of them. Specifically:

1. **Atomic node insertion.** A `CaseGraph` mutation that would insert
   several rows (a node plus its typed payload table, plus any derivation or
   inference input rows) commits all of them together or none. A client
   disconnect or crash mid-command must never leave a `case_nodes` row
   without its typed payload row, or a payload row referencing an
   uncommitted node.
2. **Sequential node ids are assigned inside the transaction**, from the
   same source of truth `nexo-core::CaseGraph::insert` uses (next available
   id = current count + 1), under a row lock scoped to the case so two
   concurrent commands against the same case cannot assign the same id.
   `nexo-core`'s reference-integrity checks assume this ordering; the
   database is what has to keep the promise once concurrency exists.
3. **Explicit policy-bundle identity on every evaluation.** An
   `action_evaluations` row always carries the exact `policy_bundle_id` and
   its digest that produced it, written in the same transaction as the
   evaluation result. There is no code path that writes an evaluation result
   without also writing which bundle produced it; per
   `docs/POLICY_BUNDLE_CONTRACT.md`, a bundle is immutable once activated, so
   this pairing can never point at a bundle that has since changed under it.
4. **Policy-bundle activation is append-only.** Activating a new bundle
   version inserts a new `policy_bundle_activations` row; it never updates or
   deletes a prior activation row. "Which bundle was active for jurisdiction
   X at time T" is answered by querying activation history, not by mutating
   a "current" pointer that would make past evaluations ambiguous about
   which bundle they actually saw.
5. **Failed commands are invisible.** A rolled-back transaction leaves no
   partial row anywhere reachable by a subsequent read. This is verified in
   `crates/nexo-app`'s test suite by deliberately aborting a multi-row
   command mid-transaction and asserting the case graph is unchanged.

## What is normalized vs. sealed as typed payload

Every structural fact `nexo-core` already validates at insertion time is
normalized into its own relational columns: node kind, node references,
actor references, tool/version references, confirmation states, confidence
bounds. This is what makes reference-integrity queries (does this
`Observation` really point at an `Artifact` in the same case?) and
audit/export tooling possible without deserializing anything.

`ActionEvaluation` results are the one exception, by design, not convenience:
the evaluator (`nexo-core::evaluator`) is a pure function that has already
produced a closed, typed, immutable result before this layer ever sees it.
Re-normalizing every field of every `ActionEvaluation` variant into its own
columns would let a future migration silently reinterpret an already-decided
result — exactly the failure mode `docs/ARCHITECTURE.md` rules out for
explanations. Instead:

- Top-level columns (`case_id`, `route_action_id`, `policy_bundle_id`,
  `result_kind`, `action_status` or `non_actionable_variant`, `evaluated_at`)
  are normalized, because queries and indexes need them.
- The full typed result — every factual/legal support reference, every
  satisfied and unmet requirement, every negative-evidence witness — is
  stored as a canonical, versioned JSON payload (`result_payload`,
  `result_schema_version`) that round-trips losslessly back into the
  `nexo-core` type it came from. The payload is written once, in the same
  transaction as the top-level columns, and is never edited in place; a
  reevaluation produces a new row, never a mutated one.

This mirrors the "explanation is a deterministic projection, never authority"
rule in `docs/ARCHITECTURE.md`: the JSON payload here is not a second,
looser decision — it is the sealed record of a decision `nexo-core` already
made, kept intact so a verifier can reread exactly what was decided.

## Storage capability boundary

`nexo-app` never opens, parses, decompresses, or executes artifact bytes.
Its only interaction with artifact content is:

- requesting the sandbox worker (`docs/SANDBOX.md`) run against a
  content-addressed object already written to the integrity layer's object
  store, and
- recording the typed result (an `Observation` payload or a bounded failure
  record) the worker returns.

`nexo-app` holds object-store write capability; a sandboxed extraction
worker process never does (`docs/SANDBOX.md`).

## Schema

Implemented in `crates/nexo-app/migrations/0001_init.sql`. Summary of the
table groups and the invariant each enforces beyond what a foreign key alone
gives:

- **`actors`** — one row per identity; `cases.owner_actor_id` is `NOT NULL`
  so no case can exist without an owner.
- **`cases`** — one row per case; all graph state is scoped by `case_id`.
- **`digests`, `ingestion_records`** — the application-layer half of
  `ArtifactId`/`DigestId`/`ProvenanceId`: content digest, declared (untrusted)
  metadata, and acquisition provenance, kept separate from the sandbox
  worker's typed extraction result. Digest identities are immutable after
  insertion, and provenance records are immutable after insertion. Once an
  ingestion row is bound to an artifact, its declared identity fields are
  immutable; only its sandbox status may transition. Artifact insertion also
  requires matching case and byte-size identities across ingestion and
  provenance.
- **`tools`, `tool_versions`** — `UNIQUE (tool_id, version)`, mirroring
  `ToolVersion`'s "version zero is unrepresentable" rule: version is
  `NOT NULL` and constrained `> 0`; identity fields are immutable after
  insertion.
- **`case_nodes`** plus one payload table per `NodeKind`
  (`artifact_nodes`, `observation_nodes`, `user_assertion_nodes`,
  `derived_fact_nodes`, `inference_nodes`) — `case_nodes` is the kind
  discriminator and the id-allocation source of truth; every payload table's
  primary key is `(case_id, node_id)` and foreign-keys back into
  `case_nodes` so a node can never exist without exactly one typed payload
  row of the matching kind. A deferred database constraint checks that the
  payload exists at transaction commit, while immediate triggers reject a
  payload whose table does not match the discriminator, and the discriminator
  is immutable after node creation; all graph nodes, payloads, and input edges are
  append-only.
- **`derived_fact_inputs`, `inference_inputs`** — ordered input edges
  (`ordinal`), `UNIQUE (case_id, node_id, ordinal)` so input order is
  reconstructible exactly as declared. Database triggers enforce that
  derivations consume only factual-support nodes and that all edges point to a
  prior node, preserving the acyclic sequential graph. The application layer
  and deferred database constraints enforce non-empty input sets and the row-
  count bounds mirroring `MAX_DERIVATION_INPUTS`/`MAX_INFERENCE_INPUTS`.
- **`policy_bundles`, `policy_bundle_activations`** — bundle identity is
  immutable after activation; `policy_bundle_activations` is append-only, per
  the transaction contract above. The database rejects activation updates and
  deletes so an old activation cannot be erased to unlock historical mutation;
  each bundle also has at most one activation event.
- **`normative_sources`, `normative_claims`, `normative_claim_sources`** —
  mirrors `docs/POLICY_BUNDLE_CONTRACT.md`'s bundle identity fields exactly;
  `normative_claim_sources` requires at least one row per claim through a
  deferred database constraint, so a claim and its source edge may still be
  assembled atomically.
- **`action_routes`** — the durable form of `ActionRoute`: which claims and
  mandatory requirements a route requires. Once its bundle is activated, the
  database also freezes its requirement rows; route-claim ordinals are unique
  within each route.
- **`action_evaluations`** — top-level columns plus the sealed JSON payload,
  as described above.
- **`evaluation_receipts`** — application-owned binding evidence for a
  `SUPPORTED` evaluation: durable evaluation/action/case/bundle identities,
  result and input-manifest digests, and schema/version fields. These receipts
  are not preparation capabilities by themselves and are never created for a
  non-actionable or conditional evaluation.
- **`preparations`** — `DraftRequest` / `EvidencePackage` / `Export` records,
  referencing the `action_evaluations` row whose `Actionable` result they
  were prepared from; a preparation can never reference a
  `NonActionable` evaluation, enforced at the application layer at
  preparation-insert time.
- **`audit_log`** — append-only, hash-chained event record (Step 2 of
  `plan.md` defines the chain itself in `nexo-integrity`); this layer only
  guarantees every command that mutates state also appends exactly one audit
  row, in the same transaction, and a database trigger rejects updates and
  deletes.

## What this layer explicitly does not do

- It does not decide whether an `ActionOption` is `AVAILABLE`; it stores
  what `nexo-core` already decided.
- It does not select a policy bundle for a jurisdiction; it records which
  bundle an evaluation used.
- It does not verify a capture digest; it stores the digest and the
  `VerifiedCaptureAttestation` result the integrity layer already computed.
- It does not grant a second actor access to a case; that requires an
  explicit sharing feature this contract does not yet define.
