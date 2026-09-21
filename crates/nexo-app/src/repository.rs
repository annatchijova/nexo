//! Transactional repository functions against `migrations/0001_init.sql`,
//! implementing the transaction contract in
//! `docs/APPLICATION_LAYER_CONTRACT.md`: every multi-row mutation commits
//! atomically, case-graph node ids are assigned sequentially inside the
//! transaction under a row lock scoped to the case, and every evaluation
//! records the exact policy-bundle identity that produced it.
//!
//! This module contains no domain logic: every invariant `nexo-core`
//! already enforces in memory is trusted here, not re-derived. Its job is
//! narrower — make already-valid domain state durable, and make illegal
//! states (a node without its payload row, an evaluation with no recorded
//! bundle) impossible to reach through a partial commit.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Postgres, Row, Transaction};

pub type Pool = PgPool;
pub type Tx<'a> = Transaction<'a, Postgres>;

#[derive(Debug)]
pub enum RepoError {
    Db(sqlx::Error),
    /// A route required at least one claim id, but none were supplied —
    /// mirrors `nexo_core::NormativeClaim`'s own non-empty rule, checked
    /// again here because a claim/route insert is not routed through
    /// `nexo-core` construction at all (it is built directly from request
    /// data by the API layer).
    EmptyClaimList,
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RepoError {}

impl From<sqlx::Error> for RepoError {
    fn from(value: sqlx::Error) -> Self {
        Self::Db(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActorRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CaseRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DigestRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProvenanceRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IngestionRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolVersionRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PolicyBundleRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NormativeSourceRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NormativeClaimRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActionRouteRowId(pub i64);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ActionEvaluationRowId(pub i64);

/// A case-graph node id: scoped to one case, assigned sequentially from 1,
/// exactly matching `nexo_core::CaseGraph`'s own id-allocation rule.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CaseNodeId(pub i64);

pub async fn connect(database_url: &str) -> Result<Pool, RepoError> {
    Ok(PgPool::connect(database_url).await?)
}

pub async fn apply_migration(pool: &Pool) -> Result<(), RepoError> {
    sqlx::raw_sql(crate::SCHEMA_MIGRATION).execute(pool).await?;
    Ok(())
}

// ---------------------------------------------------------------------
// Actors and cases
// ---------------------------------------------------------------------

pub async fn create_actor(pool: &Pool, external_identity: &str) -> Result<ActorRowId, RepoError> {
    let row = sqlx::query("INSERT INTO actors (external_identity) VALUES ($1) RETURNING id")
        .bind(external_identity)
        .fetch_one(pool)
        .await?;
    Ok(ActorRowId(row.try_get("id")?))
}

pub async fn find_actor_by_identity(
    pool: &Pool,
    external_identity: &str,
) -> Result<Option<ActorRowId>, RepoError> {
    let row = sqlx::query("SELECT id FROM actors WHERE external_identity = $1")
        .bind(external_identity)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| ActorRowId(r.get("id"))))
}

pub async fn create_case(pool: &Pool, owner: ActorRowId) -> Result<CaseRowId, RepoError> {
    let row = sqlx::query("INSERT INTO cases (owner_actor_id) VALUES ($1) RETURNING id")
        .bind(owner.0)
        .fetch_one(pool)
        .await?;
    Ok(CaseRowId(row.try_get("id")?))
}

/// Returns the case's owner, or `None` if no case with this id exists.
/// Every read/write that acts "as" an actor must check this against the
/// caller's own identity before touching case state — per
/// `docs/APPLICATION_LAYER_CONTRACT.md`'s actor/ownership model, there is
/// no ownerless and no implicitly shared case.
pub async fn case_owner(pool: &Pool, case: CaseRowId) -> Result<Option<ActorRowId>, RepoError> {
    let row = sqlx::query("SELECT owner_actor_id FROM cases WHERE id = $1")
        .bind(case.0)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| ActorRowId(r.get("owner_actor_id"))))
}

// ---------------------------------------------------------------------
// Digests, provenance, ingestion, tools — the small supporting rows every
// case-graph node payload references.
// ---------------------------------------------------------------------

