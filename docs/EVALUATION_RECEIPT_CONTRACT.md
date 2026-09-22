# Evaluation receipt contract

## Purpose

An evaluation row is a sealed record of a decision. It is not, by itself, a
capability to prepare material. Before Step 7 can mint a
`VerifiedPreparationSnapshot`, the application layer must produce a receipt
that proves the four binding relations in `docs/BINDING_EVIDENCE_CONTRACT.md`.

## Required bindings

Each receipt must bind, without relying on co-presence in a struct:

| Relation | Required evidence |
| --- | --- |
| Evaluation identifies the action | Durable evaluation id, exact action-route id, result kind/status, and a canonical digest of the decided result. |
| Action belongs to the case | Evaluation's case id and the ownership check that authorized the command. |
| Evaluation used this policy | Immutable policy-bundle id and its captured bundle digest, recorded in the same evaluation transaction. |
| Evaluation derived from these inputs | Canonical, ordered input-manifest digest covering the case-graph references visible to the projection, plus the graph/schema version. |

The receipt also carries a canonical action fingerprint, evaluator version and
result schema version. Missing
or mismatched evidence fails closed; no receipt is emitted for a
non-actionable or conditionally supported result.

All persisted digest references use the canonical lowercase 64-character
SHA-256 representation. The database rejects alternate algorithms and
malformed encodings before they can bind to an artifact, policy, evaluation,
receipt, or preparation.

## Authority boundary

The API/application composition owns the receipt because it is the only layer
that sees the authorized case, durable evaluation identity, policy row, and
input projection together. `nexo-core` remains the authority for whether the
action is supported; it does not read PostgreSQL or mint a durable identity.

The preparation constructor may consume only a verified receipt capability. A
caller must not be able to assemble one from public `case_id`, `route_id`,
bundle id, and digest arguments. The receipt producer must verify that every
endpoint belongs to the same transaction and case before it can cross into
the core preparation boundary.

## Canonical input manifest

The first implementation uses the projection's ordered case-node identities,
the policy-bundle digest, and the result schema version as typed canonical
values. It must not include rendered prose, wall-clock time, bearer tokens, or
database row ordering that is not part of the projection contract. A changed
node, bundle, or schema therefore produces a different manifest digest.

## Non-goals

- This contract does not add a delivery, filing, recipient, credential, or
  transport capability.
- It does not make a digest prove legal truth; it proves byte-level identity of
  the referenced record.
- It does not make a `CONDITIONALLY_SUPPORTED` route preparable. The core's
  `ActionOption::is_available()` gate remains authoritative.

## Failure cases

| Condition | Required result |
| --- | --- |
| Evaluation is non-actionable or conditional | No preparation capability. |
| Case ownership cannot be re-established | No receipt. |
| Policy row or captured digest is absent/mismatched | No receipt. |
| Input manifest cannot be reproduced from the authorized projection | No receipt; preserve the evaluation for audit. |
| Receipt is reused after a case or policy change | Preparation is invalidated or a new evaluation is required. |
