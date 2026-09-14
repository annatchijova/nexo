# Preparation contract

## Boundary

An `ActionOption` is a legal or rights route. A `Preparation` is a locally
generated material for pursuing that route. Neither type is an external act.

```text
ActionOption (supported route)
  └── Preparation
        ├── DraftRequest
        ├── EvidencePackage
        └── Export

Preparation != delivery, filing, signature, acceptance, submission, or notice
```

## Preconditions

Every preparation records:

```text
action_option_ref
evaluation_snapshot_ref
policy_bundle_digest
input_manifest_digest
preparation_kind
generator/version
output_artifact_ref
output_digest
```

The evaluation snapshot prevents a draft from being represented as current
support after the case facts or policy bundle changed. The output is an
artifact, not an instruction to an integration.

## State model

```text
NOT_PREPARED → PREPARED → EXPORTED
                    └── INVALIDATED
```

`EXPORTED` means NEXO made the material available to the person. It does not
mean sent, filed, received, accepted, or legally effective. There is no
`SENT`, `FILED`, `DELIVERED`, or `AUTO_SUBMIT` state in this domain.

## External-act gate

Delivery, filing, signature, acceptance, and notice are R3 external actions.
They belong to a human-controlled application boundary and require an explicit
human actor, a preview of the exact material and target, and a durable record
of that actor's declaration if the result later enters the case graph.

NEXO may subsequently record an external act only as evidence supplied or
confirmed by that actor, for example an `Artifact` receipt or a
`UserAssertion`. It must not infer the act from preparation or export.

## Prohibitions

- No recipient address, endpoint, credential, or transport capability belongs
  to `Preparation`.
- No model or generator participates in preparation: explanatory text is
  deterministic rendering, and nothing outside this contract can select a
  recipient, approve a draft, or trigger delivery.
- A scheduler may invalidate stale preparations; it cannot deliver them.
- Re-exporting an unchanged artifact is idempotent. Regenerating after an input
  or policy change creates a new preparation with a new manifest digest.

## Failure behavior

| Condition | Required result |
| --- | --- |
| Route ceases to be supported | `INVALIDATED`; preserve old artifact for audit, do not present it as current. |
| Policy bundle changes | `INVALIDATED` until re-evaluated. |
| Input manifest cannot be reproduced | Do not label output as a current preparation. |
| User wants NEXO to send/file | Explain the boundary and provide the export; do not request transport credentials. |
