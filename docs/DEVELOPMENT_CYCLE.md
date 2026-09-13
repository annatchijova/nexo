# Development cycle

Every layer follows this sequence:

1. Write the domain contract, threat model, invariants, limits, and explicit non-goals.
2. Implement the smallest complete slice that satisfies that contract.
3. Write adversarial and property tests designed to fail against a deliberately broken implementation.
4. Run a focused red-team review against the actual code and tests.
5. Resolve confirmed findings, record residual risks, then start the next layer.

No layer is declared complete because a demo works. A green test suite is reported with what it proves, what it does not prove, and which hostile cases remain outside coverage.

Git history is forward-only: no rebase, squash, or force-push in agent-assisted work. Each implementation or remediation commit must be independently reviewable.
