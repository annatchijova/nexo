# AR policy bundle: personal data access, rectification, and suppression

## Purpose

The first real (not illustrative) jurisdiction bundle, per plan.md Step 4:
Ley 25.326 (Protección de los Datos Personales), articles 14 and 16 —
the right to request access to one's own personal data held by a public or
private data controller, and the right to have it rectified, updated, or
suppressed. Implemented in `crates/nexo-policy-ar`.

This is the narrowest of the three real situations the maintainer named as
in scope (the other two — non-consensual intimate content under Ley 27.736,
and digital gender-based violence under Ley 26.485 — are future bundles
following this same contract shape). It was chosen to go first because its
factual predicate (identity of the requester) and its legal mechanics
(a request to a data controller, with fixed response deadlines) are the
least procedurally entangled with criminal process, making it the cleanest
bundle to get right before the harder two.

## Source capture

`crates/nexo-policy-ar/sources/ley_25326_texact.html` is the exact bytes
fetched from InfoLEG (`servicios.infoleg.gob.ar`, Argentina's official
legislative information service, operated by the Ministerio de Justicia),
retrieved 2026-09-21. SHA-256:
`61548a0fba22550e19a4189c101876238501cb6a9b5821cea5e36478708a3b61`.

This is not a paraphrase: `captured_source_is_the_real_infoleg_text` asserts
the committed file literally contains "25.326", "ARTICULO 14",
"previa acreditaci[ón]", and "ARTICULO 16". `build()` attests these exact
bytes through `nexo_policy_bridge::attest_capture` — the same bridge any
future adapter would use — so a mutated or truncated copy of the source
fails to build a bundle at all, not just fails a separate review step.

## Claims and route

| Claim | Article | Proposition |
| --- | --- | --- |
| `claim_access` | Art. 14 | Right to request and obtain one's own personal data from a public or private data controller, upon proof of identity, within ten calendar days of the controller being formally notified. |
| `claim_rectification` | Art. 16 | Right to have personal data rectified, updated, or (where applicable) suppressed, within five business days of the request. |

Both claims share the one captured source above (`SupportRole::Primary`),
carry the statute's own effective date (2000-10-30, promulgación parcial)
as their validity start, and are open-ended (Ley 25.326 remains in force).

`build()` also assembles one `ActionRoute` — "request access, rectification,
or suppression of personal data," jurisdiction AR — requiring both claims
and one mandatory requirement mirroring art. 14.1's own precondition:
identity must be proven before a controller owes a response.

## Evaluated outcomes

Every case below runs the real `nexo_core::evaluate` against the real
bundle; nothing here is asserted without actually calling the evaluator.

| Scenario | Result | Test |
| --- | --- | --- |
| Identity proven, correct jurisdiction and date | `Actionable(Supported)` | `actionable_when_jurisdiction_date_and_identity_all_line_up` |
| Identity not yet proven | `NonActionable(InsufficientFacts)` | `insufficient_facts_when_identity_not_yet_proven` |
| Route asks for United States, bundle is Argentina | `NonActionable(OutOfJurisdiction)` | `out_of_jurisdiction_when_route_targets_a_different_jurisdiction_than_the_bundle` |
| Reference date precedes this bundle version's own activation window | `NonActionable(PolicyNotCurrent)` | `policy_not_current_when_reference_date_precedes_bundle_activation` |
| A bundle variant whose `SourcePolicy` does not accept the claim's actual acquisition channel | `NonActionable(Abstain)` | `abstains_when_the_only_eligible_source_authority_is_excluded_by_the_bundle` |

## Known limit: `Contraindicated` and `ConflictingLegalClaims` are out of scope here

`nexo_core::negative_evidence::PolicyRuleEngine::new` is `pub(crate)` —
by design, per its own doc comment, so that "an adapter [cannot] declare an
arbitrary relation table and then treat it as legal policy." Those two
non-actionable variants can only be produced by a policy-bundle loader
living inside `nexo-core` itself, which does not exist yet. This bundle
therefore cannot and does not exercise them; `nexo-core`'s own
`negative_evidence_tests.rs` already covers that machinery in isolation.
Closing this gap is future work for whichever layer becomes the in-core
bundle loader, not for an adapter crate like this one.

## Bundle currency vs. statute effective date

Two different dates are deliberately kept distinct: the statute's own
effective date (`claim.validity()`, 2000-10-30 onward — when the *law*
took effect) and this bundle version's own activation window
(`bundle.validity()`, 2026-09-21 onward — when *this captured NEXO bundle*
was built). `policy_not_current_when_reference_date_precedes_bundle_activation`
evaluates at 2020-01-01: the statute was in force then, but no bundle
version of it had been captured and activated in NEXO yet, so the honest
answer is `PolicyNotCurrent`, not a silent claim that 2020 evidence was
somehow evaluated against 2026 policy text.

## Non-goals

- Ley 27.736 and Ley 26.485 bundles: future work, same contract shape.
- Filing the actual data-access request with a real controller: this
  bundle only establishes that the route is supported; per
  `docs/ARCHITECTURE.md`'s "Preparation boundary," preparing a draft
  request is a distinct, not-yet-built capability, and NEXO never performs
  the external act itself.
- `Contraindicated`/`ConflictingLegalClaims`: see above.
