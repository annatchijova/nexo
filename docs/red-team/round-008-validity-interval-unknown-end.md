# Security Audit — Validity interval end semantics

## Red Team Round 8

**Date:** 2026-09-13
**Scope:** `ValidityInterval` boundary semantics.

The initial documentation called `effective_to: None` “no known end date,”
while `contains()` treated it as valid for every later date. This was a
contract-level ambiguity, not an executed exploit.

`None` now means the policy bundle explicitly represents an open-ended interval.
It never means an unknown end date. The accompanying test proves that such an
interval includes a later in-domain date.

Unknown currency is intentionally not represented by `ValidityInterval`. A
policy layer that cannot establish an end when one is material must fail closed
as `POLICY_NOT_CURRENT`; it cannot construct a current claim from uncertainty.

`KnownEnd | NoEnd | UnknownEnd` remains deferred until a legitimate source or
bundle workflow needs to preserve unknown end data rather than fail closed.
