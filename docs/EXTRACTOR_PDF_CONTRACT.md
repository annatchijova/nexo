# PDF extractor contract

## Purpose

The third extractor accepted under `docs/SANDBOX.md`'s "First implementation
gate," after `docs/EXTRACTOR_PLAINTEXT_CONTRACT.md` and
`docs/EXTRACTOR_EML_CONTRACT.md`. It turns PDF bytes into candidate
`Observation` records — one per text-showing operation on a page — so a
document a person already has (a form letter, a scanned-and-OCR'd notice, a
"print to PDF" export) can become evidence without first being retyped.

This is a **bounded subset** of PDF, not a general renderer or a full parser
of the format. It performs no interpretation of the extracted text's
meaning; that boundary belongs to `nexo-core`, exactly as for the other two
extractors.

## Isolation mechanism

Identical to the other two extractors': Docker, orchestrated by
`crates/nexo-sandbox`, with `--network none`, a read-only root filesystem
plus a small `noexec,nosuid` tmpfs, `--cap-drop ALL`,
`--security-opt no-new-privileges`, a fixed unprivileged numeric user, and
explicit memory/CPU/pids/wall-clock limits. This extractor adds no new
isolation surface, only a new image.

The extractor binary itself (`crates/nexo-extractor-pdf`) is a statically
linked musl binary in a `FROM scratch` image: no shell, no libc, no package
manager. Unlike the other two extractors, it is **not** zero-dependency: it
depends on `flate2` with only the `rust_backend` (`miniz_oxide`, pure Rust)
feature enabled, specifically to decompress `/FlateDecode` streams — the
near-universal compression PDF writers use for content streams. Hand-rolling
DEFLATE decompression was judged a substantially larger and easier-to-get-
subtly-wrong undertaking than the parsing this crate does hand-roll itself
(object scanning, dictionaries, content-stream tokenizing, text encoding,
ToUnicode CMaps); `rust_backend` specifically keeps the binary statically
linked with no libc dependency, so it still fits the same `FROM scratch`
image as the other two.

## Supported format

A PDF file, located and read via a **linear scan** for `N G obj ... endobj`
blocks — no xref table, incremental-update chain, or object-stream (PDF
1.5+ compressed cross-reference, `/Type /ObjStm`) resolution. The last
occurrence of a given object number in the file wins; object generation
numbers are not distinguished.

Within that, this extractor supports:

- **Pages**: every object declaring `/Type /Page`, ordered by object number
  (not by walking the `/Pages` tree's `/Kids` array — a document whose
  object-declaration order doesn't match its logical page order will have
  its pages extracted out of order).
- **Content streams**: a page's `/Contents` (a single indirect reference or
  an array of them, concatenated per spec), read directly or decoded via
  `/FlateDecode`. A `/Length` given as an indirect reference (very common in
  real-world PDF writers, including LibreOffice's own export — not just a
  direct integer) is resolved by looking up that object.
- **Text-showing operators**: `Tj`, `TJ` (string pieces concatenated,
  kerning numbers ignored), `'`, and `"` (operands read, but the extra
  word/character-spacing numbers `"` takes are ignored). Text position and
  matrix operators (`Td`, `TD`, `T*`, `Tm`, `cm`, ...) are read past but not
  interpreted — this extractor recovers *what* text is shown, not *where*.
- **Text decoding**: for a simple (non-`Type0`) font, byte codes are decoded
  through that font's own `/ToUnicode` CMap (`beginbfchar`/`beginbfrange`)
  when it has one — the standard mechanism PDF writers use to keep text
  extractable independent of a subsetted embedded font's own arbitrary
  glyph ordering, and the case a real LibreOffice export actually needed
  (see "Real-world verification" below). A code with no ToUnicode entry
  falls back to WinAnsiEncoding (Windows-1252; ASCII for 0x00-0x7F, matches
  Latin-1 for 0xA0-0xFF, the CP1252-specific mappings for 0x80-0x9F).
- **Inline images** (`BI ... ID ... EI`) are skipped over, not parsed, so
  their raw binary data doesn't confuse the content-stream tokenizer.

## Resource limits

Enforced by the extractor binary itself, in addition to the container-level
limits every extractor image gets identically (see above):

