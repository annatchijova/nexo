# Plain-text / chat-export extractor contract

## Purpose

The first extractor accepted under `docs/SANDBOX.md`'s "First implementation
gate." It turns plain-text bytes — a pasted message, a chat export, a `.txt`
file — into candidate `Observation` records: one per non-blank line, with a
`line:<n>` locator that survives round-tripping back to the exact source
line. It performs no interpretation of the text's meaning; that boundary
belongs to `nexo-core`.

## Isolation mechanism

Docker, orchestrated by `crates/nexo-sandbox`. Every invocation runs with:

- `--network none` — verified in `nexo-sandbox`'s
  `sandbox_flags_block_network_and_root_filesystem_write` test, which
  confirms a container using these exact flags cannot reach an external
  host.
- `--read-only` root filesystem plus a small `noexec,nosuid` tmpfs at
  `/tmp` — verified by the same test, which confirms a container using
  these flags cannot write outside the mounted volumes.
- `--cap-drop ALL`, `--security-opt no-new-privileges`, and a fixed
  unprivileged numeric user (`65534:65534`, no host account or group
  mapping).
- Explicit `--memory`, `--memory-swap`, `--cpus`, and `--pids-limit`.
- A host-side wall-clock deadline (`SandboxLimits::wall_clock`) enforced by
  a watcher thread that `docker kill`s the named container if it is
  exceeded — verified by `wall_clock_timeout_kills_a_runaway_container`,
  which proves a container that never exits on its own is actually killed,
  not merely intended to be.
- No `-e`/environment flags at all: the container's environment is exactly
  what its own image sets, never anything inherited from the host process
  running `nexo-sandbox`.

The extractor binary itself (`crates/nexo-extractor-plaintext`) is a
statically linked, zero-Rust-dependency musl binary packaged in a
`FROM scratch` image: no shell, no libc, no package manager, and no second
program a compromised extraction could pivot to inside its own container.

## Supported format

UTF-8 plain text of unbounded internal structure: a chat export, a pasted
message, a `.txt` file. The extractor does not sniff or trust a declared
MIME type or file extension (per `docs/SANDBOX.md`, those are untrusted
assertions); it only ever inspects the bytes themselves.

## Resource limits

Enforced twice, independently — once by the container (`nexo-sandbox`'s
flags) and once inside the extractor binary itself, so a bug or
misconfiguration in one layer does not remove the other:

| Limit | Value | Enforced by |
| --- | --- | --- |
| Input size | 25 MiB | Extractor binary (`MAX_INPUT_BYTES`), checked against `stat` and again against the actual bytes read, so a file that grows between `stat` and `read` is still caught. |
| Line count | 50,000 | Extractor binary (`MAX_LINES`). |
| Single line length | 100,000 bytes | Extractor binary (`MAX_LINE_BYTES`). |
| Container memory | 256 MiB (default; caller-configurable via `SandboxLimits`) | Docker (`--memory`/`--memory-swap`, no swap beyond the memory cap). |
| Container CPU | 0.5 CPUs (default) | Docker (`--cpus`). |
| Process count | 32 (default) | Docker (`--pids-limit`). |
| Wall clock | 10 seconds (default) | `nexo-sandbox` watcher thread + `docker kill`. |
| Result file read back | 4 MiB (default) | `nexo-sandbox::JobDir::read_result`, which reads at most `max_output_bytes + 1` and rejects anything larger rather than allocating unboundedly for the response either. |

Decompression-ratio limits are **not applicable** to this extractor: it
performs no decompression of any kind. An archive-bomb-style test is
therefore out of scope for this extractor specifically; it applies to a
future archive/compressed-format extractor, which must define its own
ratio limit before acceptance.

## Rejection behavior

Every rejection is a typed, bounded failure record —
`{"kind":"failure","reason":"<code>"}` — never a partial result, a crash, or
arbitrary output. Defined reason codes: `input_too_large`, `invalid_utf8`,
`too_many_lines`, `line_too_long`. The extractor's own process exit code is
`0` for every one of these: a bounded failure is a successful run that
correctly identified an artifact it cannot safely process. Exit code `1` is
reserved for the extractor failing to do its job at all (cannot read
`/input/artifact`, cannot write `/output/result.json`), which
`nexo-sandbox` surfaces as `SandboxError::ExtractorCrashed`, distinct from
a bounded failure and never silently treated as "no observations."

## Required evidence

- Malformed input: invalid UTF-8 (`invalid_utf8_is_a_bounded_failure_not_a_crash`
  in `crates/nexo-extraction`), oversized input one byte past the cap
  (`oversized_input_is_a_bounded_failure`).
- Well-formed input, including multi-byte UTF-8 and a blank line correctly
  excluded from the observation list while preserving line numbers for the
  lines around it (`valid_text_produces_observations_with_line_locators`).
- Empty input is a valid, non-failure, empty observation list, not an error
  (`empty_input_produces_an_empty_observation_list_not_a_failure`).
- Archive-bomb tests: not applicable, see above.
- Isolation mechanism: network and root-filesystem-write denial
  (`sandbox_flags_block_network_and_root_filesystem_write`), and wall-clock
  kill of a runaway container (`wall_clock_timeout_kills_a_runaway_container`,
  against `crates/nexo-sandbox/testdata/hostile-sleep`, a test-only image
  that never exits on its own).

## Reproducing the image

```
scripts/build_extractors.sh
```

Builds the musl binary, the `nexo-extractor-plaintext:local` image, and the
test-only hostile images `nexo-sandbox`'s suite depends on. Neither the
compiled binary nor the built image is committed to the repository; both
are reproducible from source on demand, consistent with keeping the
container/process isolation mechanism auditable rather than trusting an
opaque prebuilt artifact.

## Non-goals

- Interpreting extracted text (sentiment, entity extraction, translation):
  that belongs to a future, separately gated extractor or to `nexo-core`'s
  `Inference` machinery, never to this extractor.
- Producing `Observation` graph nodes directly: `nexo-extraction` returns
  `ObservationCandidate` values; promoting one into a real
  `nexo_core::ObservationNode` requires an `ArtifactId`, `ToolVersion`, and
  `UtcInstant` that only the application layer can supply.
- Any format other than plain UTF-8 text. PDF, email, and image/OCR
  extraction are future, separately gated extractors per
  `docs/SANDBOX.md`'s "First implementation gate," not silently handled by
  this one.
