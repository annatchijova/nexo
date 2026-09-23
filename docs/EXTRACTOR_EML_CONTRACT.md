# Email (`.eml`) extractor contract

## Purpose

The second extractor accepted under `docs/SANDBOX.md`'s "First implementation
gate," after `docs/EXTRACTOR_PLAINTEXT_CONTRACT.md`. It turns raw email
source bytes (RFC 5322, an `.eml` file, a forwarded message pasted as text)
into candidate `Observation` records: one per message header NEXO treats as
evidence (who, to whom, when, and the subject), and one per non-blank line
of the decoded text body. It performs no interpretation of the message's
meaning; that boundary belongs to `nexo-core`, exactly as for the
plain-text extractor.

## Isolation mechanism

Identical to `docs/EXTRACTOR_PLAINTEXT_CONTRACT.md`'s: Docker, orchestrated
by `crates/nexo-sandbox`, with `--network none`, a read-only root filesystem
plus a small `noexec,nosuid` tmpfs, `--cap-drop ALL`,
`--security-opt no-new-privileges`, a fixed unprivileged numeric user, and
explicit memory/CPU/pids/wall-clock limits. `nexo-sandbox`'s isolation tests
(`sandbox_flags_block_network_and_root_filesystem_write`,
`wall_clock_timeout_kills_a_runaway_container`) already cover this mechanism
generically — it takes an image name as a parameter, and this extractor adds
no new isolation surface, only a new image.

The extractor binary itself (`crates/nexo-extractor-eml`) is a statically
linked, zero-Rust-dependency musl binary packaged in a `FROM scratch` image,
same as the plain-text extractor: no shell, no libc, no package manager, and
no second program a compromised extraction could pivot to inside its own
container. Header unfolding, MIME boundary splitting, and
quoted-printable/base64 decoding are hand-rolled rather than pulled in from
a MIME-parsing crate, for the same reason the plain-text extractor hand-rolls
its JSON encoding: this binary runs directly against hostile bytes, and
every dependency added here is code that would run against that same
untrusted input.

## Supported format

RFC 5322 message source (a `.eml` file, or raw email source pasted as text):
a header block, a blank line, then a body. The extractor does not sniff or
trust a declared MIME type or file extension; it only ever inspects the
bytes themselves, exactly as `docs/SANDBOX.md` requires.

Within that, this extractor supports a **bounded subset** of MIME, not the
full RFC 2045–2049 family:

- **Headers surfaced as evidence**: `Date`, `From`, `To`, `Subject` (first
  occurrence of each; folded continuation lines are unfolded with a single
  joining space). Every other header (routing `Received` lines, message
  IDs, DKIM signatures, and so on) is read only far enough to skip past it —
  never surfaced as an observation, since it carries no evidentiary content
  a person would cite.
- **Body**: a single, non-multipart `text/plain` body, or the **first**
  `text/plain` part of a `multipart/*` message. Content-Transfer-Encoding
  `7bit`, `8bit`, `binary` (no decoding), `quoted-printable`, and `base64`
  are decoded; any other value is a bounded failure.
- **Charset**: `utf-8` and `us-ascii` only. Anything else is a bounded
  failure rather than an attempted, possibly-wrong transliteration.

## Resource limits

Enforced by the extractor binary itself, in addition to the container-level
limits `nexo-sandbox` applies identically to every extractor image (see
above):

| Limit | Value |
| --- | --- |
| Input size | 25 MiB (`MAX_INPUT_BYTES`), checked against `stat` and again against the bytes actually read. |
| Header count | 500 (`MAX_HEADERS`). |
| Single header's unfolded value | 10,000 bytes (`MAX_HEADER_BYTES`). |
| Body line count | 50,000 (`MAX_LINES`). |
| Single body line length | 100,000 bytes (`MAX_LINE_BYTES`). |
| MIME parts scanned per multipart message | 50 (`MAX_MIME_PARTS`). |

Container memory, CPU, process count, wall clock, and result-file
read-back size are the same defaults `SandboxLimits::conservative_default()`
applies to every extractor, per `docs/EXTRACTOR_PLAINTEXT_CONTRACT.md`.

Decompression-ratio limits are **not applicable**: quoted-printable and
base64 are both strictly length-reducing or length-neutral to decode (a
`=XX` triplet decodes to at most one byte; four base64 characters decode to
at most three bytes), so decoding can never amplify `body` past its own
already-bounded length the way a compressed archive could. An
archive-bomb-style test is therefore out of scope for this extractor, same
reasoning as the plain-text extractor's.

## Rejection behavior

Every rejection is a typed, bounded failure record —
`{"kind":"failure","reason":"<code>"}` — never a partial result, a crash, or
arbitrary output, identical in shape to the plain-text extractor's. Defined
reason codes:

