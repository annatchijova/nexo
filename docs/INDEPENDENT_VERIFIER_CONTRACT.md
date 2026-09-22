# Independent verifier contract

`nexo-verify` validates an export without importing the API, application
repository, or PostgreSQL. Its input is a directory containing a
`manifest.json` and the artifact files named by that manifest.

The manifest shape is versioned:

```json
{
  "schema_version": 1,
  "case_reference": 7,
  "generated_at_unix_seconds": 1700000000,
  "policy_bundle_digest": "<64 lowercase hex characters>",
  "manifest_digest": "<seal of the canonical manifest>",
  "artifacts": [
    {"label": "artifact/1", "digest": "<sha256>", "path": "objects/ab/file"}
  ]
}
```

Verification fails closed when the schema is unknown, a digest is malformed,
the manifest seal does not match, an artifact is missing or has a different
SHA-256, or an artifact path is absolute, traverses a parent directory, or
resolves outside the export root (including through a symlink).

The canonical manifest seal covers the schema version, case reference,
generation instant, policy-bundle digest, and sorted `(label, digest)` pairs.
The transport `path` is deliberately not part of that seal; the verifier
still validates it as a confined filesystem reference before reading bytes.

Usage:

```text
cargo run -p nexo-verifier -- path/to/export/manifest.json
```

Exit code `0` means every check passed. Exit code `1` means the export was
rejected; exit code `2` means the command line was invalid.
