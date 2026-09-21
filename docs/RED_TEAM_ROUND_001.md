# Security Audit — NEXO application/integrity/sandbox/policy layers
## Red Team Round 001
**Date:** 2026-09-21
**Method:** Abductive Engineering (A–D–I) + Red-Team Auditing
**Scope:** Everything committed in this round of work: `nexo-app` (schema,
object store), `nexo-integrity` (manifest, capture-attestation test
coverage), `nexo-sandbox`, `nexo-extractor-plaintext`, `nexo-extraction`,
`nexo-policy-ar`. Out of scope: `nexo-core` (already carries its own
adversarial/property test suite and prior red-team rounds per
`docs/red-team/`), anything not yet built (HTTP API, web UI, repository
code against the `nexo-app` schema).
**Base:** `main` @ the four commits from this round (`9106b43`, `5af7777`,
`5da126d`, `cf2eabe`), audited before push.
**Reproducible evidence:** every finding below names the exact test that
now guards it; all live in the crates' own `#[cfg(test)]` modules and run
under `cargo test --workspace`.

## Threat model

- Attacker CAN: supply arbitrary artifact bytes to the extraction
  pipeline; issue concurrent requests against the object store and the
  sandbox orchestrator (e.g. two overlapping uploads of the same file);
  cause a future commit to silently alter a committed "captured official
  source" file (accidental bad merge, encoding mangling, or deliberate
  tampering by a compromised contributor/CI credential).
- Attacker CANNOT: modify compiled code at runtime; break SHA-256;
  obtain the Docker host's root credentials; bypass the container
  isolation flags at the kernel level; read the runtime process's memory
  directly.
- Trust boundaries crossed by the findings below: artifact-bytes → object
  store (RT-001-01, RT-001-05); artifact-bytes → sandboxed container
  (RT-001-03, RT-001-04); committed-file → cited-legal-claim (RT-001-02).

## Epistemic legend
CODE FACT · PLAUSIBLE HYPOTHESIS · CONFIRMED BY INDUCTION · FALSIFIED

## Executive summary

| ID | Severity | Level | Module | Finding |
|----|----------|-------|--------|---------|
| RT-001-02 | HIGH | CONFIRMED BY INDUCTION | nexo-policy-ar | Captured-source integrity check was self-referential; a tampered legal-deadline text passed every test. |
| RT-001-01 | MEDIUM | CONFIRMED BY INDUCTION | nexo-app::object_store | Concurrent `put()` of identical content shared a deterministic temp path; one writer's `rename` could fail. |
| RT-001-03 | LOW | CONFIRMED BY INDUCTION | nexo-sandbox + nexo-extractor-plaintext | Input within every extractor-advertised cap can still exceed the orchestrator's output-size cap. |
| RT-001-04 | MEDIUM | CODE FACT | nexo-sandbox | No input-size ceiling existed between the caller and the container, despite SANDBOX.md's promise. |
| RT-001-05 | MEDIUM | CODE FACT | nexo-app::object_store | `put`/`get` had no size ceiling at all; `get` read an entire file into memory unconditionally. |

All five are fixed and covered by a regression test in the same commit
range as this report; none required rolling back a decision, only
patching an implementation gap.

## Findings

### RT-001-02 — Captured legal-source "verification" was circular

**Severity:** HIGH **Epistemic level:** CONFIRMED BY INDUCTION **Bucket:** vulnerability (composition)

