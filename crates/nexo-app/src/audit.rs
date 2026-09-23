//! Transactional audit_chain/v1 writer and database projection.
//!
//! The chain is deliberately narrow: it records mutating application events,
//! not every derived explanation or every read. The caller must use the same
//! transaction for the state mutation and this append.

use chrono::{DateTime, Utc};
use nexo_integrity::{audit_digest_v1, canonical_json, Sha256Digest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};

use crate::repository::{ActorRowId, CaseRowId};

pub const AUDIT_CHAIN_ID: &str = "mutations";
pub const AUDIT_CHAIN_VERSION: u64 = 1;
pub const AUDIT_GENESIS: Sha256Digest = Sha256Digest::zero();

pub type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Debug)]
pub enum AuditError {
    Database(sqlx::Error),
    CanonicalPayload,
    InvalidProvenanceRefs,
    InvalidDigest,
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditCheckpoint {
    pub chain_version: u64,
    pub at_sequence: u64,
    pub tip_digest: String,
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
        "SELECT chain_version, current_sequence, current_tip
         FROM audit_chains WHERE chain_id = $1 FOR UPDATE",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_one(&mut **tx)
    .await?;
    let chain_version: i64 = chain.try_get("chain_version")?;
    let current_sequence: i64 = chain.try_get("current_sequence")?;
    let previous_bytes: Vec<u8> = chain.try_get("current_tip")?;
    let previous_digest = digest_from_bytes(&previous_bytes)?;
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
    let entry_digest = audit_digest_v1(
        AUDIT_CHAIN_ID,
        u64::try_from(chain_version).map_err(|_| AuditError::InvalidDigest)?,
        u64::try_from(sequence).map_err(|_| AuditError::InvalidDigest)?,
        &canonical,
        previous_digest,
    );

    sqlx::query(
        "INSERT INTO audit_log
            (chain_id, chain_version, sequence, event_id, case_id, actor_id,
             occurred_at, event_kind, event_payload, provenance_refs,
             previous_entry_hash, entry_hash)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE audit_chains SET current_sequence = $2, current_tip = $3
         WHERE chain_id = $1",
    )
    .bind(AUDIT_CHAIN_ID)
    .bind(sequence)
    .bind(entry_digest.as_bytes().as_slice())
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO audit_checkpoints
            (chain_id, chain_version, at_sequence, tip_digest, entry_count)
         VALUES ($1, $2, $3, $4, $3)",
    )
    .bind(AUDIT_CHAIN_ID)
    .bind(chain_version)
    .bind(sequence)
    .bind(entry_digest.as_bytes().as_slice())
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

/// Materializes a stable audit-export-v1 document. The caller may write this
/// JSON to a separately retained destination and verify it with nexo-verify.
pub async fn export(pool: &crate::repository::Pool) -> Result<AuditExport, AuditError> {
    let chain =
        sqlx::query("SELECT chain_version, genesis_digest FROM audit_chains WHERE chain_id = $1")
            .bind(AUDIT_CHAIN_ID)
            .fetch_one(pool)
            .await?;
    let chain_version: i64 = chain.try_get("chain_version")?;
    let genesis: Vec<u8> = chain.try_get("genesis_digest")?;
    let genesis = digest_from_bytes(&genesis)?.to_string();

    let rows = sqlx::query(
        "SELECT sequence, event_id, occurred_at, actor_id, case_id, event_kind,
                event_payload, provenance_refs, previous_entry_hash, entry_hash
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
            })
        })
        .collect::<Result<Vec<_>, AuditError>>()?;
    let checkpoint = sqlx::query(
        "SELECT chain_version, at_sequence, tip_digest, entry_count
         FROM audit_checkpoints WHERE chain_id = $1
         ORDER BY at_sequence DESC LIMIT 1",
    )
    .bind(AUDIT_CHAIN_ID)
    .fetch_optional(pool)
    .await?
    .ok_or(AuditError::InvalidDigest)?;
    let tip: Vec<u8> = checkpoint.try_get("tip_digest")?;
    Ok(AuditExport {
        schema_version: 1,
        chain_id: AUDIT_CHAIN_ID.into(),
        chain_version: u64::try_from(chain_version).map_err(|_| AuditError::InvalidDigest)?,
        genesis_digest: genesis,
        events,
        checkpoint: AuditCheckpoint {
            chain_version: u64::try_from(checkpoint.try_get::<i64, _>("chain_version")?)
                .map_err(|_| AuditError::InvalidDigest)?,
            at_sequence: u64::try_from(checkpoint.try_get::<i64, _>("at_sequence")?)
                .map_err(|_| AuditError::InvalidDigest)?,
            tip_digest: digest_from_bytes(&tip)?.to_string(),
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
        "schema_version": 1,
        "actor_id": actor.0,
        "case_id": case.map(|value| value.0),
        "event_id": event_id,
        "event_kind": event_kind,
        "event_payload": event_payload,
        "occurred_at": occurred_at.to_rfc3339(),
        "provenance_refs": provenance_refs,
    })
}
