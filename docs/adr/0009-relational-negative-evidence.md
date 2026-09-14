# ADR 0009: make negative legal evidence relational

## Status

Proposed for Round 014.

## Decision

Conflicts and contraindications will be represented by typed evidence relations,
not independent factual/legal vectors or boolean flags. A conflict names its
claim pair and a deterministic witness. A contraindication names the factual
support, legal ground, and deterministic rule match that bind them.

## Alternatives rejected

- **`conflicting: bool`:** loses which claims conflict and cannot be audited.
- **Two claim ids without a witness:** treats coexistence as contradiction.
- **Separate fact and legal vectors:** allows unrelated evidence to manufacture
  a contraindication.
- **LLM-generated contradiction text:** explanation is not authority or proof.

## Falsification evidence required

Before the implementation is accepted, tests must construct every invalid shape
above and demonstrate rejection, then construct one valid witnessed conflict and
one valid fact-to-ground contraindication with deterministic support.
