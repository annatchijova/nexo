# Security Audit — Case-graph nodes and validated insertion

## Red Team Round 9

**Date:** 2026-09-13
**Method:** contract-vs-code confrontation + executable insertion probes
**Base:** `arena/01a09d92-nexo @ dbd0209` (slice under review); remediations
land in `55ad258` on the same branch
**Scope:** `crates/nexo-core/src/case_graph.rs` with
`case_graph_tests.rs` and `case_graph_property_tests.rs`, against
`docs/CASE_GRAPH_CONTRACT.md` and ADR 0005. No evaluator, persistence,
adapter, or sandbox exists yet.

## Threat model

- Attacker CAN supply hostile node payloads through a future adapter:
  dangling and kind-wrong references, inference inputs smuggled into
  derivations, empty or over-bound input lists, blank locators, and
  attempts to create cycles.
- Attacker CANNOT alter the compiled core, read a clock through it, or
  bypass constructor visibility.
- This round tests structural admission rules, never the truth of any
  declaration.

## Executive summary

| ID | Level | Bucket | Finding |
| --- | --- | --- | --- |
| RT-017 | CONFIRMED BY INDUCTION → remediated | Contract/code coherence | The contract promised a typed error for missing provenance; the implementation makes absence unrepresentable. |
| RT-018 | CONFIRMED BY INDUCTION → remediated | Documentation accuracy | ADR 0005 located the inference-input rejection at construction; it fires at graph insertion. |
| RT-019 | CONFIRMED BY INDUCTION → remediated | Test quality | The observation kind guard was pinned by a single example test only. |
| RT-020 | CODE FACT | Adapter boundary | Unbounded text payloads and unbounded inference depth remain adapter responsibilities. |
| RT-021 | FALSIFIED | Cycle construction | A derivation cycle cannot be expressed through the public API. |
| RT-022 | FALSIFIED | Inference laundering | Inference-only material assembles no factual support. |

Evidence: the authoring sandbox has no Rust toolchain, so every probe below
was executed by the repository's own CI workflow (`cargo build`,
`cargo test --workspace --locked`, `cargo clippy -- -D warnings`), green at
the remediation commit. The cited test names are the executed inductions.

## RT-017 — Contract promised a runtime error the code cannot produce

The failure-state table read "Any required provenance field missing → typed
error naming the absent field." No such error variant exists: every payload
field is a required constructor argument, so absence is unrepresentable
rather than rejected. The implementation is stronger than the contract, but
a contract describing a nonexistent error misleads the next reader and the
next layer. Remediated by rewording the table row to state the compile-time
guarantee.

## RT-018 — ADR 0005 named the wrong boundary

The ADR said constructing a derivation with an `Inference` input "is a typed
construction error". `DerivedFactNode::try_new` checks presence and bounds
only; the kind rejection fires in `CaseGraph::insert`, where referenced
nodes exist to be checked. Remediated by rewording the decision to name the
actual boundary and the `InferenceInDerivationInputs` error.

## RT-019 — One example test carried the whole kind guard

`ReferenceKindMismatch` for observation targets was pinned only by
`observation_rejects_a_non_artifact_target`; the property suite probed
dangling references but never kind mismatches. Remediated by adding
`observations_never_target_non_artifacts`, which for an arbitrary generated
graph aims an observation at a guaranteed non-artifact node and asserts the
typed rejection.

## RT-020 — Unbounded text and depth remain an adapter obligation

`NonEmptyText` payloads (locators, and Codex's normative fields) and
inference-over-inference chains have no core-level size or depth bound.
This is consistent with the documented boundary: adapters must enforce byte
and body limits before allocation, and acyclicity plus bounded fan-in keeps
traversal deterministic. CODE FACT, not a defect of this slice; it becomes
an exploit only if a future adapter allocates untrusted input without
limits.

## RT-021 — Cycle construction is unrepresentable

`derivation_cycle_is_unrepresentable_under_sequential_assignment` attempts
the smallest loop: a derivation citing a node that does not exist yet. The
probe fails closed as `DanglingReference`, and sequential id assignment
makes every cycle attempt reduce to that shape. The `DerivedInputCycle`
check remains as defense for any future bulk-load path. Vector FALSIFIED.

## RT-022 — Inference laundering into factual support

Three independent guards were probed: the absent `Inference` variant of
`FactualSupport` (compile-time, pinned by the wildcard-less match in
`property_tests::support_kind`), `NodeKind::Inference.is_factual_support()
== false` (`node_kind_factual_support_membership_matches_the_boundary`),
and the insertion boundary rejecting inference inputs to derivations
(`derivation_rejects_an_inference_input` plus the
`derivations_never_accept_inference_inputs` property). Vector FALSIFIED at
every layer that exists so far.

## Residual risks

- No evaluator exists yet; the guarantee that interpretation cannot change
  an action state is structural (no conversion API) rather than evaluated.
- Byte, nesting, and depth limits are adapter obligations that must be
  implemented before any untrusted input reaches these constructors.
- Cardinality bounds (64) are asserted, not derived from a workload model.
- Repeated insertion runs the derivation-cycle closure each time; the
  resulting quadratic worst case is acceptable at graph sizes that fit this
  layer and must be revisited with persistence.
- Red-team rounds so far were performed by the same agents that implement
  the layers; independent review remains an open process obligation.
