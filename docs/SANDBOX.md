# Artifact sandbox contract

NEXO will receive hostile evidence. An artifact is data, never code to run, and its filename, MIME declaration, OCR output, embedded text, and metadata are all untrusted assertions.

## Non-negotiable boundary

The API process may hash, size-check, and persist a byte stream. It must not execute, open, preview, convert, OCR, decompress, or parse that artifact.

Those operations belong to a dedicated worker sandbox with:

- no host filesystem mounts except a per-job read-only input and write-only result directory;
- no application database credentials, object-store write capability, shell access, or inherited environment secrets;
- network disabled by default;
- explicit CPU, memory, wall-clock, file-count, output-size, and decompression-ratio limits;
- a typed extraction result that can contain observations or a bounded failure record, never arbitrary executable output.

## First implementation gate

Before the first extractor is accepted, its contract must name supported formats, rejection behavior, resource limits, malformed-input tests, archive-bomb tests where relevant, and the container/process isolation mechanism.
