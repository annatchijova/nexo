//! Transactional audit_chain/v1 writer and database projection.
//!
//! The chain is deliberately narrow: it records mutating application events,
//! not every derived explanation or every read. The caller must use the same
//! transaction for the state mutation and this append.

use chrono::{DateTime, Utc};
use nexo_integrity::{
    AuditHmacKeyring, Sha256Digest, audit_checkpoint_hmac_v1, audit_digest_v2, audit_hmac_v1,
    canonical_json,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};

use crate::repository::{ActorRowId, CaseRowId};

pub const AUDIT_CHAIN_ID: &str = "mutations";
pub const AUDIT_CHAIN_VERSION: u64 = 2;
pub const AUDIT_AUTH_SCHEME: &str = "hmac-sha256";
pub const AUDIT_GENESIS: Sha256Digest = Sha256Digest::zero();

pub type Tx<'a> = Transaction<'a, Postgres>;

static AUDIT_KEYRING: std::sync::OnceLock<Result<AuditHmacKeyring, String>> =
    std::sync::OnceLock::new();

fn configured_keyring() -> Result<&'static AuditHmacKeyring, AuditError> {
    match AUDIT_KEYRING.get_or_init(AuditHmacKeyring::from_environment) {
        Ok(keyring) => Ok(keyring),
        Err(error) => Err(AuditError::HmacKeyUnavailable(error.clone())),
    }
}

#[derive(Debug)]
pub enum AuditError {
    Database(sqlx::Error),
    CanonicalPayload,
    InvalidProvenanceRefs,
    InvalidDigest,
    HmacKeyUnavailable(String),
    HmacKeyVersionMismatch(String),
}

