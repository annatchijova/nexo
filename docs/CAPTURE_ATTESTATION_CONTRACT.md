# Capture attestation boundary

## Purpose

This contract connects policy-bundle construction to `nexo-integrity` without
making the dependency-free domain kernel pretend that an enum variant is a
cryptographic proof. The bridge verifies bytes first, then supplies an explicit
attestation to `PolicyBundle`.

## Trust model

The bridge receives untrusted serialized bundle metadata and captured bytes.
It may read the object store and call `nexo-integrity`; `nexo-core` may not.
The bridge cannot change the expected digest, artifact identity, or provenance
after verification. A caller-controlled `CaptureStatus::Verified` field is
never accepted as evidence by itself.

## Verification sequence

```text
serialized bundle metadata + captured bytes
            │
            ├─ parse and bound fields
            ├─ recompute SHA-256 with nexo-integrity
            ├─ compare recomputed digest to expected digest
            ├─ bind artifact identity, digest, provenance, and retrieval event
            └─ issue VerifiedCaptureAttestation
                         │
                         ▼
                 nexo-core::PolicyBundle
```

The comparison is over the exact captured bytes, not a URL, normalized text,
or generated summary. A mismatch rejects the import. There is no “best effort”
or warning-only path.

## Attestation properties

`VerifiedCaptureAttestation` is an application-layer value containing the
verified artifact/digest/provenance references and the verification event. It
is not deserialized from user input. It is created only by the bridge after a
successful integrity check and consumed immediately to construct the immutable
bundle.

`VerifiedCaptureAttestation` is the constructor capability for the
dependency-free prototype. Its fields are private; the bridge is the only
production caller that should invoke the documented `unsafe` minting boundary.
Untrusted serialized status is ignored entirely.

## Replay and identity

- Replaying identical bytes with identical digest is the same capture identity
  only when the artifact/provenance references intentionally identify the same
  event.
- Reusing a digest with different bytes fails verification.
- Reusing a verified status with different bytes is not evidence; the bridge
  recomputes the digest for every import.
- A later retrieval with changed bytes creates a new capture and must not mutate
  a historical bundle.

## Failure semantics

| Condition | Result |
| --- | --- |
| Missing/oversized metadata or bytes | reject before allocation/verification |
| Recomputed digest differs from expected | reject capture |
| Unknown schema | reject or migrate through an explicit version chain |
| Serialized `Verified` flag without bridge verification | reject/ignore flag |
| Valid capture but ineligible authority/channel | policy-level rejection |

## Required evidence

The bridge test suite must include a known SHA-256 vector, one-byte mutation,
line-ending mutation, digest/reference mismatch, replay with changed bytes, and
an input that marks itself `Verified` without a verification call. The last
case must fail even though the status text is affirmative.

## Non-goals

- This bridge does not decide legal truth or source hierarchy.
- It does not select a jurisdictional policy or evaluate case facts.
- It does not make external legal requests or send data.