| Limit | Value |
| --- | --- |
| Input size | 25 MiB (`MAX_INPUT_BYTES`). |
| Objects found by the linear scan | 100,000 (`MAX_OBJECTS`). |
| Pages | 2,000 (`MAX_PAGES`). |
| One content (or ToUnicode) stream's decoded size | 10 MiB (`MAX_STREAM_DECODED_BYTES`). |
| Sum of every decoded stream's size across the document | 50 MiB (`MAX_TOTAL_DECODED_BYTES`). |
| Text observations | 50,000 (`MAX_LINES`), 100,000 bytes each (`MAX_LINE_BYTES`). |

Unlike the other two extractors, **decompression-ratio limits are a real
concern here, not a not-applicable note**: `/FlateDecode` is genuine
decompression, so a small input can legitimately expand into a much larger
output. `MAX_STREAM_DECODED_BYTES` and `MAX_TOTAL_DECODED_BYTES` are this
extractor's archive-bomb defense, checked incrementally against a `Read`
adapter (never trusting a stream's own declared `/Length` for how much
decompressed data to allocate) and defended in depth: a per-stream cap
(catches one large bomb) and a cumulative cap (catches many
individually-small-enough streams that bomb in aggregate).

## Rejection behavior

Every rejection is a typed, bounded failure record —
`{"kind":"failure","reason":"<code>"}` — identical in shape to the other two
extractors'. Defined reason codes:

`input_too_large`, `too_many_objects`, `no_pages_found`, `too_many_pages`,
`stream_too_large`, `malformed_stream`, `unsupported_font_encoding` (the
active font is `/Subtype /Type0` or `/Encoding /Identity-H` — see
"Non-goals"), `too_many_lines`, `line_too_long`.

The extractor's own process exit code is `0` for every one of these; exit
code `1` is reserved for the extractor failing to do its job at all,
surfaced by `nexo-sandbox` as `SandboxError::ExtractorCrashed`.

## Real-world verification

Every other extractor in this workspace is tested against hand-built
fixtures. This one is additionally tested against a **real PDF produced by
LibreOffice Writer** (`soffice --headless --convert-to pdf`, fixture
committed at `crates/nexo-extraction/testdata/real_libreoffice_export.pdf`
and `crates/nexo-api/testdata/real_libreoffice_export.pdf`), containing
Spanish text with accented characters. This mattered concretely: a
hand-built fixture using `/Length 123` (a direct integer) and a standard
font passed every test while this extractor still failed against
LibreOffice's real output, which uses `/Length 3 0 R` (an indirect
reference — the first bug this found and fixed) and a subsetted embedded
`TrueType` font whose byte codes have **no relationship to WinAnsiEncoding
at all**, relying entirely on its own `/ToUnicode` CMap to remain
extractable (the second gap this found, and the reason ToUnicode support
exists in this extractor rather than being deferred as a non-goal).

## Required evidence

- `Tj`, the `TJ` array form, and the `'`/`"` operators each produce the
  expected text (`simple_tj_text_is_extracted`,
  `tj_array_concatenates_string_pieces_and_ignores_kerning_numbers`).
- Literal-string escapes (parens, backslash) and hex strings decode
  correctly (`escaped_parens_and_backslash_in_literal_string_are_decoded`,
  `hex_string_operand_is_decoded`).
- A Latin-1/WinAnsi accented byte decodes to the matching character
  (`accented_latin1_byte_in_literal_string_decodes_as_the_matching_unicode_char`).
- `/FlateDecode` content is inflated before tokenizing
  (`flate_decode_content_stream_is_inflated_before_tokenizing`).
- A font's own `/ToUnicode` CMap is used when present, both the `bfchar`
  and `bfrange` forms
  (`to_unicode_cmap_decodes_codes_that_have_no_relationship_to_winansi`,
  `to_unicode_bfrange_maps_a_consecutive_run_of_codes`).
- A `Type0`/`Identity-H` font is a bounded failure, not a garbled decode
  (`type0_identity_h_font_is_a_bounded_failure_not_a_garbled_decode`).
- Multiple `/Contents` streams for one page are concatenated in order
  (`multiple_content_streams_for_one_page_are_concatenated`).