impl From<sqlx::Error> for AuditError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AuditEvent {
    pub sequence: i64,
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub previous_digest: Sha256Digest,
    pub entry_digest: Sha256Digest,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditExport {
    pub schema_version: u64,
    pub chain_id: String,
    pub chain_version: u64,
    pub auth_scheme: String,
    pub hmac_key_version: String,
    pub genesis_digest: String,
    pub events: Vec<AuditExportEvent>,
    pub checkpoint: AuditCheckpoint,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditExportEvent {
    pub sequence: u64,
    pub event_id: String,
    pub occurred_at: String,
    pub actor_id: i64,
    pub case_id: Option<i64>,
    pub event_kind: String,
    pub event_payload: Value,
    pub provenance_refs: Value,
    pub previous_digest: String,
    pub entry_digest: String,
    pub auth_scheme: String,
    pub hmac_key_version: String,
    pub entry_hmac: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditCheckpoint {
    pub chain_version: u64,
    pub auth_scheme: String,
    pub hmac_key_version: String,
    pub at_sequence: u64,
    pub tip_digest: String,
    pub tip_hmac: String,
    pub entry_count: u64,
}

/// Append one event while holding the chain row lock. The caller owns commit.
pub async fn append(
    tx: &mut Tx<'_>,
    actor: ActorRowId,
    case: Option<CaseRowId>,
    event_kind: &str,
    event_payload: Value,
    provenance_refs: Value,
) -> Result<AuditEvent, AuditError> {
    if event_kind.is_empty() || !provenance_refs.is_array() {
        return Err(AuditError::InvalidProvenanceRefs);
    }

    let chain = sqlx::query(
        "SELECT chain_version, auth_scheme, hmac_key_version, current_sequence,
                current_tip, current_tip_hmac
         FROM audit_chains WHERE chain_id = $1 FOR UPDATE",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_one(&mut **tx)
    .await?;
    let chain_version: i64 = chain.try_get("chain_version")?;
    let auth_scheme: String = chain.try_get("auth_scheme")?;
    let hmac_key_version: String = chain.try_get("hmac_key_version")?;
    if auth_scheme != AUDIT_AUTH_SCHEME
        || u64::try_from(chain_version).ok() != Some(AUDIT_CHAIN_VERSION)
    {
        return Err(AuditError::HmacKeyVersionMismatch(
            "database chain is not audit_chain/v2".into(),
        ));
    }
    let keyring = configured_keyring()?;
    if keyring.current_version() != hmac_key_version {
        return Err(AuditError::HmacKeyVersionMismatch(hmac_key_version.clone()));
    }
    let key = keyring
        .key(&hmac_key_version)
        .ok_or_else(|| AuditError::HmacKeyVersionMismatch(hmac_key_version.clone()))?;
    let current_sequence: i64 = chain.try_get("current_sequence")?;
    let previous_bytes: Vec<u8> = chain.try_get("current_tip")?;
    let previous_digest = digest_from_bytes(&previous_bytes)?;
    let current_tip_hmac = digest_from_bytes(&chain.try_get::<Vec<u8>, _>("current_tip_hmac")?)?;
    if current_sequence > 0 {
        let checkpoint = sqlx::query(
            "SELECT chain_version, auth_scheme, hmac_key_version,
                    at_sequence, tip_digest, tip_hmac, entry_count
             FROM audit_checkpoints WHERE chain_id = $1
             ORDER BY at_sequence DESC LIMIT 1",
        )
        .bind(AUDIT_CHAIN_ID)
        .fetch_one(&mut **tx)
        .await?;
        let checkpoint_version: i64 = checkpoint.try_get("chain_version")?;
        let checkpoint_auth: String = checkpoint.try_get("auth_scheme")?;
        let checkpoint_key_version: String = checkpoint.try_get("hmac_key_version")?;
        let checkpoint_sequence: i64 = checkpoint.try_get("at_sequence")?;
        let checkpoint_tip = digest_from_bytes(&checkpoint.try_get::<Vec<u8>, _>("tip_digest")?)?;
        let checkpoint_hmac = digest_from_bytes(&checkpoint.try_get::<Vec<u8>, _>("tip_hmac")?)?;
        let checkpoint_count: i64 = checkpoint.try_get("entry_count")?;
        let checkpoint_key = keyring
            .key(&checkpoint_key_version)
            .ok_or_else(|| AuditError::HmacKeyVersionMismatch(checkpoint_key_version.clone()))?;
        let expected_checkpoint_hmac = audit_checkpoint_hmac_v1(
            checkpoint_key,
            &checkpoint_key_version,
            AUDIT_CHAIN_ID,
            AUDIT_CHAIN_VERSION,
            u64::try_from(checkpoint_sequence).map_err(|_| AuditError::InvalidDigest)?,
            checkpoint_tip,
            u64::try_from(checkpoint_count).map_err(|_| AuditError::InvalidDigest)?,
        );
        if checkpoint_version != chain_version
            || checkpoint_auth != AUDIT_AUTH_SCHEME
            || checkpoint_sequence != current_sequence
            || checkpoint_count != current_sequence
            || checkpoint_tip != previous_digest
            || checkpoint_hmac != expected_checkpoint_hmac
            || current_tip_hmac != checkpoint_hmac
        {
            return Err(AuditError::InvalidDigest);
        }
    }
    let sequence = current_sequence
        .checked_add(1)
        .ok_or(AuditError::InvalidDigest)?;
    let event_id = format!("{}:{sequence}", AUDIT_CHAIN_ID);
    let occurred_at = Utc::now();

    let event = event_value(
        actor,
        case,
        &event_id,
        occurred_at,
        event_kind,
        event_payload.clone(),
        provenance_refs.clone(),
    );
    let canonical = canonical_json(&event).map_err(|_| AuditError::CanonicalPayload)?;
    let entry_digest = audit_digest_v2(
        AUDIT_CHAIN_ID,
        u64::try_from(chain_version).map_err(|_| AuditError::InvalidDigest)?,
        u64::try_from(sequence).map_err(|_| AuditError::InvalidDigest)?,
        &canonical,
        previous_digest,
    );
    let entry_hmac = audit_hmac_v1(
        key,
        &hmac_key_version,
        AUDIT_CHAIN_ID,
        AUDIT_CHAIN_VERSION,
        u64::try_from(sequence).map_err(|_| AuditError::InvalidDigest)?,
        &canonical,
        previous_digest,
        entry_digest,
    );
    let checkpoint_hmac = audit_checkpoint_hmac_v1(
        key,
        &hmac_key_version,
        AUDIT_CHAIN_ID,
        AUDIT_CHAIN_VERSION,
        u64::try_from(sequence).map_err(|_| AuditError::InvalidDigest)?,
        entry_digest,
        u64::try_from(sequence).map_err(|_| AuditError::InvalidDigest)?,
    );

    sqlx::query(
        "INSERT INTO audit_log
            (chain_id, chain_version, sequence, event_id, case_id, actor_id,
             occurred_at, event_kind, event_payload, provenance_refs,
             previous_entry_hash, entry_hash, auth_scheme, hmac_key_version,
             entry_hmac)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                 $13, $14, $15)",
    )
    .bind(AUDIT_CHAIN_ID)
    .bind(chain_version)
    .bind(sequence)
    .bind(&event_id)
    .bind(case.map(|value| value.0))
    .bind(actor.0)
    .bind(occurred_at)
    .bind(event_kind)
    .bind(event_payload)
    .bind(provenance_refs)
    .bind(previous_digest.as_bytes().as_slice())
    .bind(entry_digest.as_bytes().as_slice())
    .bind(AUDIT_AUTH_SCHEME)
    .bind(&hmac_key_version)
    .bind(entry_hmac.as_bytes().as_slice())
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE audit_chains
         SET current_sequence = $2, current_tip = $3, current_tip_hmac = $4
         WHERE chain_id = $1",
    )
    .bind(AUDIT_CHAIN_ID)
    .bind(sequence)
    .bind(entry_digest.as_bytes().as_slice())
    .bind(checkpoint_hmac.as_bytes().as_slice())
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO audit_checkpoints
            (chain_id, chain_version, auth_scheme, hmac_key_version,
             at_sequence, tip_digest, tip_hmac, entry_count)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $5)",
    )
    .bind(AUDIT_CHAIN_ID)
    .bind(chain_version)
    .bind(AUDIT_AUTH_SCHEME)
    .bind(&hmac_key_version)
    .bind(sequence)
    .bind(entry_digest.as_bytes().as_slice())
    .bind(checkpoint_hmac.as_bytes().as_slice())
    .execute(&mut **tx)
    .await?;

