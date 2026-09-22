# Policy source contract

## Purpose

This contract governs the source side of normative support. It does not decide
what a rule means, fetch the web, or make an action available. Its job is to
prevent authority, acquisition, capture, and legal proposition from collapsing
into one mutable record.

## Entities

```text
NormativeSource
  authority_kind
  issuer
  locator
  acquisition_channel
  retrieved_at
  captured_artifact
  digest
  provenance

NormativeClaim
  proposition
  jurisdiction
  validity_interval
  policy_version

ClaimSourceSupport
  claim_ref
  source_ref
  support_role
```

`ClaimSourceSupport` is many-to-many. A source capture may support multiple
claims; a claim may need a statute, a regulation, and an official guidance
source. The relation itself is versioned in a policy bundle, not inferred from
prose at render time.

## Independent dimensions

### Authority kind

Authority answers why a source may support a normative claim.

```text
PRIMARY_OFFICIAL          enacted statute, regulation, court or regulator material
OFFICIAL_INTERPRETIVE     issuing authority guidance or official explanation
SECONDARY_ANALYSIS        commentary, practitioner analysis, academic material
UNVERIFIED                lead only; cannot support a normative claim
```

`authority_kind` classifies a source; it is not a universal function of legal
weight, hierarchy, bindingness, or precedential force. The eventual
jurisdiction policy determines which source kinds may support which claim
classes and with what legal effect. A `SECONDARY_ANALYSIS` item may discover a
primary source but does not become primary by being useful.

### Acquisition channel

Acquisition answers how NEXO obtained the representation.

```text
WEB_FETCH | OFFICIAL_API | USER_PROVIDED | RESEARCH_CONNECTOR | IMPORTED_BUNDLE
```

It is not an authority ranking. An official statute retrieved by `WEB_FETCH`
is still `PRIMARY_OFFICIAL`; a secondary newsletter retrieved via a trusted
research connector remains `SECONDARY_ANALYSIS`.

## Source-capture invariants

For every `NormativeSource` eligible for policy evaluation:

```text
issuer is present
locator is present and immutable for this capture
retrieved_at is recorded from the acquisition event
captured_artifact identifies the exact bytes reviewed
digest equals the captured artifact digest
provenance identifies acquisition channel and collector/version
```

The source object identifies a capture, not an eternal URL. A later retrieval
of the same locator with changed bytes is a new `NormativeSource` capture with
a new artifact/digest. It may supersede an older source in a later policy
bundle but must not overwrite historical evaluations.

The database enforces the digest equality above: a source row cannot bind a
different digest to its captured artifact reference. Source captures are
append-only from creation, not only after a policy bundle is activated.

## Claim invariants

For every `NormativeClaim` used as legal support:

```text
proposition is explicit and stable within its policy version
jurisdiction is explicit
validity interval is explicit, including an open end where applicable
at least one ClaimSourceSupport exists
every supporting NormativeSource is eligible under the bundle's source policy
```

The claim does not carry retrieval metadata, raw bytes, agent configuration,
or model text. It can reference more than one source without duplicating their
captures.

## Prohibited authority substitutions

The following may control workflow behavior but cannot satisfy normative
support by themselves:

```text
prompt
agent configuration
user preference
workflow playbook
secondary-source summary without its primary source
```

They may decide which question to show, which source to acquire next, or which
route to evaluate first. They cannot turn a proposition into a legal claim.

## Failure states

| Condition | Required evaluation consequence |
| --- | --- |
| No eligible source capture for the proposed legal route | `ABSTAIN` (`NoAuthoritativeLegalSource`); never ask the person for incident facts to repair NEXO's normative deficit. |
| Source capture exists but bundle currency cannot be established | `POLICY_NOT_CURRENT`. |
| Claim source exists but jurisdiction is not within bundle scope | `OUT_OF_JURISDICTION`. |
| Capture digest disagrees with artifact bytes | Reject the source/bundle before evaluation. |

`INSUFFICIENT_FACTS` is reserved for a different axis: an eligible, current,
in-scope legal route exists, but one or more case-factual predicates remain
unproven. It must never mean “NEXO lacks an eligible normative source.”

## Non-goals

- This is not a citation style guide.
- This does not decide legal hierarchy in every jurisdiction.
- This does not make a web fetch trusted or a URL permanent.
- This does not authorize external communication or filing.
