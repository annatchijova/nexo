# Case graph contract

## Purpose

The case graph is NEXO's typed domain record of a case: what was submitted,
what was extracted, what was declared, what was derived, and what is merely
interpreted. This contract governs node kinds, their provenance obligations,
and the reference rules between them. It does not evaluate actions, select
policy bundles, acquire sources, or perform I/O. Its job is to keep artifact,
extraction, declaration, derivation, and interpretation distinct, and to make
any provenance gap visible before material from this graph can reach an
action evaluation.

## Status

Proposed for the evidence-graph layer (README layer 2). Its smallest
complete slice is implemented in `crates/nexo-core/src/case_graph.rs` with
the adversarial and property tests required by `docs/DEVELOPMENT_CYCLE.md`;
the evaluator, persistence mapping, and extraction paths remain unstarted.

## Threat model

An attacker can submit hostile artifacts with crafted filenames, metadata, and
embedded text; declare contradictory or false assertions; and, through a
future adapter, attempt to insert:

- observations that reference no artifact or name no extractor;
- derived facts whose inputs were never declared;
- inferences presented as observations, or smuggled into derivation inputs;
- reference cycles through derived facts;
- unbounded fan-in intended to exhaust memory in retained graph state.

Retrieved official bytes and every agent or model output remain untrusted
input at this boundary, as elsewhere.

At this layer an attacker cannot read clocks, files, or network through the
domain core, and cannot make an `Inference` into `FactualSupport`; that
boundary is structural (ADR 0001 and the compile guard in
`crates/nexo-core/src/property_tests.rs`).

## Node kinds and required provenance

| Node | Meaning | Required provenance |
| --- | --- | --- |
| `Artifact` | Original bytes submitted to NEXO. | Content digest reference, byte size, ingestion record reference, source provenance reference. |
| `Observation` | A direct extraction from an artifact. | Artifact reference, extractor identity and version, precise locator, recording instant. |
| `UserAssertion` | A statement declared by a person. | Actor identity, recording instant, confirmation state. |
| `DerivedFact` | A reproducible transformation over declared inputs. | Non-empty input references, transformation identity and version. |
| `Inference` | An interpretation not directly observed. | Non-empty input references, method identity and version, bounded confidence bound. |

Every provenance field is mandatory at construction. A missing field is a
typed construction error, never a silent default. The table elaborates the
ontology row in `docs/ARCHITECTURE.md`; it does not reassign meaning.

## The evidence/interpretation boundary

Factual support kinds are exactly the graph kinds whose provenance can
explain a case fact: `Artifact`, `Observation`, `UserAssertion`,
`DerivedFact`. `Inference` is a graph node and nothing more:

- `Inference` is not a `FactualSupport` variant and never becomes one by
  being referenced or displayed.
- `DerivedFact` inputs are restricted to factual-support kinds. A derivation
  that consumed an interpretation would launder it into a fact; the
  transformation's reproducibility cannot repair its input's epistemic
  status. This decision is recorded in ADR 0005.
- `Inference` inputs may be any graph node, including another inference.
  Interpretations may build on interpretations; they remain interpretations.

## Reference rules

1. Every reference must resolve to an existing node of a kind permitted for
   that edge. Dangling references and kind mismatches are construction
   errors.
2. An `Observation` references exactly one `Artifact`.
3. A `DerivedFact` references at least one input, every input is a
   factual-support kind, and the reference subgraph reachable through
   derivation inputs is acyclic. Reproducibility requires a well-founded
   input order; a cycle makes the transformation unreproducible by
   construction. Under sequential node-id assignment a cycle is structurally
   unreachable; the check remains as defense for any future bulk-load path.
4. An `Inference` references at least one input of any node kind.
5. Two nodes with identical content remain distinct nodes. Identity is the
   assigned `NodeId`; the core never deduplicates.

## Time

The core reads no ambient clock. Every instant is a `UtcInstant` supplied by
an adapter; every calendar date is a `CivilDate`. Recording instants are
provenance data, not ordering guarantees: durable ordering belongs to the
persistence layer. An adapter that cannot supply a time fails closed at the
adapter boundary; the core never invents one.

