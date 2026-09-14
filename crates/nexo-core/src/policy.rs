//! Immutable policy bundles and deterministic jurisdiction selection.

use crate::{
    AcquisitionChannel, ArtifactId, AuthorityKind, CivilDate, DigestId, MAX_LEGAL_SUPPORT,
    NonEmptyText, NormativeClaimId, NormativeSource, PolicyBundleId, ProvenanceId, UtcInstant,
    ValidityInterval,
};

pub const MAX_POLICY_BUNDLES: usize = 64;
pub const MAX_SOURCE_POLICY_ENTRIES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PolicySchemaVersion(u16);

impl PolicySchemaVersion {
    pub const CURRENT: Self = Self(1);

    pub const fn try_new(value: u16) -> Result<Self, PolicySchemaVersionError> {
        if value == 0 {
            Err(PolicySchemaVersionError::Zero)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicySchemaVersionError {
    Zero,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyVersion(NonEmptyText);

impl PolicyVersion {
    pub fn try_new(value: String) -> Result<Self, crate::TextError> {
        NonEmptyText::try_new(value).map(Self)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum JurisdictionCode {
    Argentina,
    UnitedStates,
}

impl JurisdictionCode {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Argentina => "AR",
            Self::UnitedStates => "US",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourcePolicy {
    authorities: Vec<AuthorityKind>,
    channels: Vec<AcquisitionChannel>,
}

impl SourcePolicy {
    pub fn try_new(
        authorities: Vec<AuthorityKind>,
        channels: Vec<AcquisitionChannel>,
    ) -> Result<Self, SourcePolicyError> {
        if authorities.is_empty() || channels.is_empty() {
            return Err(SourcePolicyError::Empty);
        }
        if authorities.len() > MAX_SOURCE_POLICY_ENTRIES
            || channels.len() > MAX_SOURCE_POLICY_ENTRIES
        {
            return Err(SourcePolicyError::TooManyEntries);
        }
        if has_duplicate(&authorities) || has_duplicate(&channels) {
            return Err(SourcePolicyError::DuplicateEntry);
        }
        if authorities.contains(&AuthorityKind::Unverified) {
            return Err(SourcePolicyError::UnverifiedAuthority);
        }
        Ok(Self {
            authorities,
            channels,
        })
    }

    pub fn allows(&self, authority: AuthorityKind, channel: AcquisitionChannel) -> bool {
        self.authorities.contains(&authority) && self.channels.contains(&channel)
    }

    pub fn authorities(&self) -> &[AuthorityKind] {
        &self.authorities
    }

    pub fn channels(&self) -> &[AcquisitionChannel] {
        &self.channels
    }

    pub fn source_is_eligible(&self, source: &NormativeSource) -> bool {
        self.allows(source.authority_kind(), source.acquisition_channel())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePolicyError {
    Empty,
    TooManyEntries,
    DuplicateEntry,
    UnverifiedAuthority,
}

/// A capture whose bytes were verified by an external integrity boundary.
///
/// The fields are private so safe callers cannot manufacture an attestation
/// from serialized metadata. The bridge is the only intended caller of the
/// `unsafe` constructor, after recomputing and comparing the capture digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCaptureAttestation {
    captured_artifact: ArtifactId,
    digest: DigestId,
    provenance: ProvenanceId,
    retrieved_at: UtcInstant,
}

impl VerifiedCaptureAttestation {
    /// Creates an attestation after an external verifier has checked exact
    /// captured bytes against the expected digest.
    ///
    /// # Safety
    ///
    /// The caller must have compared the exact captured bytes with the
    /// expected digest using the integrity boundary immediately beforehand.
    pub unsafe fn from_verified_capture(
        captured_artifact: ArtifactId,
        digest: DigestId,
        provenance: ProvenanceId,
        retrieved_at: UtcInstant,
    ) -> Self {
        Self {
            captured_artifact,
            digest,
            provenance,
            retrieved_at,
        }
    }

    pub const fn captured_artifact(&self) -> ArtifactId {
        self.captured_artifact
    }

    pub const fn digest(&self) -> DigestId {
        self.digest
    }

    pub const fn provenance(&self) -> ProvenanceId {
        self.provenance
    }

    pub const fn retrieved_at(&self) -> UtcInstant {
        self.retrieved_at
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyBundle {
    id: PolicyBundleId,
    schema_version: PolicySchemaVersion,
    policy_version: PolicyVersion,
    jurisdiction: JurisdictionCode,
    validity: ValidityInterval,
    claims: Vec<NormativeClaimId>,
    source_policy: SourcePolicy,
    captured_artifact: ArtifactId,
    digest: DigestId,
    provenance: ProvenanceId,
    retrieved_at: UtcInstant,
}

impl PolicyBundle {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: PolicyBundleId,
        schema_version: PolicySchemaVersion,
        policy_version: PolicyVersion,
        jurisdiction: JurisdictionCode,
        validity: ValidityInterval,
        claims: Vec<NormativeClaimId>,
        source_policy: SourcePolicy,
        capture: VerifiedCaptureAttestation,
    ) -> Result<Self, PolicyBundleError> {
        if schema_version != PolicySchemaVersion::CURRENT {
            return Err(PolicyBundleError::UnsupportedSchemaVersion);
        }
        if claims.is_empty() {
            return Err(PolicyBundleError::MissingClaim);
        }
        if claims.len() > MAX_LEGAL_SUPPORT {
            return Err(PolicyBundleError::TooManyClaims);
        }
        if has_duplicate(&claims) {
            return Err(PolicyBundleError::DuplicateClaim);
        }
        Ok(Self {
            id,
            schema_version,
            policy_version,
            jurisdiction,
            validity,
            claims,
            source_policy,
            captured_artifact: capture.captured_artifact,
            digest: capture.digest,
            provenance: capture.provenance,
            retrieved_at: capture.retrieved_at,
        })
    }

    pub const fn id(&self) -> PolicyBundleId {
        self.id
    }

    pub const fn schema_version(&self) -> PolicySchemaVersion {
        self.schema_version
    }

    pub fn policy_version(&self) -> &PolicyVersion {
        &self.policy_version
    }

    pub const fn jurisdiction(&self) -> JurisdictionCode {
        self.jurisdiction
    }

    pub const fn validity(&self) -> ValidityInterval {
        self.validity
    }

    pub fn claims(&self) -> &[NormativeClaimId] {
        &self.claims
    }

    pub fn source_policy(&self) -> &SourcePolicy {
        &self.source_policy
    }

    pub fn source_is_eligible(&self, source: &NormativeSource) -> bool {
        self.source_policy.source_is_eligible(source)
    }

    pub const fn captured_artifact(&self) -> ArtifactId {
        self.captured_artifact
    }

    pub const fn digest(&self) -> DigestId {
        self.digest
    }

    pub const fn provenance(&self) -> ProvenanceId {
        self.provenance
    }

    pub const fn retrieved_at(&self) -> UtcInstant {
        self.retrieved_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyBundleError {
    UnsupportedSchemaVersion,
    MissingClaim,
    TooManyClaims,
    DuplicateClaim,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyBundleSet {
    bundles: Vec<PolicyBundle>,
}

impl PolicyBundleSet {
    pub fn try_new(bundles: Vec<PolicyBundle>) -> Result<Self, PolicyBundleSetError> {
        if bundles.len() > MAX_POLICY_BUNDLES {
            return Err(PolicyBundleSetError::TooManyBundles);
        }
        if bundles.iter().enumerate().any(|(index, bundle)| {
            bundles[index + 1..]
                .iter()
                .any(|other| other.id() == bundle.id())
        }) {
            return Err(PolicyBundleSetError::DuplicateBundle);
        }
        Ok(Self { bundles })
    }

    pub fn bundles(&self) -> &[PolicyBundle] {
        &self.bundles
    }

    pub fn select(
        &self,
        requested_jurisdiction: JurisdictionCode,
        reference_date: CivilDate,
    ) -> PolicySelection {
        let matching: Vec<&PolicyBundle> = self
            .bundles
            .iter()
            .filter(|bundle| bundle.jurisdiction() == requested_jurisdiction)
            .collect();
        if matching.is_empty() {
            return if self.bundles.is_empty() {
                PolicySelection::NoBundle
            } else {
                PolicySelection::OutOfJurisdiction
            };
        }
        let current: Vec<&PolicyBundle> = matching
            .iter()
            .copied()
            .filter(|bundle| bundle.validity().contains(reference_date))
            .collect();
        match current.as_slice() {
            [] => PolicySelection::PolicyNotCurrent,
            [bundle] => PolicySelection::Selected {
                bundle_id: bundle.id(),
                policy_version: bundle.policy_version().clone(),
            },
            _ => PolicySelection::AmbiguousSelection,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyBundleSetError {
    TooManyBundles,
    DuplicateBundle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicySelection {
    Selected {
        bundle_id: PolicyBundleId,
        policy_version: PolicyVersion,
    },
    NoBundle,
    OutOfJurisdiction,
    PolicyNotCurrent,
    AmbiguousSelection,
}

fn has_duplicate<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[index + 1..].contains(value))
}
