# Policy bundle contract

## Purpose

This contract defines the immutable, versioned policy unit that may be supplied
to a future pure evaluator. It selects normative material; it does not fetch
sources, interpret arbitrary prose, inspect case artifacts, or make an action
available.

## Threat model

An attacker can provide a forged, stale, duplicated, out-of-jurisdiction, or
partially migrated bundle through an import adapter. They can also reorder
claims and source references, replay an older bundle, and submit an unknown
schema version.

At this layer an attacker cannot alter the compiled policy kernel or make it
read an ambient clock. Persistence, activation authority, source capture, and
cryptographic verification remain outside the pure domain function and must be
passed in explicitly.

## Bundle identity

Each bundle has:

```text
PolicyBundle
  id                  stable graph identity
  schema_version      serialized-shape version
  policy_version      human/reference version of the legal policy
  jurisdiction        explicit jurisdiction code
  validity            closed CivilDate interval; open end is explicit
  claims              bounded, deterministically ordered claim references
  source_policy       allowed authority/channel classes for this bundle
  captured_artifact   exact immutable bundle bytes
  digest              seal of the captured artifact
  provenance          acquisition and collector reference
```

`VerifiedCaptureAttestation` is an adapter capability accepted by this
dependency-free kernel; it is not a cryptographic proof generated here. The
application/integrity boundary must verify that the captured bytes match the
digest before supplying the attestation. A caller that forges this status is
already outside the stated trust boundary, but the API and documentation must
keep that assumption visible.

The persisted bundle row enforces the same relation: its digest must equal the
captured-artifact digest, rather than merely referring to another existing
digest row.

`schema_version` answers “how is this bundle serialized?” and must not be
confused with `policy_version`, which answers “which legal policy revision is
this?” A newer unknown schema is rejected for evaluation, never silently read
as the current shape. Historical readers may migrate known older schemas and
must preserve the original version and digest.

## Selection contract

The pure selector receives:

```text
select(bundle_set, requested_jurisdiction, reference_date)
```

It returns exactly one of:

```text
Selected(bundle_id, policy_version)
NoBundle
OutOfJurisdiction
PolicyNotCurrent
AmbiguousSelection
```

Selection is deterministic: the same immutable bundle set, jurisdiction, and
reference date produce the same result regardless of insertion order. A bundle
is eligible only when:

```text
bundle.jurisdiction == requested_jurisdiction
reference_date ∈ bundle.validity
schema_version is known and migrated successfully
bundle digest/capture verification has succeeded
```

The selector never chooses “the newest” by wall-clock arrival, vector order,
or lexical accident. If two eligible bundles remain incomparable under the
declared replacement/version ordering, it returns `AmbiguousSelection` and
fails closed.

## Source policy boundary

`source_policy` states which `AuthorityKind` and `AcquisitionChannel` classes
may support claims in this bundle. These are eligibility predicates, not a
universal ranking of legal force. Jurisdiction-specific semantics belong in the
bundle data and its documented policy version.

An eligible bundle still does not prove that a claim is legally correct. Before
an evaluator uses a claim, every referenced `NormativeSource` must resolve,
pass the capture-digest invariant, and satisfy this bundle's source policy.
No source reference may be upgraded because an agent, prompt, or secondary
summary says it is authoritative.

## Bounds and invariants

- Bundles, claim references, source-policy entries, and serialized fields are
  bounded before allocation at the adapter boundary.
- Claim references are non-empty and unique by `NormativeClaimId`; ordering is
  canonicalized before sealing.
- The bundle is immutable after construction. Activation/revocation is an
  application transaction that selects which immutable bundle set is visible;
  it does not mutate a historical bundle.
- `reference_date` is supplied by the caller and recorded with the evaluation;
  the selector has no ambient clock.
- A digest authenticates the captured bytes' identity, not the truth or legal
  weight of the claims inside them.

## Failure semantics

| Condition | Result |
| --- | --- |
| Requested jurisdiction has no bundle | `NoBundle` |
| Matching bundle is outside its validity interval | `PolicyNotCurrent` |
| Bundle jurisdiction differs from request | `OutOfJurisdiction` |
| Unknown/newer schema or failed migration | reject; never evaluate |
| Capture digest mismatch | reject before selection (at the integrity boundary) |
| Two eligible incomparable bundles | `AmbiguousSelection` |
| Claim source absent or ineligible | evaluator-level `ABSTAIN`, not `INSUFFICIENT_FACTS` |

`INSUFFICIENT_FACTS` remains exclusively a case-factual result after a current,
in-scope, source-complete legal route has been selected.

## Non-goals

- No legal advice, action evaluation, or requirement satisfaction.
- No web/API acquisition or source parsing.
- No persistence, activation authorization, or concurrent update protocol.
- No universal ontology of legal hierarchy, precedent, or bindingness.

## What would falsify this contract

- Two insertion orders produce different selection results.
- A stale or unknown-schema bundle can be selected as current.
- A digest mismatch reaches selection output.
- A caller can supply `Verified` without a preceding integrity verification
  step and the application fails to reject it at its boundary.
- A prompt/model output can make an ineligible source eligible.
- A missing normative source is surfaced as `INSUFFICIENT_FACTS`.
