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

## Image/OCR evidence intake is not built

`nexo-extraction` ships three extractors: plain text
(`crates/nexo-extractor-plaintext`), email
(`crates/nexo-extractor-eml`, RFC 5322 plus a bounded MIME subset — CODE
FACT, see [`EXTRACTOR_EML_CONTRACT.md`](EXTRACTOR_EML_CONTRACT.md)), and PDF
(`crates/nexo-extractor-pdf` — CODE FACT, see
[`EXTRACTOR_PDF_CONTRACT.md`](EXTRACTOR_PDF_CONTRACT.md)). Image/OCR
extraction, named in the original build plan, is not built. In practice this
means a person can paste or upload plain text, attach a raw `.eml` message,
or attach a PDF as evidence today, but not attach a screenshot for OCR —
they would need to transcribe that content into text first.

The `.eml` and PDF extractors' own non-goals are their own limitations, not
restated here in full — see each contract's "Non-goals" section. Worth
naming directly because they bound what "attach a PDF" actually covers
today: the PDF extractor does not read scanned pages with no embedded text
layer (it extracts text that is already text in the PDF, not pixels), does
not decrypt password-protected PDFs, and does not resolve PDF 1.5+
compressed object streams — a PDF built with those will surface as
`no_pages_found` rather than being silently misread.

Closing the OCR gap means adding an extractor crate inside the existing
sandbox boundary (`docs/SANDBOX.md`), with its own contract naming supported
formats and rejection behavior, per the "First implementation gate"
convention the three existing extractors already follow.

## The web UI is a single dense page, not the full case-timeline experience

`web/src/main.ts` (CODE FACT, ~300 lines as of this writing) covers the
evidence-to-citation-to-export flow end-to-end — evidence intake,
evaluation, preparation, export — but as one page rather than the
chronological, visually-distinguished case timeline described in
[`WEB_UI_CONTRACT.md`](WEB_UI_CONTRACT.md). It has not had an accessibility
pass (contrast, keyboard navigation, screen-reader labels), which matters
here specifically because this is evidence a person may need to use under
stress.

## Personal deployment is prepared and locally validated, not yet running on AWS

[`../deploy/aws/README.md`](../deploy/aws/README.md) and
[`../deploy/aws/provision.sh`](../deploy/aws/provision.sh) document and
automate the install order for a single-tenant AWS instance (EC2,
PostgreSQL, the Docker extractor sandbox, TLS via
[`../deploy/aws/Caddyfile`](../deploy/aws/Caddyfile), backups).

What is RUNTIME-CONFIRMED this session, precisely, so the boundary is
exact: `cargo build --release -p nexo-api` was built and run for real
inside a fresh Amazon Linux 2023 container (`docker run amazonlinux:2023`)
— package installation (`dnf install docker postgresql16-server ...`),
the pinned Rust toolchain, and the release build all completed cleanly, and
the resulting binary's `ldd` output resolves with no missing libraries.
Separately (on the host, not in the AL2023 container), the release binary
was run with the exact production-shaped environment file
`provision.sh` generates — including `NEXO_AUDIT_HMAC_KEY`, which this
session found was missing from the *previous* version of this document
even though `nexo_app::audit::append` requires it for every mutating
endpoint, meaning a deployment that followed the old instructions literally
would have booted and then failed on the first evidence upload. Against
that real release binary, this session ran the complete path over real
HTTP — `/healthz`, both seeded bundles, evidence intake in all three kinds
(`plain_text`, `eml`, `pdf`), an assertion, an evaluation reaching
`actionable`/`supported`, both preparation kinds
(`draft_request`, `evidence_package`), export, and independent verification
of that export with the real `nexo-verify` CLI — all successful.

What remains a PLAUSIBLE HYPOTHESIS, not RUNTIME-CONFIRMED: `postgresql-setup
--initdb`, `systemctl enable`, and the `useradd`/`usermod` steps in
`provision.sh` were not run against real systemd (a plain container has no
systemd as PID 1) — these are CODE FACT, standard, well-established
RHEL/AL commands, not independently exercised this session. No EC2 instance
has actually been created, and no domain/TLS certificate has actually been
issued. The Vercel-hosted web demo currently has no backend behind it — see
the Technical README's Status section for exactly what the demo can and
cannot do as a result.

## The United States bundle has not started

Argentina (Ley 25.326, Ley 27.736) is the only jurisdiction with a real,
wired policy bundle. The build plan sequences the United States bundle
after Argentina's bundles complete a full adversarial (red-team) review —
that sequencing has not happened yet, so no US bundle work has begun.

## Resolved since the previous audit

- **PDF evidence intake.** `crates/nexo-extractor-pdf` is a new sandboxed
  extractor: linear object scan, `/FlateDecode` decompression (via
  `flate2`'s pure-Rust `rust_backend` — the one extractor in this workspace
  with a real dependency, and a real archive-bomb defense, since DEFLATE is
  genuine decompression), content-stream tokenizing, and per-font
  `/ToUnicode` CMap decoding with a WinAnsiEncoding fallback. `POST
  /v1/cases/{case_id}/evidence` accepts `"kind": "pdf"` with `text` as
  base64 (PDF is binary; a JSON string must be valid UTF-8). Tested against
  hand-built fixtures *and* a real PDF produced by LibreOffice Writer,
  which is what actually found and fixed two real bugs during development:
  an indirect-reference `/Length` (`3 0 R`, not a direct integer) that a
  hand-built-fixture-only test suite would never have exercised, and a
  subsetted embedded font whose byte codes have no relationship to
  WinAnsiEncoding at all, requiring `/ToUnicode` CMap support that wasn't
  originally planned as in-scope. RUNTIME-CONFIRMED at every layer,
  including the real-PDF case: 15 unit tests in the extractor binary, 3
  adapter tests against the real built image (one against the real
  LibreOffice fixture, asserting the exact expected Spanish text with
  accents intact), and two full HTTP-through-Docker-through-database tests.
  See [`EXTRACTOR_PDF_CONTRACT.md`](EXTRACTOR_PDF_CONTRACT.md) for the full
  contract, including what it still does not handle (Type0/composite
  fonts, object streams, encryption, scanned pages with no text layer).
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
