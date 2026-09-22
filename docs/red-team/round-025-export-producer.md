# Security Audit — atomic export producer
## Red Team Round 025

**Date:** 2026-09-21  **Scope:** `nexo-app::export::write_export` and its
composition with `nexo-verifier`. **Base:** `main @ e475015`.

## Threat model

The caller may provide arbitrary artifact labels and bytes. The caller cannot
modify the producer binary or the verifier. The producer is not exposed by an
HTTP route yet; database selection and lifecycle transitions are out of scope
for this round.

## Invariants tested

| Invariant | Evidence | Result |
| --- | --- | --- |
| The manifest digest commits the case, timestamp, policy digest, labels, and byte digests | `writes_export_consumable_by_verifier` plus `nexo-verifier` integration | FALSIFIED attack / invariant holds |
| Every emitted artifact path is confined to the export directory | Producer derives paths only from fixed-width SHA-256 hex; verifier rejects traversal and symlink escape | FALSIFIED attack / invariant holds |
| An export is not visible as a destination before its files and manifest exist | Temporary directory is populated before one final rename | FALSIFIED attack / invariant holds |
| Existing exports are not overwritten | `rejects_overwrite_and_invalid_labels` | FALSIFIED attack / invariant holds |
| Duplicate or empty labels cannot create ambiguous manifest entries | `Manifest::try_new` rejects duplicates; producer rejects empty labels | FALSIFIED attack / invariant holds |

## Discarded vectors

| Vector | Level | Result |
| --- | --- | --- |
| Label `artifact/1` interpreted as a filesystem escape | FALSIFIED BY INDUCTION | Labels are sealed metadata; filesystem paths are digest-derived. |
| `../escape` artifact path | FALSIFIED BY INDUCTION | The producer never emits caller-controlled artifact paths; the standalone verifier rejects them. |
| Tamper with an emitted object after generation | FALSIFIED BY INDUCTION | The independent verifier recomputes the object digest and rejects the export. |
| Failure while writing a temporary export | PLAUSIBLE HARDENING CHECK | The temp directory is cleaned up, but a filesystem fault-injection test is not part of this round. |

## Residual hardening

1. `generated_at_unix_seconds` is caller-supplied. This is appropriate for a
   pure producer, but the future application command must source it from the
   server clock and must not accept it from HTTP input.
2. The producer has no aggregate export-size limit. Individual evidence
   objects are bounded elsewhere, but a future export command should define a
   total budget before accepting an attacker-influenced collection.
3. Directory durability after a sudden power loss is not claimed: visibility
   is atomic, but the producer does not `fsync` the directory after rename.

No confirmed vulnerability was found in the current, non-HTTP composition.