    Ok(AuditEvent {
        sequence,
        event_id,
        occurred_at,
        previous_digest,
        entry_digest,
    })
}

/// Moves future events to the configured current key version after confirming
/// that the retained checkpoint can still be authenticated with the keyring.
/// Past events keep their original public key version and HMAC; callers must
/// retain their old verification keys during the retention window.
pub async fn rotate_key_version(pool: &crate::repository::Pool) -> Result<(), AuditError> {
    let keyring = configured_keyring()?;
    let current_version = keyring.current_version();
    let mut tx = pool.begin().await?;
    let chain = sqlx::query(
        "SELECT chain_version, hmac_key_version, current_sequence, current_tip
         FROM audit_chains WHERE chain_id = $1 FOR UPDATE",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_one(&mut *tx)
    .await?;
    let chain_version: i64 = chain.try_get("chain_version")?;
    let stored_version: String = chain.try_get("hmac_key_version")?;
    if u64::try_from(chain_version).ok() != Some(AUDIT_CHAIN_VERSION) {
        return Err(AuditError::HmacKeyVersionMismatch(
            "database chain is not audit_chain/v2".into(),
        ));
    }
    if keyring.key(&stored_version).is_none() {
        return Err(AuditError::HmacKeyVersionMismatch(stored_version));
    }
    if stored_version != current_version {
        sqlx::query("UPDATE audit_chains SET hmac_key_version = $2 WHERE chain_id = $1")
            .bind(AUDIT_CHAIN_ID)
            .bind(current_version)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Materializes a stable authenticated audit-export-v2 document. The caller may write this
/// JSON to a separately retained destination and verify it with nexo-verify.
pub async fn export(pool: &crate::repository::Pool) -> Result<AuditExport, AuditError> {
    let chain = sqlx::query(
        "SELECT chain_version, auth_scheme, hmac_key_version, genesis_digest
             FROM audit_chains WHERE chain_id = $1",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_one(pool)
    .await?;
    let chain_version: i64 = chain.try_get("chain_version")?;
    let auth_scheme: String = chain.try_get("auth_scheme")?;
    let hmac_key_version: String = chain.try_get("hmac_key_version")?;
    let keyring = configured_keyring()?;
    if keyring.key(&hmac_key_version).is_none() {
        return Err(AuditError::HmacKeyVersionMismatch(hmac_key_version.clone()));
    }
    let genesis: Vec<u8> = chain.try_get("genesis_digest")?;
    let genesis = digest_from_bytes(&genesis)?.to_string();

    let rows = sqlx::query(
        "SELECT sequence, event_id, occurred_at, actor_id, case_id, event_kind,
                event_payload, provenance_refs, previous_entry_hash, entry_hash,
                auth_scheme, hmac_key_version, entry_hmac
         FROM audit_log WHERE chain_id = $1 ORDER BY sequence ASC",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_all(pool)
    .await?;
    let events = rows
        .into_iter()
        .map(|row| {
            let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
            let previous: Vec<u8> = row.try_get("previous_entry_hash")?;
            let entry: Vec<u8> = row.try_get("entry_hash")?;
            let auth_scheme: String = row.try_get("auth_scheme")?;
            let hmac_key_version: String = row.try_get("hmac_key_version")?;
            let hmac: Vec<u8> = row.try_get("entry_hmac")?;
            Ok(AuditExportEvent {
                sequence: u64::try_from(row.try_get::<i64, _>("sequence")?)
                    .map_err(|_| sqlx::Error::Protocol("negative audit sequence".into()))?,
                event_id: row.try_get("event_id")?,
                occurred_at: occurred_at.to_rfc3339(),
                actor_id: row.try_get("actor_id")?,
                case_id: row.try_get("case_id")?,
                event_kind: row.try_get("event_kind")?,
                event_payload: row.try_get("event_payload")?,
                provenance_refs: row.try_get("provenance_refs")?,
                previous_digest: digest_from_bytes(&previous)?.to_string(),
                entry_digest: digest_from_bytes(&entry)?.to_string(),
                auth_scheme,
                hmac_key_version,
                entry_hmac: digest_from_bytes(&hmac)?.to_string(),
            })
        })
        .collect::<Result<Vec<_>, AuditError>>()?;
    let checkpoint = sqlx::query(
        "SELECT chain_version, auth_scheme, hmac_key_version,
                at_sequence, tip_digest, tip_hmac, entry_count
         FROM audit_checkpoints WHERE chain_id = $1
         ORDER BY at_sequence DESC LIMIT 1",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_optional(pool)
    .await?
    .ok_or(AuditError::InvalidDigest)?;
    let tip: Vec<u8> = checkpoint.try_get("tip_digest")?;
    let tip_hmac: Vec<u8> = checkpoint.try_get("tip_hmac")?;
    Ok(AuditExport {
        schema_version: 2,
        chain_id: AUDIT_CHAIN_ID.into(),
        chain_version: u64::try_from(chain_version).map_err(|_| AuditError::InvalidDigest)?,
        auth_scheme,
        hmac_key_version,
        genesis_digest: genesis,
        events,
        checkpoint: AuditCheckpoint {
            chain_version: u64::try_from(checkpoint.try_get::<i64, _>("chain_version")?)
                .map_err(|_| AuditError::InvalidDigest)?,
            auth_scheme: checkpoint.try_get("auth_scheme")?,
            hmac_key_version: checkpoint.try_get("hmac_key_version")?,
            at_sequence: u64::try_from(checkpoint.try_get::<i64, _>("at_sequence")?)
                .map_err(|_| AuditError::InvalidDigest)?,
            tip_digest: digest_from_bytes(&tip)?.to_string(),
            tip_hmac: digest_from_bytes(&tip_hmac)?.to_string(),
            entry_count: u64::try_from(checkpoint.try_get::<i64, _>("entry_count")?)
                .map_err(|_| AuditError::InvalidDigest)?,
        },
    })
}

fn digest_from_bytes(bytes: &[u8]) -> Result<Sha256Digest, AuditError> {
    if bytes.len() != 32 {
        return Err(AuditError::InvalidDigest);
    }
    let mut value = [0_u8; 32];
    value.copy_from_slice(bytes);
    Ok(Sha256Digest::from_bytes(value))
}

pub fn event_value(
    actor: ActorRowId,
    case: Option<CaseRowId>,
    event_id: &str,
    occurred_at: DateTime<Utc>,
    event_kind: &str,
    event_payload: Value,
    provenance_refs: Value,
) -> Value {
    json!({
        "schema_version": 2,
        "actor_id": actor.0,
        "case_id": case.map(|value| value.0),
        "event_id": event_id,
        "event_kind": event_kind,
        "event_payload": event_payload,
        "occurred_at": occurred_at.to_rfc3339(),
        "provenance_refs": provenance_refs,
    })
}
