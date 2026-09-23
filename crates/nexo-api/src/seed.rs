//! Seeds a policy bundle into PostgreSQL, once, idempotently, at startup.
//! Returns the DB row ids the evaluation endpoint needs to satisfy the
//! `action_evaluations` table's foreign keys — evaluation itself still runs
//! against the in-memory `nexo_core::PolicyBundle`/`ActionRoute` the
//! relevant policy crate built; this only makes the *fact that an
//! evaluation used this bundle version* durable and queryable.

use chrono::{DateTime, NaiveDate, Utc};
use nexo_app::repository::{
    self, ActionRouteRowId, ActorRowId, CaseRowId, PolicyBundleRowId, Pool,
};
use sqlx::Row;

use crate::bundle::PolicyBundleHandle;

#[derive(Clone, Copy, Debug)]
pub struct SeededArBundle {
    pub policy_bundle: PolicyBundleRowId,
    pub action_route: ActionRouteRowId,
}

/// The bundle-specific literals `seed_bundle` needs, kept out of the
/// generic seeding logic below so that logic cannot silently drift between
/// bundles the way two hand-copied seed functions eventually would.
pub struct SeedParams<'a> {
    pub policy_version: &'a str,
    pub validity_from: NaiveDate,
    pub captured_at_unix_seconds: i64,
    pub captured_source_sha256_hex: &'a str,
    pub captured_source_issuer: &'a str,
    pub captured_source_locator: &'a str,
    pub claim_effective_from: NaiveDate,
    pub route_title: &'a str,
}

pub async fn seed_bundle(
    pool: &Pool,
    handle: &PolicyBundleHandle,
    params: SeedParams<'_>,
    seeded_by: ActorRowId,
) -> Result<SeededArBundle, repository::RepoError> {
    let mut tx = pool.begin().await?;
    // Serialize startup seeding across processes. One advisory lock key per
    // bundle (derived from its stable key, not user input) so two different
    // bundles can seed concurrently without contending on the same lock.
    let lock_key = advisory_lock_key(handle.key);
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(lock_key)
        .execute(&mut *tx)
        .await?;

    let existing = sqlx::query(
        "SELECT bundle.id AS bundle_id, route.id AS route_id
         FROM policy_bundles bundle
         JOIN policy_bundle_activations activation
           ON activation.policy_bundle_id = bundle.id
         JOIN action_routes route
           ON route.policy_bundle_id = bundle.id
         JOIN digests digest
           ON digest.id = bundle.digest_id
         WHERE bundle.jurisdiction = 'AR'
           AND bundle.bundle_key = $1
           AND bundle.schema_version = 1
           AND bundle.policy_version = $2
           AND bundle.validity_from = $3
           AND digest.algorithm = 'sha256'
           AND digest.hex = $4
           AND route.jurisdiction = 'AR'
           AND route.title = $5
         ORDER BY activation.activated_at DESC
         LIMIT 1",
    )
    .bind(handle.key)
    .bind(params.policy_version)
    .bind(params.validity_from)
    .bind(params.captured_source_sha256_hex)
    .bind(params.route_title)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(existing) = existing {
        tx.commit().await?;
        return Ok(SeededArBundle {
            policy_bundle: PolicyBundleRowId(existing.get("bundle_id")),
            action_route: ActionRouteRowId(existing.get("route_id")),
        });
    }

    // Bundle provenance is not case-scoped in the schema today
    // (provenance_records.case_id is NOT NULL); seeding uses a dedicated
    // bootstrap case as the provenance's home. See docs/API_CONTRACT.md,
    // "Known simplifications."
    let bootstrap_case = sqlx::query("INSERT INTO cases (owner_actor_id) VALUES ($1) RETURNING id")
        .bind(seeded_by.0)
        .fetch_one(&mut *tx)
        .await
        .map(|row| CaseRowId(row.get("id")))?;

    let captured_at: DateTime<Utc> = DateTime::from_timestamp(params.captured_at_unix_seconds, 0)
        .expect("caller-supplied capture instant is a valid Unix timestamp");

    let digest =
        repository::upsert_digest(&mut tx, "sha256", params.captured_source_sha256_hex).await?;
    let provenance = repository::insert_provenance(
        &mut tx,
        bootstrap_case,
        "web_fetch",
        Some(seeded_by),
        captured_at,
        serde_json::json!({"bundle_key": handle.key}),
    )
    .await?;

    let policy_bundle = repository::insert_policy_bundle(
        &mut tx,
        "AR",
        handle.key,
        1,
        params.policy_version,
        params.validity_from,
        None,
        digest,
        digest,
        provenance,
    )
    .await?;
    let source = repository::insert_normative_source(
        &mut tx,
        "primary_official",
        "web_fetch",
        params.captured_source_issuer,
        params.captured_source_locator,
        captured_at,
        digest,
        digest,
        provenance,
    )
    .await?;

    let mut claim_rows = Vec::new();
    for (_, citation) in &handle.citations {
        let claim = repository::insert_normative_claim(
            &mut tx,
            policy_bundle,
            citation.proposition,
            "AR",
            params.claim_effective_from,
            None,
            &[(source, "primary")],
        )
        .await?;
        claim_rows.push(claim);
    }

    let action_route = repository::insert_action_route(
        &mut tx,
        policy_bundle,
        "AR",
        params.route_title,
        &claim_rows,
    )
    .await?;
    repository::activate_policy_bundle(&mut tx, policy_bundle, seeded_by).await?;
    nexo_app::audit::append(
        &mut tx,
        seeded_by,
        Some(bootstrap_case),
        "policy_bundle.activated",
        serde_json::json!({
            "policy_bundle_id": policy_bundle.0,
            "action_route_id": action_route.0,
            "bundle_key": handle.key,
            "policy_version": params.policy_version,
        }),
        serde_json::json!([{"provenance_id": provenance.0}]),
    )
    .await?;

    tx.commit().await?;

    Ok(SeededArBundle {
        policy_bundle,
        action_route,
    })
}