pub async fn upsert_digest(
    tx: &mut Tx<'_>,
    algorithm: &str,
    hex: &str,
) -> Result<DigestRowId, RepoError> {
    let row = sqlx::query(
        "INSERT INTO digests (algorithm, hex) VALUES ($1, $2)
         ON CONFLICT (algorithm, hex) DO UPDATE SET algorithm = EXCLUDED.algorithm
         RETURNING id",
    )
    .bind(algorithm)
    .bind(hex)
    .fetch_one(&mut **tx)
    .await?;
    Ok(DigestRowId(row.try_get("id")?))
}

pub async fn insert_provenance(
    tx: &mut Tx<'_>,
    case: CaseRowId,
    channel: &str,
    actor: Option<ActorRowId>,
    recorded_at: DateTime<Utc>,
    detail: JsonValue,
) -> Result<ProvenanceRowId, RepoError> {
    let row = sqlx::query(
        "INSERT INTO provenance_records (case_id, channel, actor_id, recorded_at, detail)
         VALUES ($1, $2::acquisition_channel, $3, $4, $5) RETURNING id",
    )
    .bind(case.0)
    .bind(channel)
    .bind(actor.map(|a| a.0))
    .bind(recorded_at)
    .bind(detail)
    .fetch_one(&mut **tx)
    .await?;
    Ok(ProvenanceRowId(row.try_get("id")?))
}

pub async fn insert_ingestion_record(
    tx: &mut Tx<'_>,
    case: CaseRowId,
    received_at: DateTime<Utc>,
    declared_filename: Option<&str>,
    declared_mime: Option<&str>,
    byte_size: i64,
    status: &str,
) -> Result<IngestionRowId, RepoError> {
    let row = sqlx::query(
        "INSERT INTO ingestion_records
            (case_id, received_at, declared_filename, declared_mime, byte_size, status)
         VALUES ($1, $2, $3, $4, $5, $6::sandbox_status) RETURNING id",
    )
    .bind(case.0)
    .bind(received_at)
    .bind(declared_filename)
    .bind(declared_mime)
    .bind(byte_size)
    .bind(status)
    .fetch_one(&mut **tx)
    .await?;
    Ok(IngestionRowId(row.try_get("id")?))
}