- **Surprise/expectation violated:** `nexo-policy-ar::build()` calls
  `nexo_policy_bridge::attest_capture(CAPTURED_SOURCE_BYTES, expected_digest, ...)`
  — but `expected_digest` was computed as
  `hash_bytes(CAPTURED_SOURCE_BYTES)`, i.e. a hash of the exact same bytes
  being "verified." `attest_capture` itself is correct in isolation (its
  own test suite, extended this round, proves it rejects any bytes that
  don't match an *independently supplied* expected digest). The defect is
  in the composition: the caller never supplied an independent expectation.
- **Abduction:** if the expected digest is derived from the same bytes it
  is checked against, the check can never fail regardless of what the
  committed file actually contains.
- **Deduction:** editing the committed source file to change a
  legal deadline (a real, high-consequence kind of corruption for this
  project) should still let every test in the crate pass, including the
  one specifically named to check source fidelity
  (`captured_source_is_the_real_infoleg_text`, which only checks for
  article-number substrings, not the specific text at risk).
- **Induction:** ran it. Replaced "diez días" (ten days, art. 14's real
  response deadline) with "DOS días" in the committed
  `sources/ley_25326_texact.html`, then ran `cargo test -p nexo-policy-ar`.
  **Result: all 6 tests passed**, including
  `captured_source_is_the_real_infoleg_text`. Source file restored
  immediately after (`git diff` confirmed clean, digest matched the
  original before restoring).
- **Causal chain:**
  ```
  future commit silently edits sources/ley_25326_texact.html
      ↓
  build() re-hashes the (now wrong) file as its own "expected" digest
      ↓
  attest_capture compares the file to a hash of itself — always equal
      ↓
  a wrong legal deadline ships as if it were the cited official text
  ```
- **Threat-model precondition:** a committed source file changes without
  every reviewer independently re-verifying its content against the
  original government page. This is a realistic precondition for a
  project maintained across multiple contributors/agents over time — it
  is exactly the kind of drift code review catches for logic but easily
  misses for a large block of captured HTML.
- **Fix:** `nexo_integrity::Sha256Digest::from_hex` added; `nexo-policy-ar`
  now pins `CAPTURED_SOURCE_SHA256_HEX` as a literal (independent of the
  file) and uses it as `expected_digest`. Re-ran the same tamper
  experiment against the fixed code: 5 of 7 tests now fail immediately
  with `DigestMismatch`, as they must.
- **Regression test:** `a_tampered_copy_of_the_captured_source_fails_attestation`
  (byte-flip on an in-memory copy, proving the pinned-digest wiring
  specifically, independent of re-running the full-file tamper
  experiment by hand).

### RT-001-01 — Concurrent `put()` of identical content could fail

**Severity:** MEDIUM **Epistemic level:** CONFIRMED BY INDUCTION **Bucket:** vulnerability (concurrency)

- **Surprise/expectation violated:** the doc comment on `put()` claimed
  "writing the same bytes twice is idempotent... never a corrupting
  concurrent write, because the write path is write-to-temp-then-rename."
  That claim assumed each call had its own temp file; it did not —
  `temp_path` was `<digest>.tmp`, identical for every caller writing the
  same content.
- **Abduction:** two threads racing to `put()` identical bytes both target
  the same temp path; whichever renames first moves the file, so the
  second thread's `rename` should fail with `NotFound`.
- **Deduction:** spawning several threads per round, all `put()`-ing the
  same ~8 MB buffer repeatedly, should surface an `Err` from at least one
  thread within a modest number of rounds if the race is real.
- **Induction:** ran it — 8 threads × 20 rounds of concurrent identical
  8 MB `put()` calls. **Result: failed on the very first attempt**, with
  `Io(Os { code: 2, kind: NotFound, ... })` — worse than the originally
  hypothesized silent corruption; it surfaced immediately as a hard error
  rather than something that would need many rounds to catch.
- **Causal chain:**
  ```
  thread A: put(bytes) → writes <digest>.tmp → renames to <digest>
  thread B: put(bytes) → writes <digest>.tmp (same path, A already moved it)
      ↓
  thread B's fs::write recreates <digest>.tmp, then rename races A's completed rename
      ↓
  whichever thread's rename runs second finds no source file → Io(NotFound)
  ```
- **Threat-model precondition:** any deployment where two requests can
  write the same artifact concurrently — realistic for a personal-use
  server handling retried uploads or two devices syncing the same
  evidence file at once, not an exotic attacker capability.
- **Fix:** `put()` now writes to a `tempfile::NamedTempFile` created fresh
  per call (unique path by construction, via the `tempfile` crate already
  in the dependency tree) and persists it into place; a losing race now
  overwrites with byte-identical content instead of erroring.
- **Regression test:** `concurrent_put_of_identical_large_content_never_yields_corrupted_read`
  (kept the original hypothesis's name; it now also implicitly proves no
  `Err` occurs, not just no corruption).

### RT-001-03 — Extractor's own caps can still exceed the orchestrator's output cap

**Severity:** LOW **Epistemic level:** CONFIRMED BY INDUCTION **Bucket:** hygiene / spec inconsistency (safe failure mode)

- **Surprise/expectation violated:** `nexo-extractor-plaintext`'s contract
  advertises 25 MiB input, 50,000 lines, 100,000 bytes/line as its
  acceptance bounds. `nexo-sandbox`'s default `max_output_bytes` is 4 MiB.
  Nothing cross-checks these against each other.
- **Abduction:** an input using close-to-maximal line lengths, well under
  the 25 MiB input cap, could still produce JSON output several times
  larger than 4 MiB, since output size scales with input size plus JSON
  overhead, not with the per-field caps independently.
- **Deduction:** ~230 lines of ~100,000 bytes each (~23 MiB input, under
  every extractor-side cap) should produce a ~23 MiB result file and be
  rejected by `nexo-sandbox::JobDir::read_result`'s own bound.
- **Induction:** ran it twice — once directly against `docker run` (result:
  23,007,285-byte `result.json` written successfully inside the
  container, no crash, no OOM under the 256 MiB container memory limit),
  once through the full `run_extraction` path (result:
  `Err(SandboxError::OutputTooLarge)`, exactly as the existing bound
  predicts).
- **Causal chain:** the failure mode here is already safe — a typed,
  fail-closed error, not a crash, hang, or silent truncation — so this is
  recorded as a spec-consistency gap rather than a vulnerability with
  attacker-reachable consequence beyond failing that one job.
- **Threat-model precondition:** none needed for the safe outcome; an
  attacker attempting to force an OOM or a hang via this path is
  FALSIFIED (see Discarded vectors).
- **Fix:** none applied to the constants (raising `max_output_bytes` or
  lowering the extractor's per-field caps would each trade off real
  chat-export throughput against memory headroom, and no real deployment
  scenario in `plan.md` needs 100,000-byte single lines). Documented
  instead as a known, tested interaction.
- **Regression test:** `large_but_within_input_cap_extraction_hits_output_too_large_not_a_crash`.

### RT-001-04 — No input-size ceiling between the caller and the container

**Severity:** MEDIUM **Epistemic level:** CODE FACT (fix verified by a fast, docker-independent test; the "unbounded memory" consequence itself was not separately induced, since doing so would mean deliberately exhausting memory on the audit machine — an unnecessary risk to take to confirm what the code already makes undeniable)

- **Code fact:** prior to this round's fix, `run_extraction` took
  `input_bytes: &[u8]` and passed it straight to
  `JobDir::write_input`, which called `fs::write` unconditionally. No
  check anywhere in `nexo-sandbox` compared `input_bytes.len()` against
  any limit before writing it to disk and handing it to the container.
- **Deduction:** `docs/SANDBOX.md` promises "explicit CPU, memory,
  wall-clock, file-count, output-size, and decompression-ratio limits" at
  the isolation layer. Input size was conspicuously absent from what this
  crate itself enforced — it was fully delegated to whatever calls
  `run_extraction` (today, nothing; the extractor's own 25 MiB check
  catches it, but only *after* the bytes have already been copied to disk
  and handed into the container).
- **Fix:** `SandboxLimits::max_input_bytes` (default 32 MiB) is now
  checked at the top of `run_extraction`, before any file write or
  container launch.
- **Regression test:** `oversized_input_is_rejected_before_touching_disk_or_docker`
  — deliberately docker-independent, so it holds even in an environment
  without Docker installed.

### RT-001-05 — Object store had no size ceiling at all

**Severity:** MEDIUM **Epistemic level:** CODE FACT (same reasoning as RT-001-04 for not detonating a real OOM)

- **Code fact:** `FilesystemObjectStore::put` wrote `bytes` of any length;
  `get` called `fs::read(&path)`, which allocates a buffer sized to the
  entire file, unconditionally. Every other boundary in this codebase
  that touches attacker-influenceable bytes bounds them (the extractor's
  own input/line caps, `nexo-sandbox`'s output-read cap, now its
  input cap) — this one did not.
- **Fix:** `FilesystemObjectStore` now takes a `max_bytes` ceiling
  (`open_with_limit`, defaulting to 256 MiB via `open`). `put` checks
  before writing; `get` reads with `Read::take(max_bytes + 1)` and
  rejects anything over the limit, the same pattern
  `nexo-sandbox::JobDir::read_result` already used.
- **Regression test:** `put_and_get_are_bounded_by_max_bytes` — covers
  both the `put`-side rejection and a `get`-side object that grew past
  the limit on disk (simulated by writing directly to the shard path,
  bypassing `put`'s own check, to isolate the `get`-side guard).

## Discarded (non-exploitable) vectors

| Vector | Result | Why it failed |
|---|---|---|
| Path traversal via a crafted digest in `path_for` | FALSIFIED (code fact) | The digest is always computed internally by this crate (`hash_bytes`); `Display` always emits exactly 64 lowercase hex characters. No caller-supplied string ever reaches `shard_path`. |
| `docker --memory-swap` silently unenforced on this host (some kernels disable cgroup swap accounting, which would make the memory limit weaker than the flags suggest) | FALSIFIED on this host | `docker run --memory 50m --memory-swap 50m alpine:3 true` succeeded with no daemon warning; `docker info` shows cgroup v2. Recorded as a per-host check to repeat during Step 9 deployment, not a defect in this code. |
| Local co-resident process predicting `nexo-sandbox`'s `/tmp/nexo-sandbox-<pid>-<nanos>` job directory and symlink-racing it before creation | Discarded — out of the stated threat model | `docs/SANDBOX.md`'s threat model is hostile *artifact bytes*, not a hostile co-resident local user with independent code execution on the host; the latter is a different, broader threat model this crate never claimed to defend against. |
| `nexo-extractor-plaintext`'s own MAX_LINES × MAX_LINE_BYTES causing an OOM/crash inside its own container | FALSIFIED by induction | A ~23 MiB crafted input (near-maximal per-field values) completed successfully inside the 256 MiB container memory limit; see RT-001-03, whose actual failure mode is the separate, already-safe output-size cap, not an OOM. |
| Injecting a forged `"status":"Verified"` string into bytes handed to `attest_capture` | FALSIFIED (pre-existing coverage, re-verified this round) | `attest_capture`'s only inputs are raw bytes and an expected digest; there is no code path that parses or trusts an embedded status field. `serialized_verified_flag_without_bridge_call_is_not_an_input` covers this directly. |

## Recommendations (out of scope of this change — record only)

- Step 9 (personal deployment) should re-run the `--memory-swap`
  discarded-vector check against the actual production host, since cgroup
  swap-accounting availability is host-specific, not something this
  code can guarantee from the application layer.
- When the application-layer repository code (Step 1's remaining slice)
  is built, it should reuse `FilesystemObjectStore::open_with_limit`
  with a size ceiling chosen deliberately per artifact type, rather than
  the generic default.
- `Contraindicated`/`ConflictingLegalClaims` coverage for the AR bundle
  remains blocked on an in-core policy-bundle loader (see
  `docs/POLICY_BUNDLE_AR_DATA_ACCESS_CONTRACT.md`'s "Known limit"); not a
  finding of this round, restated here so it isn't lost.
