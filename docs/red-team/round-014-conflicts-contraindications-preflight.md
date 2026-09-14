# Security Audit — Conflicts and contraindications preflight

## Red Team Round 14 — integration audit

**Date:** 2026-09-14  **Method:** adversarial constructor analysis
**Base:** `main @ 061c64a`
**Scope:** legacy constructors, relational evidence, and the deterministic
policy producer.

## Threat model

- Caller controls claim ids, factual-support ids, and ordering.
- Caller cannot bypass private fields or make an absent graph node resolve.
- A claim's legal meaning is not inferred from its identifier or display text.

## Findings

### RT-041 — Distinct claims are not necessarily conflicting

**Level:** CODE FACT. The current `ConflictingLegalClaims::try_new` accepts any
two distinct `NormativeClaimId` values. The constructor enforces cardinality and
identity only; it has no conflict witness or relation. A claim about access and
a claim about deletion can therefore be labelled “conflicting” merely by being
listed together.

**Required remediation:** replace the vector-only shape with a claim pair bound
to a deterministic conflict witness.

### RT-042 — Contraindication vectors can be unrelated

**Level:** CODE FACT. `Contraindicated::try_new` accepts non-empty factual and
legal vectors independently. Nothing binds a particular fact to the legal
ground that supposedly contraindicates the route. The type can therefore carry
two individually valid supports while making an unsupported combined claim.

**Required remediation:** require a typed fact/ground/rule-match relation.

### RT-044 — Witness minting authority is part of the proof

**Level:** CODE FACT. An opaque `ConflictWitnessId` or `RuleMatchId` supplied
by an adapter would be forgeable: a caller could pair valid but unrelated ids
and manufacture a negative result. The proposed implementation therefore keeps
rule-match fields private and exposes minting only as `pub(crate)` constructors
for the policy rule engine. The engine must receive deterministic claim/fact
inputs, rule identity, and version; the witness constructor checks that the
relation repeats those identities exactly.

The current slice intentionally has no public minting API. Its tests exercise
the relation guards from inside the crate; the next evaluator integration must
make the rule engine the sole production producer and add an end-to-end test
that an adapter-supplied id cannot enter a `NonActionable` result.

The authority is deliberately narrower than a cryptographic proof: `pub(crate)`
means every module in `nexo-core` can currently call the minting functions,
but external crates cannot. `PolicyRuleId::new` is public because an id is only
an identifier; it carries no claim that a rule matched. The constructors bind
the deterministic identities (claim pair, or fact/ground/trigger tuple) and
reject self-pairs or mismatched witnesses. They do **not** independently prove
that the policy rule is legally correct, current, or applicable. That remains a
residual obligation of the future policy-rule engine, which must be the only
production caller and must include bundle/version and rule inputs in its own
auditable evaluation record.

This distinction prevents a stronger but false claim such as “a valid rule id
proves a conflict.” It proves only that a trusted in-crate producer asserted a
specific relation over specific ids; semantic correctness still requires the
producer's deterministic policy evaluation and source-backed inputs.

### RT-043 — Existing guards remain useful

**Level:** CONFIRMED BY INDUCTION from the existing suite. Single-claim
conflicts, duplicate claims, empty factual context, and empty legal context are
rejected. Those guards must survive the relational redesign; they are not a
substitute for the missing semantic binding.

### RT-045 — Legacy vector API was a confirmed bypass, now fail-closed

Before integration, the vector constructors accepted unrelated bounded vectors;
the prior tests captured those successful constructions. After integration,
both public legacy entry points return `RelationalEvidenceRequired`
unconditionally. The only in-crate constructors accept relational evidence.

### RT-046 — `pub(crate)` alternative minting path

`pub(crate)` is a crate boundary, not a proof that only one module can call a
constructor. A future module inside `nexo-core` could call it. The supported
production producer is `PolicyRuleEngine`, whose constructor is also
crate-private: the bundle loader, not an adapter, supplies the deterministic
relation tables. It requires the tuple to be present, then attaches bundle
identity, bundle version, and rule identity. Direct constructor calls are guard
tests, not production paths. External crates cannot call the engine or assemble
private fields, pinned by the `ConflictWitness` compile-fail doctest.

## Discarded vectors

| Vector | Result | Reason |
| --- | --- | --- |
| Treat two ids as a contradiction | FALSIFIED as a design assumption | Identity does not establish incompatibility. |
| Public witness constructor | REJECTED | It would let any caller certify an arbitrary relation. |
| Let an inference be the factual side | FALSIFIED by enum shape | `FactualSupport` has no `Inference` variant. |
| Use prose explanation as witness | Rejected | Narration has no normative authority. |

## Exit criteria for implementation

Typed relations, engine-produced matches, end-to-end construction, and the
legacy fail-closed tests now pass. The remaining limit is semantic: the engine
proves only membership in its configured deterministic relation table; it does
not establish universal legal truth, hierarchy, or freshness beyond the
attached bundle record.
