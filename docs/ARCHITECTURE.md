# NEXO architecture

## Decision boundary

NEXO treats an action option as a conclusion with two independent support paths:

```text
ActionOption
  ├── factual_support → provenance → Artifact | UserAssertion
  └── legal_support   → NormativeClaim → official source + locator
```

The action evaluator is pure and deterministic. Persistence, HTTP, object storage, time acquisition, and model calls are outside it.

## Domain ontology

| Node | Meaning | Required provenance |
| --- | --- | --- |
| `Artifact` | Original bytes submitted to NEXO. | Content hash, ingestion metadata, source provenance. |
| `Observation` | A direct extraction from an artifact. | Artifact reference, extractor ID/version, precise locator. |
| `UserAssertion` | A statement declared by a person. | Actor, recording time, confirmation state. |
| `DerivedFact` | A reproducible transformation over declared inputs. | Input references, transformation ID/version. |
| `Inference` | An interpretation not directly observed. | Input references, method/version, bounded confidence semantics. |
| `NormativeClaim` | A jurisdiction-specific legal or policy assertion. | Primary source, locator, jurisdiction, validity interval, policy version. |
| `ActionOption` | A possible action shown to the person. | Factual and legal support, requirements, state. |

## Action evaluation states

`ActionEvaluation` is a closed sum type:

```text
ActionEvaluation
├── Actionable(ActionOption)
│   ├── SUPPORTED
│   └── CONDITIONALLY_SUPPORTED
└── NonActionable
    ├── INSUFFICIENT_FACTS     → legal route + named missing requirements
    ├── CONTRAINDICATED        → factual context + legal grounds
    ├── OUT_OF_JURISDICTION    → jurisdiction evidence + policy-bundle identity
    ├── POLICY_NOT_CURRENT     → policy-bundle identity + freshness evidence
    └── ABSTAIN                → one typed abstention cause
```

An actionable route always has factual and legal support. A non-actionable
evaluation has the smaller, specific proof obligation appropriate to why no
route may be offered; it is never represented as a partially populated action.

`AVAILABLE` is a UI projection permitted only for a `SUPPORTED` action whose mandatory requirements are all satisfied.

## Verifiable invariants

For every visible actionable route:

```text
factual_support is non-empty
legal_support is non-empty
every factual support node has valid provenance
every normative claim has primary source + locator + jurisdiction + validity interval + policy version
unmet mandatory requirement => action is not AVAILABLE
Inference alone cannot change UNSUPPORTED into SUPPORTED
```

For every visible non-actionable evaluation, its sum-type variant requires the
auditable reason for that result. For example, `POLICY_NOT_CURRENT` requires a
policy-bundle identity and freshness evidence; it cannot silently become an
unstructured status plus optional fields.

The linguistic consequence is deliberate: Spanish, English, and Russian explanations may differ, but all must reference the same authorized graph support. Explanations can be discarded and regenerated without changing the epistemic state of the case.

## Layers and dependency rule

Dependencies point inward only.

```text
Web (TypeScript) → API/application (Rust) → domain core (Rust)
                                           ↘ policy (Rust)
                                           ↘ integrity protocol (Rust)
```

- **Protocol foundation:** typed values, canonical bytes, hashes, versions, and reference IDs.
- **Domain core:** graph construction, invariant validation, state evaluation, no I/O.
- **Policy:** parses versioned jurisdiction bundles and returns normative claims plus requirements; it does not own case data.
- **Integrity:** hashes artifacts and manifests; provides optional keyed audit-chain primitives; has no database authority.
- **Application:** owns transactions, access control, storage capabilities, imports, exports, and asynchronous work.
- **Adapters:** HTTP API, web UI, extraction adapters, source acquisition, optional LLM narration, and standalone verifier.

The API, not the domain core, may call an LLM. It may send only an `ExplanationInput` projection containing already-authorized action state, satisfied/missing requirements, and cited graph node IDs.

## Data ownership

- Object storage holds original artifact bytes addressed by content hash.
- PostgreSQL holds graph metadata, references, policy-version selection, and durable transaction state.
- A policy bundle is immutable once activated; its digest is recorded with every evaluation.
- An export contains a manifest that names every included artifact digest and policy bundle digest.

## Sandbox boundary

Artifact bytes remain untrusted forever. Parsing and extraction execute in a sandboxed worker boundary, never in the API request process. The Stage 0 contract is in [`SANDBOX.md`](SANDBOX.md).
