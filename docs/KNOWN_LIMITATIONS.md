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

## Resolved since the previous audit

- **`evidence_package` preparation.** `ArtifactSummary` / `list_case_artifacts`
  in `crates/nexo-app/src/repository.rs` previously had no caller anywhere in
  the workspace. It is now the read path `POST
  /v1/cases/{case_id}/preparations` renders from when `kind` is
  `evidence_package`: a self-verifying Markdown index naming each artifact's
  real SHA-256 digest, its ingestion-declared (unverified) filename and MIME
  type, and the locators of every observation actually extracted from it.
  RUNTIME-CONFIRMED end-to-end, including through the sandboxed extractor and
  the export/verify path:
  `crates/nexo-app/tests/repository_test.rs::list_case_artifacts_reports_digests_and_observation_locators`
  and
  `crates/nexo-api/tests/api_test.rs::evidence_package_preparation_lists_case_artifacts`.
- **A flaky audit-export test.** `nexo-verifier`'s
  `audit_export_rejects_metadata_edit` intermittently failed under
  `cargo test --workspace` (reproduced at 1/8 runs with `--test-threads=4`).
  Root cause: its `temp_export()` test helper named the directory only by a
  nanosecond timestamp and used `create_dir_all`, which does not error on an
  existing path — two tests landing on the same timestamp under real thread
  parallelism silently shared one `audit.json`, so one test's write could
  clobber another's mid-assertion. Fixed by adding a per-process atomic
  counter to the directory name and switching to `create_dir` (which does
  error on collision, failing loudly instead of silently sharing state).
  RUNTIME-CONFIRMED: 40/40 clean runs at `--test-threads=8` after the fix,
  versus the reproduced failure before it.
