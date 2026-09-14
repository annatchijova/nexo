# Security Audit — Normative deficit separation

## Red Team Round 6

**Date:** 2026-09-13
**Method:** semantic counterexample against the policy-source contract
**Scope:** evaluation consequences for missing normative authority; no policy
evaluator or UI implementation exists yet.

## Threat model

A future policy evaluator or UI follows the source-contract table literally.
It cannot fabricate an eligible normative source, but it may choose among the
states the contract permits.

## Finding

| ID | Severity | Level | Finding |
| --- | --- | --- | --- |
| RT-013 | High | CODE FACT | The original table allowed absence of normative authority to become `INSUFFICIENT_FACTS`. |

## RT-013 — Normative absence could be misrepresented as missing incident facts

The original row read: `No eligible source capture | ABSTAIN or
INSUFFICIENT_FACTS`. This is a direct textual fact, not an executed software
defect.

### Counterexample

```text
case facts: sufficient to identify the affected person and event
legal route: plausible
eligible current in-scope normative source: absent from NEXO
```

Permitting `INSUFFICIENT_FACTS` makes a downstream UI capable of asking the
person for more incident details when the missing element is NEXO's policy
bundle. That violates the distinction between factual uncertainty and
normative-authority uncertainty.

### Remediation

The contract now requires `ABSTAIN(NoAuthoritativeLegalSource)` where no
eligible source supports the route. `INSUFFICIENT_FACTS` is reserved for a
route already supported by eligible, current, in-scope normative material but
missing a named factual predicate.

## Taxonomy restraint

`PRIMARY_OFFICIAL` is retained as a source classification only. It is not a
claim that statutes, judgments, administrative decisions, and regulator
materials have identical hierarchy, bindingness, or precedential force. A
jurisdiction bundle must define those semantics; NEXO does not pre-build a
universal legal-weight ontology.

## Residual limit

This is a contract-level remediation. The future policy evaluator and UI need
constructor tests showing they cannot route a normative-source failure into a
factual question flow.
