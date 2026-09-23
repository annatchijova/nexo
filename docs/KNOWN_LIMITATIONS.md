# Known limitations

This document exists so that no limitation is discovered by a reader instead
of disclosed by the project. It is linked from the
[Technical README](TECHNICAL_README.md) Status section rather than embedded
there, and it is not summarized or softened in the primary README — the
primary README narrows each claim it makes precisely enough to stay true
without needing this page; this page carries the exhaustive account.

Each item states the claim's actual boundary, the evidence behind that
boundary, and what would close the gap. Epistemic labels follow the same
convention as the Technical README: **RUNTIME-CONFIRMED** (an actual command
was run this session), **CODE FACT** (verified by reading the live source),
**PLAUSIBLE HYPOTHESIS** (not independently verified this session).

## Evidence intake is text-only today

`nexo-extraction` and `nexo-extractor-plaintext` ship exactly one extractor:
plain text (CODE FACT — `crates/nexo-extraction`, `crates/nexo-extractor-plaintext`
contain no other extractor crate). Email (`.eml`), PDF, and image/OCR
extraction, all named in the original build plan, are not built. In
practice this means a person can paste or upload plain text as evidence
today, but not attach a `.eml` export, a PDF, or a screenshot for OCR — they
would need to transcribe that content into text first.

Closing this gap means adding one extractor crate per format inside the
existing sandbox boundary (`docs/SANDBOX.md`), each with its own contract
naming supported formats and rejection behavior, per the "First
implementation gate" convention already used for the plaintext extractor
(`docs/EXTRACTOR_PLAINTEXT_CONTRACT.md`).

## The web UI is a single dense page, not the full case-timeline experience

`web/src/main.ts` (CODE FACT, ~300 lines as of this writing) covers the
evidence-to-citation-to-export flow end-to-end — evidence intake,
evaluation, preparation, export — but as one page rather than the
chronological, visually-distinguished case timeline described in
[`WEB_UI_CONTRACT.md`](WEB_UI_CONTRACT.md). It has not had an accessibility
pass (contrast, keyboard navigation, screen-reader labels), which matters
here specifically because this is evidence a person may need to use under
stress.

## Personal deployment is prepared, not running

[`../deploy/aws/README.md`](../deploy/aws/README.md) documents the install
order for a single-tenant AWS instance (EC2, PostgreSQL, the Docker
extractor sandbox, TLS, backups). This is a PLAUSIBLE HYPOTHESIS, not
RUNTIME-CONFIRMED: no live instance was reached or checked this session.
The Vercel-hosted web demo currently has no backend behind it — see the
Technical README's Status section for exactly what the demo can and cannot
do as a result.

## The United States bundle has not started

Argentina (Ley 25.326, Ley 27.736) is the only jurisdiction with a real,
wired policy bundle. The build plan sequences the United States bundle
after Argentina's bundles complete a full adversarial (red-team) review —
that sequencing has not happened yet, so no US bundle work has begun.

## One uncommitted, unwired change on `main`

`ArtifactSummary` / `list_case_artifacts` in
`crates/nexo-app/src/repository.rs` has no caller anywhere in the workspace
(CODE FACT, verified by `grep -rn` across `crates/`). It compiles cleanly
and does not affect `cargo test` or `cargo clippy`, but it is not part of
any finished, wired feature. It is flagged here rather than silently
committed as if it were complete, or silently discarded without the person
who wrote it deciding what to do with it.