pub async fn set_ingestion_status(
    tx: &mut Tx<'_>,
    ingestion: IngestionRowId,
    status: &str,
) -> Result<(), RepoError> {
    sqlx::query("UPDATE ingestion_records SET status = $1::sandbox_status WHERE id = $2")
        .bind(status)
        .bind(ingestion.0)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn ensure_tool_version(
    tx: &mut Tx<'_>,
    tool_name: &str,
    version: i64,
) -> Result<ToolVersionRowId, RepoError> {
    let tool_row = sqlx::query(
        "INSERT INTO tools (name) VALUES ($1)
         ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name RETURNING id",
    )
    .bind(tool_name)
    .fetch_one(&mut **tx)
    .await?;
    let tool_id: i64 = tool_row.try_get("id")?;

    let version_row = sqlx::query(
        "INSERT INTO tool_versions (tool_id, version) VALUES ($1, $2)
         ON CONFLICT (tool_id, version) DO UPDATE SET tool_id = EXCLUDED.tool_id
         RETURNING id",
    )
    .bind(tool_id)
    .bind(version)
    .fetch_one(&mut **tx)
    .await?;
    Ok(ToolVersionRowId(version_row.try_get("id")?))
}

// ---------------------------------------------------------------------
// Case graph node insertion. Every function here locks the case row first
// (transaction contract item 2), then allocates the next sequential
// node_id from the locked, consistent view of case_nodes, then inserts
// the discriminator row and its typed payload row together.
// ---------------------------------------------------------------------

async fn lock_case_and_next_node_id(tx: &mut Tx<'_>, case: CaseRowId) -> Result<i64, RepoError> {
    sqlx::query("SELECT id FROM cases WHERE id = $1 FOR UPDATE")
        .bind(case.0)
        .fetch_one(&mut **tx)
        .await?;
    let row = sqlx::query("SELECT COALESCE(MAX(node_id), 0) + 1 AS next FROM case_nodes WHERE case_id = $1")
        .bind(case.0)
        .fetch_one(&mut **tx)
        .await?;
    Ok(row.try_get("next")?)
}

pub async fn insert_artifact_node(
    tx: &mut Tx<'_>,
    case: CaseRowId,
    digest: DigestRowId,
    size_bytes: i64,
    ingestion: IngestionRowId,
    source_provenance: ProvenanceRowId,
) -> Result<CaseNodeId, RepoError> {
    let node_id = lock_case_and_next_node_id(tx, case).await?;
    sqlx::query("INSERT INTO case_nodes (case_id, node_id, kind) VALUES ($1, $2, 'artifact')")
        .bind(case.0)
        .bind(node_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO artifact_nodes
            (case_id, node_id, digest_id, size_bytes, ingestion_record_id, source_provenance_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(case.0)
    .bind(node_id)
    .bind(digest.0)
    .bind(size_bytes)
    .bind(ingestion.0)
    .bind(source_provenance.0)
    .execute(&mut **tx)
    .await?;
    Ok(CaseNodeId(node_id))
}

pub async fn insert_observation_node(
    tx: &mut Tx<'_>,
    case: CaseRowId,
    artifact_node: CaseNodeId,
    extractor_tool_version: ToolVersionRowId,
    locator: &str,
    recorded_at: DateTime<Utc>,
) -> Result<CaseNodeId, RepoError> {
    let node_id = lock_case_and_next_node_id(tx, case).await?;
    sqlx::query("INSERT INTO case_nodes (case_id, node_id, kind) VALUES ($1, $2, 'observation')")
        .bind(case.0)
        .bind(node_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO observation_nodes
            (case_id, node_id, artifact_case_id, artifact_node_id, extractor_tool_version, locator, recorded_at)
         VALUES ($1, $2, $1, $3, $4, $5, $6)",
    )
    .bind(case.0)
    .bind(node_id)
    .bind(artifact_node.0)
    .bind(extractor_tool_version.0)
    .bind(locator)
    .bind(recorded_at)
    .execute(&mut **tx)
    .await?;
    Ok(CaseNodeId(node_id))
}

pub async fn insert_user_assertion_node(
    tx: &mut Tx<'_>,
    case: CaseRowId,
    actor: ActorRowId,
    recorded_at: DateTime<Utc>,
    confirmed: bool,
) -> Result<CaseNodeId, RepoError> {
    let node_id = lock_case_and_next_node_id(tx, case).await?;
    sqlx::query("INSERT INTO case_nodes (case_id, node_id, kind) VALUES ($1, $2, 'user_assertion')")
        .bind(case.0)
        .bind(node_id)
        .execute(&mut **tx)
        .await?;
    let confirmation = if confirmed { "confirmed" } else { "unconfirmed" };
    sqlx::query(
        "INSERT INTO user_assertion_nodes (case_id, node_id, actor_id, recorded_at, confirmation)
         VALUES ($1, $2, $3, $4, $5::confirmation_state)",
    )
    .bind(case.0)
    .bind(node_id)
    .bind(actor.0)
    .bind(recorded_at)
    .bind(confirmation)
    .execute(&mut **tx)
    .await?;
    Ok(CaseNodeId(node_id))
}

/// Returns every node id currently recorded for `case`, for building a
/// `CaseProjection` from durable state.
pub async fn list_case_node_ids(pool: &Pool, case: CaseRowId) -> Result<Vec<i64>, RepoError> {
    let rows = sqlx::query("SELECT node_id FROM case_nodes WHERE case_id = $1 ORDER BY node_id")
        .bind(case.0)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|r| r.get("node_id")).collect())
}

#[derive(Debug)]
pub struct CaseNodeSummary {
    pub node_id: i64,
    pub kind: String,
    /// `Some(confirmed)` for a `user_assertion` node, `None` for every
    /// other kind.
    pub confirmed: Option<bool>,
}

pub async fn list_case_nodes_with_kind(
    pool: &Pool,
    case: CaseRowId,
) -> Result<Vec<CaseNodeSummary>, RepoError> {
    let rows = sqlx::query(
        "SELECT n.node_id, n.kind::text AS kind,
                (u.confirmation = 'confirmed') AS confirmed
         FROM case_nodes n
         LEFT JOIN user_assertion_nodes u
           ON u.case_id = n.case_id AND u.node_id = n.node_id
         WHERE n.case_id = $1
         ORDER BY n.node_id",
    )
    .bind(case.0)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CaseNodeSummary {
            node_id: r.get("node_id"),
            kind: r.get("kind"),
            confirmed: r.get("confirmed"),
        })
        .collect())
}