- A document with no page objects, and empty input, are both bounded
  failures, not crashes (`document_with_no_page_objects_is_a_bounded_failure`,
  `empty_input_is_a_bounded_failure_not_a_crash`).
- Archive-bomb defense: one oversized decompressed stream is rejected
  (`oversized_decompressed_stream_is_a_bounded_failure`); many streams each
  under the per-stream cap still hit the cumulative cap
  (`many_streams_each_under_the_per_stream_cap_still_hit_the_total_cap`).
- Adapter-level round trip against the real built image, plus the real
  LibreOffice fixture with its accented Spanish text intact
  (`crates/nexo-extraction/src/lib.rs`'s `pdf_*` tests, especially
  `real_libreoffice_pdf_is_extracted_with_accented_text_intact`).
- API-level round trip: `kind: "pdf"` with base64-encoded bytes reaches the
  real extractor through the real sandbox and a real database, and
  malformed base64 is rejected before ever reaching the sandbox
  (`crates/nexo-api/tests/api_test.rs`'s
  `pdf_evidence_is_extracted_via_base64_and_reaches_the_real_extractor`,
  `pdf_evidence_with_invalid_base64_is_rejected_before_reaching_the_sandbox`).
- Isolation mechanism: covered generically by `nexo-sandbox`'s own tests,
  per "Isolation mechanism" above.

## Reproducing the image

```
scripts/build_extractors.sh
```

Builds the musl binary and the `nexo-extractor-pdf:local` image alongside
the other two extractors'.

## Non-goals

- **Object streams and cross-reference streams** (PDF 1.5+ compressed
  `/Type /ObjStm`, `/Type /XRef`): not resolved. A PDF that stores its
  objects only inside compressed object streams, rather than as direct
  `N G obj` blocks, will not have those objects found by the linear scan —
  in practice this surfaces as `no_pages_found` if the `/Page` objects
  themselves live inside an ObjStm.
- **Page-tree order**: pages are ordered by object number, not by resolving
  `/Pages`/`/Kids`. Most single-revision PDF writers declare objects in
  page order, but this is not guaranteed.
- **Type0/composite fonts** (`/Subtype /Type0`, `/Encoding /Identity-H`):
  a bounded failure, not a garbled decode — see "Rejection behavior". This
  extractor's one-byte-per-character-code assumption does not hold for
  composite fonts' multi-byte code spaces.
- **Encoding `/Differences` arrays**: a simple font's custom
  code-to-glyph-name remapping (distinct from `/ToUnicode`) is not read.
  When no `/ToUnicode` CMap covers a given code, the WinAnsiEncoding
  fallback is used regardless of any `/Differences` array.
- **Inherited page resources**: `/Resources` set on an ancestor `/Pages`
  node rather than directly on the `/Page` object is not resolved — such a
  page's fonts are treated as unknown, so its text falls back to
  WinAnsiEncoding.
- **Encrypted or password-protected PDFs**: not decrypted; their streams
  will fail to decompress or decode meaningfully, typically surfacing as
  `malformed_stream` or garbled text rather than a specific
  `encrypted_pdf` reason code.
- **Images and OCR**: no image extraction or text recognition of any kind.
  A scanned page with no text layer (no embedded, extractable text at all)
  produces no observations for that page, not an error — this extractor
  finds text that is already text in the PDF, it does not read pixels.
- **Producing `Observation` graph nodes directly**: this extractor and its
  adapter return `ObservationCandidate` values; promoting one into a real
  `nexo_core::ObservationNode` is an application-layer decision, same as
  for the other two extractors.

## A known test-infrastructure flakiness

`cargo test --workspace` occasionally reports a spurious failure in
`nexo-extraction`'s Docker-dependent tests (observed:
`empty_input_produces_an_empty_observation_list_not_a_failure` and its
`eml_` counterpart) when the full workspace suite runs with its default
parallelism, now that three extractors' worth of container-launching tests
compete for Docker at once. Running `cargo test -p nexo-extraction` alone
reproduces cleanly every time this was checked. This is resource
contention under concurrent `docker run` invocations, not a defect in the
extraction logic — re-running the workspace suite, or running the affected
package alone, resolves it. Not yet fixed with an explicit test-level
concurrency limit; flagged here rather than silently ignored.
