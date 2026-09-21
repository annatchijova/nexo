//! Seeds the one policy bundle this round of the API targets
//! (`nexo_policy_ar::build()`) into PostgreSQL, once, at startup. Returns
//! the DB row ids the evaluation endpoint needs to satisfy the
//! `action_evaluations` table's foreign keys — evaluation itself still
//! runs against the in-memory `nexo_core::PolicyBundle`/`ActionRoute`
//! `nexo-policy-ar` built; this only makes the *fact that an evaluation
//! used this bundle version* durable and queryable.

use chrono::{DateTime, Utc};
use nexo_app::repository::{self, ActorRowId, ActionRouteRowId, Pool, PolicyBundleRowId};

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
    // Bundle provenance is not case-scoped in the schema today
    // (provenance_records.case_id is NOT NULL); seeding uses a dedicated
    // bootstrap case as the provenance's home. See docs/API_CONTRACT.md,
    // "Known simplifications."
    let bootstrap_case = repository::create_case(pool, seeded_by).await?;

    let mut tx = pool.begin().await?;

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
