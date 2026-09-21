//! Seeds the one policy bundle this round of the API targets
//! (`nexo_policy_ar::build()`) into PostgreSQL, once, at startup. Returns
//! the DB row ids the evaluation endpoint needs to satisfy the
//! `action_evaluations` table's foreign keys — evaluation itself still
//! runs against the in-memory `nexo_core::PolicyBundle`/`ActionRoute`
//! `nexo-policy-ar` built; this only makes the *fact that an evaluation
//! used this bundle version* durable and queryable.

use chrono::{DateTime, Utc};
use nexo_app::repository::{
    self, ActorRowId, ActionRouteRowId, CaseRowId, Pool, PolicyBundleRowId,
};
use sqlx::Row;

#[derive(Clone, Copy, Debug)]
pub struct SeededArBundle {
    pub policy_bundle: PolicyBundleRowId,
    pub action_route: ActionRouteRowId,
}

pub async fn seed_ar_bundle(
    pool: &Pool,
    fixture: &nexo_policy_ar::Fixture,
    seeded_by: ActorRowId,
) -> Result<SeededArBundle, repository::RepoError> {
    let mut tx = pool.begin().await?;
    // Serialize startup seeding across processes. The policy identity below
    // is a single fixture identity, so one advisory lock is sufficient and
    // avoids duplicate activations that would otherwise invalidate existing
    // preparations on every restart.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(0x4e45584f_41525f31_i64)
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
           AND bundle.schema_version = 1
           AND bundle.policy_version = 'ar-ley25326-2026.1'
           AND bundle.validity_from = DATE '2026-09-21'
           AND digest.algorithm = 'sha256'
           AND digest.hex = $1
           AND route.jurisdiction = 'AR'
           AND route.title = $2
         ORDER BY activation.activated_at DESC
         LIMIT 1",
    )
    .bind(nexo_policy_ar::CAPTURED_SOURCE_SHA256_HEX)
    .bind("Solicitar acceso, rectificación o supresión de datos personales")
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
    let bootstrap_case = sqlx::query(
        "INSERT INTO cases (owner_actor_id) VALUES ($1) RETURNING id",
    )
    .bind(seeded_by.0)
    .fetch_one(&mut *tx)
    .await
    .map(|row| CaseRowId(row.get("id")))?;

    let captured_at: DateTime<Utc> = DateTime::from_timestamp(
        1_789_948_800, // 2026-09-21T00:00:00Z — matches nexo_policy_ar::build()'s own capture instant
        0,
    )
    .expect("fixed literal is a valid instant");

    let digest = repository::upsert_digest(
        &mut tx,
        "sha256",
        nexo_policy_ar::CAPTURED_SOURCE_SHA256_HEX,
    )
    .await?;
    let provenance = repository::insert_provenance(
        &mut tx,
        bootstrap_case,
        "web_fetch",
        Some(seeded_by),
        captured_at,
        serde_json::json!({"source": "nexo-policy-ar::build()"}),
    )
    .await?;

    let policy_bundle = repository::insert_policy_bundle(
        &mut tx,
        "AR",
        1,
        "ar-ley25326-2026.1",
        chrono::NaiveDate::from_ymd_opt(2026, 9, 21).unwrap(),
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
        nexo_policy_ar::CAPTURED_SOURCE_ISSUER,
        nexo_policy_ar::CAPTURED_SOURCE_LOCATOR,
        captured_at,
        digest,
        digest,
        provenance,
    )
    .await?;

    let law_promulgated = chrono::NaiveDate::from_ymd_opt(2000, 10, 30).unwrap();
    let mut claim_rows = Vec::new();
    for (_, citation) in &fixture.citations {
        let claim = repository::insert_normative_claim(
            &mut tx,
            policy_bundle,
            citation.proposition,
            "AR",
            law_promulgated,
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
        "Solicitar acceso, rectificación o supresión de datos personales",
        &claim_rows,
    )
    .await?;
    repository::activate_policy_bundle(&mut tx, policy_bundle, seeded_by).await?;

    tx.commit().await?;

    Ok(SeededArBundle {
        policy_bundle,
        action_route,
    })
}