// ---------------------------------------------------------------------
// Policy: bundles, sources, claims, routes. Minimal write path — enough to
// seed the one AR bundle this round targets (nexo-policy-ar), not a full
// import adapter for arbitrary bundles.
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn insert_policy_bundle(
    tx: &mut Tx<'_>,
    jurisdiction: &str,
    schema_version: i16,
    policy_version: &str,
    validity_from: NaiveDate,
    validity_to: Option<NaiveDate>,
    captured_artifact_digest: DigestRowId,
    digest: DigestRowId,
    provenance: ProvenanceRowId,
) -> Result<PolicyBundleRowId, RepoError> {
    let row = sqlx::query(
        "INSERT INTO policy_bundles
            (jurisdiction, schema_version, policy_version, validity_from, validity_to,
             captured_artifact_digest_id, digest_id, provenance_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(jurisdiction)
    .bind(schema_version)
    .bind(policy_version)
    .bind(validity_from)
    .bind(validity_to)
    .bind(captured_artifact_digest.0)
    .bind(digest.0)
    .bind(provenance.0)
    .fetch_one(&mut **tx)
    .await?;
    Ok(PolicyBundleRowId(row.try_get("id")?))
}

pub async fn activate_policy_bundle(
    tx: &mut Tx<'_>,
    bundle: PolicyBundleRowId,
    activated_by: ActorRowId,
) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO policy_bundle_activations (policy_bundle_id, activated_by_actor_id)
         VALUES ($1, $2)",
    )
    .bind(bundle.0)
    .bind(activated_by.0)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_normative_source(
    tx: &mut Tx<'_>,
    authority_kind: &str,
    acquisition_channel: &str,
    issuer: &str,
    locator: &str,
    retrieved_at: DateTime<Utc>,
    captured_artifact_digest: DigestRowId,
    digest: DigestRowId,
    provenance: ProvenanceRowId,
) -> Result<NormativeSourceRowId, RepoError> {
    let row = sqlx::query(
        "INSERT INTO normative_sources
            (authority_kind, acquisition_channel, issuer, locator, retrieved_at,
             captured_artifact_digest_id, digest_id, provenance_id)
         VALUES ($1::authority_kind, $2::acquisition_channel, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(authority_kind)
    .bind(acquisition_channel)
    .bind(issuer)
    .bind(locator)
    .bind(retrieved_at)
    .bind(captured_artifact_digest.0)
    .bind(digest.0)
    .bind(provenance.0)
    .fetch_one(&mut **tx)
    .await?;
    Ok(NormativeSourceRowId(row.try_get("id")?))
}

pub async fn insert_normative_claim(
    tx: &mut Tx<'_>,
    bundle: PolicyBundleRowId,
    proposition: &str,
    jurisdiction: &str,
    validity_from: NaiveDate,
    validity_to: Option<NaiveDate>,
    sources: &[(NormativeSourceRowId, &str)],
) -> Result<NormativeClaimRowId, RepoError> {
    if sources.is_empty() {
        return Err(RepoError::EmptyClaimList);
    }
    let row = sqlx::query(
        "INSERT INTO normative_claims (policy_bundle_id, proposition, jurisdiction, validity_from, validity_to)
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(bundle.0)
    .bind(proposition)
    .bind(jurisdiction)
    .bind(validity_from)
    .bind(validity_to)
    .fetch_one(&mut **tx)
    .await?;
    let claim_id: i64 = row.try_get("id")?;
    for (ordinal, (source, role)) in sources.iter().enumerate() {
        sqlx::query(
            "INSERT INTO normative_claim_sources (claim_id, source_id, role, ordinal)
             VALUES ($1, $2, $3::support_role, $4)",
        )
        .bind(claim_id)
        .bind(source.0)
        .bind(*role)
        .bind(ordinal as i32)
        .execute(&mut **tx)
        .await?;
    }
    Ok(NormativeClaimRowId(claim_id))
}

pub async fn insert_action_route(
    tx: &mut Tx<'_>,
    bundle: PolicyBundleRowId,
    jurisdiction: &str,
    title: &str,
    claims: &[NormativeClaimRowId],
) -> Result<ActionRouteRowId, RepoError> {
    if claims.is_empty() {
        return Err(RepoError::EmptyClaimList);
    }
    let row = sqlx::query(
        "INSERT INTO action_routes (policy_bundle_id, jurisdiction, title) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(bundle.0)
    .bind(jurisdiction)
    .bind(title)
    .fetch_one(&mut **tx)
    .await?;
    let route_id: i64 = row.try_get("id")?;
    for (ordinal, claim) in claims.iter().enumerate() {
        sqlx::query(
            "INSERT INTO action_route_claims (route_id, claim_id, ordinal) VALUES ($1, $2, $3)",
        )
        .bind(route_id)
        .bind(claim.0)
        .bind(ordinal as i32)
        .execute(&mut **tx)
        .await?;
    }
    Ok(ActionRouteRowId(route_id))
}

// ---------------------------------------------------------------------
// Action evaluations and preparations
// ---------------------------------------------------------------------

/// Records an already-computed `nexo_core::ActionEvaluation` result. The
/// caller supplies the top-level normalized columns plus the canonical
/// JSON payload; this function does not itself decide anything about the
/// evaluation, matching `docs/APPLICATION_LAYER_CONTRACT.md`'s "sealed
/// JSON" design — it stores a decision `nexo-core` already made.
#[allow(clippy::too_many_arguments)]
pub async fn insert_action_evaluation(
    tx: &mut Tx<'_>,
    case: CaseRowId,
    route: ActionRouteRowId,
    bundle: PolicyBundleRowId,
    evaluator_version: &str,
    result_kind: &str,
    action_status: Option<&str>,
    non_actionable_variant: Option<&str>,
    result_schema_version: i16,
    result_payload: JsonValue,
) -> Result<ActionEvaluationRowId, RepoError> {
    let row = sqlx::query(
        "INSERT INTO action_evaluations
            (case_id, route_id, policy_bundle_id, evaluator_version, result_kind,
             action_status, non_actionable_variant, result_schema_version, result_payload)
         VALUES ($1, $2, $3, $4, $5::evaluation_result_kind, $6::action_status,
                 $7::non_actionable_variant, $8, $9)
         RETURNING id",
    )
    .bind(case.0)
    .bind(route.0)
    .bind(bundle.0)
    .bind(evaluator_version)
    .bind(result_kind)
    .bind(action_status)
    .bind(non_actionable_variant)
    .bind(result_schema_version)
    .bind(result_payload)
    .fetch_one(&mut **tx)
    .await?;
    Ok(ActionEvaluationRowId(row.try_get("id")?))
}

#[derive(Debug)]
pub struct EvaluationRow {
    pub id: i64,
    pub route_id: i64,
    pub policy_bundle_id: i64,
    pub evaluated_at: DateTime<Utc>,
    pub result_kind: String,
    pub action_status: Option<String>,
    pub non_actionable_variant: Option<String>,
    pub result_payload: JsonValue,
}

pub async fn list_evaluations_for_case(
    pool: &Pool,
    case: CaseRowId,
) -> Result<Vec<EvaluationRow>, RepoError> {
    let rows = sqlx::query(
        "SELECT id, route_id, policy_bundle_id, evaluated_at, result_kind::text,
                action_status::text, non_actionable_variant::text, result_payload
         FROM action_evaluations WHERE case_id = $1 ORDER BY evaluated_at DESC",
    )
    .bind(case.0)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| EvaluationRow {
            id: r.get("id"),
            route_id: r.get("route_id"),
            policy_bundle_id: r.get("policy_bundle_id"),
            evaluated_at: r.get("evaluated_at"),
            result_kind: r.get("result_kind"),
            action_status: r.get("action_status"),
            non_actionable_variant: r.get("non_actionable_variant"),
            result_payload: r.get("result_payload"),
        })
        .collect())
}
