//! The only supported path from captured bytes to a verified policy attestation.
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
}
