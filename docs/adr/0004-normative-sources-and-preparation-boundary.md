# ADR 0004: Separate normative sources from claims and preparations from acts

## Status

Accepted. This is a one-way-ish data-model decision; persisted claims and
exports will depend on it.

## Forces at the time

NEXO must explain a legal route through both facts and current primary sources.
One source may support many propositions; one proposition may need multiple
sources. The corpus reviewed in `claude-for-legal` usefully distinguishes source
tiers, but its labels conflate authority with acquisition in ways NEXO can model
more precisely. NEXO also must not let “generate an export” masquerade as a
legal right or silently execute that right.

## Decision

`NormativeClaim` holds a legal proposition, jurisdiction, validity interval,
policy version, and one or more `NormativeSource` references. `NormativeSource`
holds authority kind, issuer, locator, acquisition channel, retrieval time,
captured artifact, digest, and provenance.

`ActionOption` names a legal/rights route. `Preparation` names material NEXO
can create for that route (`DraftRequest`, `EvidencePackage`, `Export`). An
external act remains outside NEXO's authority boundary.

## Alternatives rejected

- Put source provenance on `NormativeClaim` — rejected: duplicates captures and
  conflates proposition, authority, acquisition, bytes, and observation time.
- Copy Claude for Legal's source labels 1:1 — rejected: `WEB_FETCH` describes a
  channel, not authority; an official statute fetched from its issuer stays
  primary.
- Model exports as legal actions — rejected: it confuses preparation with
  exercise of a right and invites authority-boundary creep.

## Assumption and revisit trigger

This assumes a source capture can be represented as an artifact with provenance
and digest. Revisit when signed legislative datasets, authenticated APIs, or
multi-language official publications require a richer source identity model.
