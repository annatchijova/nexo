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

For the selected mutating-event history, the independent verifier accepts an
authenticated `audit-export-v3` document:

```text
cargo run -p nexo-verifier -- audit path/to/audit-export.json
```

Audit verification checks the explicit genesis, canonical full-event digest,
previous-digest linkage, sequence continuity, checkpoint, authenticated chain
state, and HMAC as separate properties. The chain is v2 and the export format
is v3. Legacy v2 exports remain parseable but cannot receive the
`complete_history` result because they lack the chain-state witness. The
configured keyring is required and verification never silently falls back to
SHA-only mode. Verification does not claim truth of event content,
independent time witnessing, or resistance to an operator who controls the
complete application deployment and HMAC secret.