/// Derives a stable, non-adversarial advisory-lock key from a bundle's own
/// (compile-time-fixed) key string — never from anything request-supplied.
fn advisory_lock_key(bundle_key: &str) -> i64 {
    let digest = nexo_integrity::hash_bytes(bundle_key.as_bytes());
    let bytes: [u8; 8] = digest.as_bytes()[..8].try_into().expect("8-byte slice");
    i64::from_be_bytes(bytes)
}

pub async fn seed_ley_25326(
    pool: &Pool,
    fixture: &nexo_policy_ar::Fixture,
    seeded_by: ActorRowId,
) -> Result<(PolicyBundleHandle, SeededArBundle), repository::RepoError> {
    let handle = PolicyBundleHandle::from_ley_25326(fixture);
    let seeded = seed_bundle(
        pool,
        &handle,
        SeedParams {
            policy_version: "ar-ley25326-2026.1",
            validity_from: NaiveDate::from_ymd_opt(2026, 9, 21).unwrap(),
            captured_at_unix_seconds: 1_789_948_800, // 2026-09-21T00:00:00Z
            captured_source_sha256_hex: nexo_policy_ar::CAPTURED_SOURCE_SHA256_HEX,
            captured_source_issuer: nexo_policy_ar::CAPTURED_SOURCE_ISSUER,
            captured_source_locator: nexo_policy_ar::CAPTURED_SOURCE_LOCATOR,
            claim_effective_from: NaiveDate::from_ymd_opt(2000, 10, 30).unwrap(),
            route_title: "Solicitar acceso, rectificación o supresión de datos personales",
        },
        seeded_by,
    )
    .await?;
    Ok((handle, seeded))
}

pub async fn seed_ley_27736(
    pool: &Pool,
    fixture: &nexo_policy_ar_digital_violence::Fixture,
    seeded_by: ActorRowId,
) -> Result<(PolicyBundleHandle, SeededArBundle), repository::RepoError> {
    let handle = PolicyBundleHandle::from_ley_27736(fixture);
    let seeded = seed_bundle(
        pool,
        &handle,
        SeedParams {
            policy_version: "ar-ley27736-2026.1",
            validity_from: NaiveDate::from_ymd_opt(2026, 9, 22).unwrap(),
            captured_at_unix_seconds: 1_790_035_200, // 2026-09-22T00:00:00Z
            captured_source_sha256_hex: nexo_policy_ar_digital_violence::CAPTURED_SOURCE_SHA256_HEX,
            captured_source_issuer: nexo_policy_ar_digital_violence::CAPTURED_SOURCE_ISSUER,
            captured_source_locator: nexo_policy_ar_digital_violence::CAPTURED_SOURCE_LOCATOR,
            claim_effective_from: NaiveDate::from_ymd_opt(2023, 10, 23).unwrap(),
            route_title:
                "Solicitar orden judicial de cese y remoción de contenido de violencia digital",
        },
        seeded_by,
    )
    .await?;
    Ok((handle, seeded))
}
