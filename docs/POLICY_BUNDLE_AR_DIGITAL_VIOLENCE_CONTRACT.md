# AR policy bundle: digital violence (Ley 27.736, "Ley Olimpia")

## Purpose

The second and third real Argentina situations the maintainer named as in
scope — non-consensual intimate content, and digital gender-based
harassment/violence — turned out, after reading the actual statute rather
than assuming its shape, to be **one legal instrument, not two**.
Implemented in `crates/nexo-policy-ar-digital-violence`.

## Why one bundle, not two

Ley 27.736 does not create a standalone "revenge porn" offense. It amends
Ley 26.485 (Protección Integral para Prevenir, Sancionar y Erradicar la
Violencia contra las Mujeres) to define "violencia digital" as one single
category, and non-consensual intimate content is one clause inside that
one definition (art. 4°, incorporating inciso i) of art. 6° of Ley
26.485):

> "...conductas que atenten contra su integridad, dignidad, identidad,
> reputación, libertad, y contra el acceso, permanencia y desenvolvimiento
> en el espacio digital o que impliquen la obtención, reproducción y
> difusión, sin consentimiento, de material digital real o editado,
> íntimo o de desnudez, que se le atribuya a las mujeres, o la
> reproducción en el espacio digital de discursos de odio misóginos... o
> situaciones de acoso, amenaza, extorsión, control o espionaje de la
> actividad virtual..."

The same sentence, the same claim, the same route. Modeling these as two
bundles would have meant inventing a legal boundary the statute itself
does not draw — the kind of thing `docs/CLAUDE.md`'s "prefer the boring
explanation first" discipline exists to catch. Checking the real text
(`sources/ley_27736_norma.html`, InfoLEG) before designing the bundle
caught this; it was not assumed going in.

## Source capture

`crates/nexo-policy-ar-digital-violence/sources/ley_27736_norma.html` is
the exact bytes fetched from InfoLEG
(`servicios.infoleg.gob.ar/infolegInternet/anexos/390000-394999/391774/norma.htm`),
retrieved 2026-09-22. SHA-256:
`9bc6ccab30298a534900e083719c2f92f6ff51204e1adb7ccdd08a09b3219772`.

Not a paraphrase: `captured_source_is_the_real_infoleg_text` asserts the
committed file literally contains "27736", "LEY OLIMPIA", the
non-consensual-content clause, and the removal-order's URL-identification
requirement (cut just before each accented character, since — like
`nexo-policy-ar`'s Ley 25.326 source — this page is Latin-1 encoded, so an
accented byte survives `from_utf8_lossy` only as a replacement character,
never the original letter).

`build()` attests these exact bytes against a pinned digest, per the same
pattern `nexo-policy-ar` uses (RT-001-02 in `docs/RED_TEAM_ROUND_001.md`:
a digest recomputed from the same file it is supposedly checking is
self-referential and worthless). **Verified again for this bundle, not
just assumed to carry over:** ran the same tamper experiment — mutated
"noventa (90)" to "NUEVE (9)" in the committed file (a real substantive
change: it shortens the platform data-preservation period the statute
sets in art. 12) — and confirmed 6 of 8 tests fail immediately with
`DigestMismatch`. Source restored afterward; `git diff` clean, digest
matches the original.

One methodology note worth recording: the first tamper attempt used a
byte string (`"noventa (90) dias"`, plain ASCII) that does not actually
occur in the Latin-1-encoded file (`día` has an accented í, stored as a
single Latin-1 byte, not `"ia"`), so `bytes.replace()` silently matched
nothing and the file was never actually modified — the test would have
been a false confirmation had its result not been checked against the
file's own digest before and after. Corrected to match the literal bytes
present in the file, per the same "prefer the boring explanation" pass.

## Claims and route

| Claim | Article | Proposition |
| --- | --- | --- |
| `claim_definition` | Art. 4° (Ley 27.736 → inciso i, art. 6°, Ley 26.485) | Defines digital violence, explicitly including non-consensual distribution of intimate/nude content. |
| `claim_removal_order` | Art. 12° (Ley 27.736 → apartado a.9, art. 26°, Ley 26.485) | A court may order, by reasoned decision, that platforms remove content constituting digital violence, identifying the specific URL. |

One route — "solicitar orden judicial de cese y remoción de contenido de
violencia digital" — jurisdiction AR, requiring both claims, with one
mandatory requirement (`content_identified_requirement`) mirroring art.
12's own precondition: a court cannot order removal of unspecified
content, so the route requires the specific content/URL to already be
identified in the case (modeled as at least one `Artifact` factual-support
node — the captured screenshot/page/content itself).

## Evaluated outcomes

Every scenario below runs the real `nexo_core::evaluate`, not a mock.

| Scenario | Result | Test |
| --- | --- | --- |
| Content identified (an artifact in the case) | `Actionable(Supported)` | `actionable_when_content_is_identified` |
| No content identified yet | `NonActionable(InsufficientFacts)` | `insufficient_facts_when_no_content_identified_yet` |
| Route asks for United States, bundle is Argentina | `NonActionable(OutOfJurisdiction)` | `out_of_jurisdiction_when_route_targets_a_different_jurisdiction_than_the_bundle` |
| Reference date precedes this bundle version's activation | `NonActionable(PolicyNotCurrent)` | `policy_not_current_when_reference_date_precedes_bundle_activation` |
| A bundle variant excluding the claim's actual source channel | `NonActionable(Abstain)` | `abstains_when_the_only_eligible_source_authority_is_excluded_by_the_bundle` |

## Known limits

- Same `Contraindicated`/`ConflictingLegalClaims` gap as
  `nexo-policy-ar` — `nexo_core::negative_evidence::PolicyRuleEngine::new`
  is `pub(crate)` by design; only an in-core bundle loader (not built
  yet) can exercise those variants.
- This route models the *judicial removal-order* mechanism only (art. 12).
  Ley 27.736 also touches evidence-preservation duties (art. 9 → art. 16
  inciso l), protective no-contact orders (art. 11), and a
  multi-channel assistance service (art. 5) — each a candidate for its
  own claim/route in a future round, not modeled here because this round
  targeted the one route with a clear factual precondition and a clear
  positive/negative evaluation shape.
- Not yet wired into `nexo-api`: `nexo-api`'s `seed.rs`/`AppState` still
  targets `nexo-policy-ar` (Ley 25.326) only. Exposing a second bundle
  through the HTTP layer — likely via a `bundle` selector on the
  case-evaluation endpoint — is follow-on work, not implied complete by
  this crate's existence.