## Cardinality bounds

Consistent with the existing `MAX_*` discipline, every reference collection
is bounded at construction: derivation inputs are bounded by
`MAX_DERIVATION_INPUTS` (64) and inference inputs by
`MAX_INFERENCE_INPUTS` (64). Adapters remain responsible for enforcing byte
and body
limits before allocation, as stated in `nexo-core`; these bounds cap retained
domain state. Graph depth is not bounded at the core: acyclicity plus
bounded fan-in is sufficient for deterministic traversal, and a depth cap
would truncate legitimate case structure arbitrarily.

## Content and bytes

The core never receives, hashes, opens, or parses artifact bytes. An
`Artifact` node carries a typed reference to the content digest recorded by
the ingestion path (which uses `nexo-integrity`) plus the byte size; digest
bytes live in the application and object layers. This follows the
representation idiom already established in `nexo-core`, where provenance is
carried as typed node references (`ArtifactId`, `DigestId`, `ProvenanceId`,
`NonEmptyText`) rather than embedded payloads. Equality between digest and
stored bytes is an application-layer obligation, mirroring the capture-digest
invariant of `docs/POLICY_SOURCE_CONTRACT.md`. Extraction
results enter the graph only as typed records produced by the sandbox worker
of `docs/SANDBOX.md`; an extractor failure becomes a typed failure record,
never an `Observation`.

## Failure states

| Condition | Required construction result |
| --- | --- |
| Any required provenance field missing | Unrepresentable: payload fields are required constructor arguments, so no node can be assembled without them. Absence is a compile-time guarantee, not a runtime error. |
| Reference to a nonexistent node | `DanglingReference` error. |
| Reference to a node of the wrong kind | `ReferenceKindMismatch` error. |
| `DerivedFact` with an `Inference` input | `InferenceInDerivationInputs` error. |
| Cycle reachable through `DerivedFact` inputs | `DerivedInputCycle` error. |
| Empty derivation or inference inputs | `EmptyInputs` error. |
| Any input collection above its bound | Typed over-limit error. |

No condition above may degrade into a partially constructed node, a
placeholder provenance value, or a successful construction with warnings.

## Prohibited substitutions

Prompt text, agent configuration, user preference, and workflow
playbook may not create or alter graph nodes. Explanation is a deterministic
rendering of an already-authorized graph projection; it has no write path
into this contract. A node exists only through the validated constructors
defined here.

## Relationship to evaluation

The graph is evaluated; it does not evaluate. A future pure evaluator
consumes a graph projection, a policy bundle, and a reference time, and
returns `ActionEvaluation`; the closed sum type of ADR 0002 is unchanged.
`Inference` alone cannot turn a non-supported route into a supported one,
because it cannot enter `factual_support` at all.

## Non-goals

- Not a persistence schema; PostgreSQL mapping belongs to the application
  layer.
- Not an extractor and not a parser; artifact bytes stay behind the sandbox
  boundary.
- No confidence arithmetic. Confidence on `Inference` is a finite, discrete,
  ordered bound — never a float. The initial scale is `Low < Medium < High`,
  ordinal only; changing it requires ADR-level review.
- No graph query language, no merging or deduplication semantics, no
  jurisdiction or policy decisions.

## Adversarial cases the implementation must cover

At minimum, the implementation's test suite must fail against a deliberately
broken version of each guard: an observation with no artifact, a derived fact
with an undeclared or inference-typed input, a two-node derivation cycle,
fan-in one above each bound, every provenance field absent in turn, and an
attempt to assemble an `ActionOption` from inference-only context. Property
tests must hug each bound, as in the existing `property_tests.rs`.

## Residual limits

The cardinality bounds themselves are asserted, not derived. Graph depth is
unbounded by design. Confidence semantics are a placeholder. Nothing in this
contract establishes the truth of any declaration — only its typed,
inspectable presence.
