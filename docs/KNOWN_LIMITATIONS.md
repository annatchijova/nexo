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

## PDF and image/OCR evidence intake are not built

`nexo-extraction` ships two extractors: plain text
(`crates/nexo-extractor-plaintext`) and email
(`crates/nexo-extractor-eml`, RFC 5322 plus a bounded MIME subset — CODE
FACT, see [`EXTRACTOR_EML_CONTRACT.md`](EXTRACTOR_EML_CONTRACT.md)). PDF and
image/OCR extraction, both named in the original build plan, are not built.
In practice this means a person can paste or upload plain text, or attach a
raw `.eml` message, as evidence today, but not attach a PDF or a screenshot
for OCR — they would need to transcribe that content into text or forward
it as an email first.

The `.eml` extractor's own non-goals are its own limitations, not restated
here: only the first `text/plain` part of a message is read (attachments
and nested multipart are not extracted), an HTML-only message with no
`text/plain` alternative is a bounded failure, and only UTF-8/US-ASCII
charsets are supported — full list in
[`EXTRACTOR_EML_CONTRACT.md`](EXTRACTOR_EML_CONTRACT.md)'s "Non-goals".

Closing the PDF/OCR gap means adding one extractor crate per format inside
the existing sandbox boundary (`docs/SANDBOX.md`), each with its own
contract naming supported formats and rejection behavior, per the "First
implementation gate" convention both existing extractors already follow.

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

- **`.eml` evidence intake.** `crates/nexo-extractor-eml` is a new,
  hand-rolled, zero-dependency sandboxed extractor: RFC 5322 headers
  (Date/From/To/Subject, unfolded), plus the decoded first `text/plain`
  part of a plain or multipart body (quoted-printable and base64
  supported). `POST /v1/cases/{case_id}/evidence` now accepts
  `"kind": "eml"` to route evidence through it instead of the plain-text
  extractor. RUNTIME-CONFIRMED at three layers: 13 unit tests in the
  extractor binary itself, 5 adapter tests against the real built image in
  `crates/nexo-extraction`, and two full HTTP-through-Docker-through-database
  tests in `crates/nexo-api/tests/api_test.rs`
  (`eml_evidence_is_extracted_into_header_and_body_observations`,
  `eml_evidence_with_unsupported_encoding_is_a_bounded_rejection`) —
  including a check that its `header:*` locators are genuinely produced by
  the eml extractor and not a silently-ignored `kind` falling back to the
  plain-text one. See [`EXTRACTOR_EML_CONTRACT.md`](EXTRACTOR_EML_CONTRACT.md)
  for the full contract, including what it still does not handle
  (attachments, nested multipart, non-UTF-8 charsets).
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
