//! The supported safe path from captured bytes to a verified policy attestation.
//!
//! Serialized `verified` flags are intentionally not accepted. This bridge
//! recomputes the digest over the exact bytes on every import and only then
//! emits the capability consumed by `nexo-core::PolicyBundle`.

use nexo_core::{
    ArtifactId, DigestId, ProvenanceId, UtcInstant, VerifiedCaptureAttestation,
};
use nexo_integrity::{hash_bytes, Sha256Digest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureVerificationError {
    DigestMismatch,
}

/// Recompute and compare the digest, then issue the attestation capability.
///
/// The `unsafe` operation is isolated here because `nexo-core` deliberately
/// has no dependency on bytes or hashing. The safety precondition is discharged
/// immediately by the equality check above.
pub fn attest_capture(
    bytes: &[u8],
    expected_digest: Sha256Digest,
    captured_artifact: ArtifactId,
    digest: DigestId,
    provenance: ProvenanceId,
    retrieved_at: UtcInstant,
) -> Result<VerifiedCaptureAttestation, CaptureVerificationError> {
    if hash_bytes(bytes) != expected_digest {
        return Err(CaptureVerificationError::DigestMismatch);
    }
    // SAFETY: exact `bytes` were hashed and matched `expected_digest` directly
    // above; the caller supplies only the graph references for that capture.
    Ok(unsafe {
        VerifiedCaptureAttestation::from_verified_capture(
            captured_artifact,
            digest,
            provenance,
            retrieved_at,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU64;
    use nexo_core::{NodeId, PolicyBundle, PolicyBundleId, PolicySchemaVersion, PolicyVersion,
        JurisdictionCode, ValidityInterval, CivilDate, SourcePolicy, AuthorityKind,
        AcquisitionChannel, NormativeClaimId};

    fn node(value: u64) -> NodeId {
        NodeId::new(NonZeroU64::new(value).unwrap())
    }

    #[test]
    fn forged_verified_flag_is_not_an_input() {
        let bytes = b"policy-v1";
        let expected = hash_bytes(bytes);
        let attestation = attest_capture(
            bytes,
            expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        )
        .unwrap();
        let bundle = PolicyBundle::try_new(
            PolicyBundleId::new(node(4)),
            PolicySchemaVersion::CURRENT,
            PolicyVersion::try_new("v1".into()).unwrap(),
            JurisdictionCode::Argentina,
            ValidityInterval::try_new(CivilDate::try_new(2026, 1, 1).unwrap(), None).unwrap(),
            vec![NormativeClaimId::new(node(5))],
            SourcePolicy::try_new(
                vec![AuthorityKind::PrimaryOfficial],
                vec![AcquisitionChannel::OfficialApi],
            )
            .unwrap(),
            attestation,
        );
        assert!(bundle.is_ok());
    }

    #[test]
    fn one_byte_mutation_cannot_attest() {
        let expected = hash_bytes(b"policy-v1");
        let result = attest_capture(
            b"policy-v2",
            expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert_eq!(result.unwrap_err(), CaptureVerificationError::DigestMismatch);
    }

    // The remaining tests close CAPTURE_ATTESTATION_CONTRACT.md's "Required
    // evidence" list, which named these cases explicitly but did not yet
    // have them all present in this suite.

    #[test]
    fn known_sha256_vector_attests() {
        // NIST/RFC test vector: SHA-256("abc") = ba7816bf8f01cfea...
        let expected_hex =
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let bytes = b"abc";
        let expected = hash_bytes(bytes);
        assert_eq!(expected.to_string(), expected_hex);

        let attestation = attest_capture(
            bytes,
            expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert!(attestation.is_ok());
    }

    #[test]
    fn line_ending_mutation_cannot_attest() {
        let unix = b"line one\nline two\n";
        let crlf = b"line one\r\nline two\r\n";
        let expected = hash_bytes(unix);
        let result = attest_capture(
            crlf,
            expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert_eq!(result.unwrap_err(), CaptureVerificationError::DigestMismatch);
    }

    #[test]
    fn digest_reference_mismatch_cannot_attest() {
        // Correct, self-consistent bytes and digest for a *different* capture
        // supplied as the "expected" digest for these bytes: the bridge must
        // reject on the mismatch, not accept because both sides are
        // individually well-formed hashes.
        let bytes = b"policy-v1";
        let unrelated_expected = hash_bytes(b"an unrelated document");
        let result = attest_capture(
            bytes,
            unrelated_expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert_eq!(result.unwrap_err(), CaptureVerificationError::DigestMismatch);
    }

    #[test]
    fn replay_with_changed_bytes_is_rejected_even_with_prior_success() {
        // A capture that verified once must not make a later, different byte
        // stream verify against the same expected digest: the bridge
        // recomputes on every call and carries no state between them.
        let original = b"policy-v1";
        let expected = hash_bytes(original);
        let first = attest_capture(
            original,
            expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert!(first.is_ok());

        let replayed_with_different_bytes = b"policy-v1-tampered";
        let second = attest_capture(
            replayed_with_different_bytes,
            expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert_eq!(
            second.unwrap_err(),
            CaptureVerificationError::DigestMismatch
        );
    }

    #[test]
    fn serialized_verified_flag_without_bridge_call_is_not_an_input() {
        // There is no code path in this crate that accepts a caller-supplied
        // "verified" boolean/status at all; attest_capture's only inputs are
        // bytes and the expected digest. This test documents that boundary:
        // even a bytes payload that *spells out* an affirmative status
        // string must still fail when it does not hash to the expected
        // digest, proving the text has no evidentiary weight by itself.
        let bytes = b"{\"status\":\"Verified\"}";
        let unrelated_expected = hash_bytes(b"something else entirely");
        let result = attest_capture(
            bytes,
            unrelated_expected,
            ArtifactId::new(node(1)),
            DigestId::new(node(2)),
            ProvenanceId::new(node(3)),
            UtcInstant::from_unix_seconds(0),
        );
        assert_eq!(result.unwrap_err(), CaptureVerificationError::DigestMismatch);
    }
}