`input_too_large`, `invalid_utf8`, `too_many_headers`, `header_too_long`,
`malformed_header` (a non-blank, non-continuation line in the header block
that is not `Name: value`), `missing_boundary` (a `multipart/*`
Content-Type with no `boundary` parameter), `too_many_parts`,
`no_text_part_found` (a multipart message with no immediate `text/plain`
part), `unsupported_charset`, `unsupported_transfer_encoding`,
`invalid_base64`, `too_many_lines`, `line_too_long`.

The extractor's own process exit code is `0` for every one of these: a
bounded failure is a successful run that correctly identified a message it
cannot or will not safely process. Exit code `1` is reserved for the
extractor failing to do its job at all, surfaced by `nexo-sandbox` as
`SandboxError::ExtractorCrashed`, distinct from a bounded failure.

## Required evidence

- Well-formed message: headers and a plain body produce the expected
  header and `body:line:<n>` observations, in order
  (`simple_message_yields_header_and_body_observations`).
- Folded header continuation lines unfold correctly
  (`folded_header_continuation_is_unfolded_with_one_space`).
- A message with no blank line at all is a valid, non-failure, all-headers,
  empty-body result, not an error
  (`message_with_no_blank_line_is_all_headers_empty_body`).
- Quoted-printable and base64 bodies decode correctly, including a
  quoted-printable soft line break
  (`quoted_printable_body_is_decoded`, `base64_body_is_decoded`).
- Multipart: the first `text/plain` part is picked over a later `text/html`
  part (`multipart_picks_the_first_text_plain_part`); a multipart message
  with no text part is a bounded failure, not an empty or wrong result
  (`multipart_without_a_text_part_is_a_bounded_failure`); a `multipart/*`
  Content-Type with no boundary parameter is a bounded failure
  (`multipart_without_a_boundary_parameter_is_a_bounded_failure`).
- Malformed input: invalid UTF-8, an unsupported Content-Transfer-Encoding,
  an unsupported charset, and a malformed header block are each bounded
  failures, not crashes (`eml_invalid_utf8_is_a_bounded_failure_not_a_crash`
  and this crate's own `unsupported_transfer_encoding_is_a_bounded_failure`,
  `unsupported_charset_is_a_bounded_failure`,
  `malformed_header_block_is_a_bounded_failure`).
- Empty input is a valid, non-failure, empty observation list
  (`empty_message_produces_an_empty_observation_list_not_a_failure`).
- Adapter-level round trip against the real built image, including a
  header/body observation shape check, an invalid-UTF-8 bounded failure, an
  oversized-input bounded failure, and an empty-input non-failure
  (`crates/nexo-extraction/src/lib.rs`'s `eml_*` tests).
- API-level round trip through the real sandbox and the real database:
  `.eml` evidence produces `header:*` and `body:line:*` observations
  distinguishable from the plain-text extractor's `line:*`-only output, and
  an unsupported-encoding message surfaces as a bounded `rejection_reason`,
  never an HTTP error
  (`crates/nexo-api/tests/api_test.rs`'s
  `eml_evidence_is_extracted_into_header_and_body_observations` and
  `eml_evidence_with_unsupported_encoding_is_a_bounded_rejection`).
- Isolation mechanism: covered generically by `nexo-sandbox`'s own tests,
  per "Isolation mechanism" above — this extractor adds no new isolation
  surface to test.

## Reproducing the image

```
scripts/build_extractors.sh
```

Builds the musl binary and the `nexo-extractor-eml:local` image alongside
the plain-text extractor's. Neither the compiled binary nor the built image
is committed to the repository; both are reproducible from source on
demand.

## Non-goals

- **Attachments.** Any MIME part other than the first `text/plain` part is
  ignored, not extracted as a separate artifact. A message whose evidentiary
  content is in an attached file, not the message body, is not yet handled
  by this extractor — a real limitation, recorded in
  `docs/KNOWN_LIMITATIONS.md`, not silently dropped.
- **Nested multipart.** Only the immediate parts of a `multipart/*` message
  are scanned; a part that is itself `multipart/*` (e.g. an alternative
  block nested inside a mixed message) is skipped, not recursed into.
- **HTML bodies.** A message whose only part is `text/html`, with no
  `text/plain` alternative, is a bounded failure (`no_text_part_found`),
  not a best-effort HTML-to-text conversion.
- **Charsets other than UTF-8/US-ASCII.** A declared `charset` outside that
  pair is a bounded failure, never an attempted transliteration that could
  silently misrepresent the original text.
- **Interpreting extracted text** (sentiment, entity extraction,
  translation): belongs to a future, separately gated extractor or to
  `nexo-core`'s `Inference` machinery, never to this extractor — same
  boundary as the plain-text extractor's.
- **Producing `Observation` graph nodes directly**: this extractor and its
  adapter return `ObservationCandidate` values; promoting one into a real
  `nexo_core::ObservationNode` is an application-layer decision, same as
  for the plain-text extractor.
