# ADR 0005: Derivations cannot consume inferences

## Status

Proposed for the evidence-graph layer. Recorded alongside
`docs/CASE_GRAPH_CONTRACT.md`.

## Context

The case graph distinguishes factual-support kinds (`Artifact`,
`Observation`, `UserAssertion`, `DerivedFact`) from `Inference`. ADR 0001
already excludes `Inference` from `FactualSupport`. A remaining route for an
interpretation to reach factual support is indirect: a `DerivedFact` whose
declared inputs include an `Inference`. The derivation itself is
reproducible, but its output inherits the hypothesis in its input; presenting
that output as a case fact would launder interpretation into evidence while
keeping every provenance field formally populated.

## Decision

`DerivedFact` inputs are restricted to factual-support kinds. Inserting a
derivation with an `Inference` input fails closed with the typed error
`InferenceInDerivationInputs` at graph insertion, the boundary where
referenced nodes exist to be checked; payload constructors verify presence
and bounds only.
`Inference` inputs remain unrestricted over node kinds: interpretations may
build on interpretations, and the result stays an interpretation because
`Inference` has no path into `FactualSupport`.

## Consequences

- The epistemic boundary of ADR 0001 is closed transitively, not only at the
  support constructor.
- A legitimate workflow of "derive after interpreting" must record the
  interpretation separately and then declare the underlying facts as
  derivation inputs directly: the derivation cites evidence, the inference
  cites reasoning.
- If a future workflow needs hypothesis-conditioned derivation, it must
  arrive as a new explicit node kind and a new ADR, never by relaxing this
  guard.
