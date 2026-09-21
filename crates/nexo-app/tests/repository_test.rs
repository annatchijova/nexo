//! Integration tests for `nexo_app::repository` against a real PostgreSQL
//! instance. Requires `DATABASE_URL` (e.g. from `scripts/test_schema.sh`'s
//! disposable container, or the dev container started by hand); each test
//! skips gracefully, printing why, when it is unset or unreachable — the
//! same pattern `nexo-sandbox`'s Docker-dependent tests use.
//!
//! Every test creates its own case (and, where relevant, its own actor)
//! so tests can run concurrently against the same database without
//! colliding, and never truncates shared tables.

use chrono::Utc;
use nexo_app::repository::{self, ActorRowId};
use serde_json::json;
use sqlx::PgPool;

/// Connects only — does not apply the migration. Tests run concurrently
/// against one shared database (`scripts/test_repository.sh` applies the
/// migration exactly once before the suite starts, the same way a real
/// deployment runs a migration once and connects many times after).
async fn pool() -> Option<PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL not set (see scripts/test_repository.sh)");
            return None;
        }
    };
    match repository::connect(&url).await {
        Ok(pool) => Some(pool),
        Err(err) => {
            eprintln!("skipping: could not connect to {url}: {err:?}");
            None
        }
    }
}

async fn unique_actor(pool: &PgPool, label: &str) -> ActorRowId {
    let identity = format!(
        "{label}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    repository::create_actor(pool, &identity).await.unwrap()
}

#[tokio::test]
async fn case_nodes_are_assigned_sequential_ids_starting_at_one() {
    let Some(pool) = pool().await else { return };
    let actor = unique_actor(&pool, "seq").await;
    let case = repository::create_case(&pool, actor).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let a1 = repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), true)
        .await
        .unwrap();
    let a2 = repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), true)
        .await
        .unwrap();
    let a3 = repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), false)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(a1.0, 1);
    assert_eq!(a2.0, 2);
    assert_eq!(a3.0, 3);
}

/// The core promise of the transaction contract's item 2: concurrent
/// commands against the *same* case never assign the same node_id twice.
/// Fires many concurrent single-node-insert transactions and checks the
/// resulting ids are exactly {1..=N} with no duplicate and no gap.
#[tokio::test]
async fn concurrent_node_insertion_on_the_same_case_never_collides() {
    let Some(pool) = pool().await else { return };
    let actor = unique_actor(&pool, "concurrent").await;
    let case = repository::create_case(&pool, actor).await.unwrap();

    let mut handles = Vec::new();
    for _ in 0..16 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let mut tx = pool.begin().await.unwrap();
            let node =
                repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), true)
                    .await
                    .unwrap();
            tx.commit().await.unwrap();
            node.0
        }));
    }
    let mut ids: Vec<i64> = Vec::new();
    for handle in handles {
        ids.push(handle.await.unwrap());
    }
    ids.sort_unstable();
    assert_eq!(ids, (1..=16).collect::<Vec<_>>(), "expected no gap and no duplicate");
}

/// A rolled-back transaction must leave nothing behind: no case_nodes row
/// visible afterward, and the next successful insert must still start at 1
/// (the aborted attempt's id was never actually allocated).
#[tokio::test]
async fn aborted_transaction_leaves_no_partial_state() {
    let Some(pool) = pool().await else { return };
    let actor = unique_actor(&pool, "abort").await;
    let case = repository::create_case(&pool, actor).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), true)
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let remaining = repository::list_case_node_ids(&pool, case).await.unwrap();
    assert!(remaining.is_empty(), "rolled-back insert must not be visible");

    let mut tx = pool.begin().await.unwrap();
    let node = repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), true)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(node.0, 1, "the aborted attempt's id must not have been consumed");
}

#[tokio::test]
async fn artifact_and_observation_nodes_round_trip_with_reference_integrity() {
    let Some(pool) = pool().await else { return };
    let actor = unique_actor(&pool, "artifact").await;
    let case = repository::create_case(&pool, actor).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let digest = repository::upsert_digest(&mut tx, "sha256", &"ab".repeat(32))
        .await
        .unwrap();
    let provenance = repository::insert_provenance(
        &mut tx,
        case,
        "user_provided",
        Some(actor),
        Utc::now(),
        json!({"note": "test fixture"}),
    )
    .await
    .unwrap();
    let ingestion = repository::insert_ingestion_record(
        &mut tx,
        case,
        Utc::now(),
        Some("evidence.txt"),
        Some("text/plain"),
        1024,
        "accepted",
    )
    .await
    .unwrap();
    let artifact = repository::insert_artifact_node(&mut tx, case, digest, 1024, ingestion, provenance)
        .await
        .unwrap();

    let tool_version = repository::ensure_tool_version(&mut tx, "nexo-extractor-plaintext", 1)
        .await
        .unwrap();
    let observation = repository::insert_observation_node(
        &mut tx,
        case,
        artifact,
        tool_version,
        "line:1",
        Utc::now(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(artifact.0, 1);
    assert_eq!(observation.0, 2);

    let nodes = repository::list_case_node_ids(&pool, case).await.unwrap();
    assert_eq!(nodes, vec![1, 2]);
}

/// An `Observation` naming an artifact node id from a *different* case
/// must be rejected by the schema's own foreign key, regardless of what
/// the repository layer passes through — this proves the repository
/// didn't quietly bypass the reference-integrity constraint
/// `scripts/test_schema.sh` already checks at the SQL level, from the
/// Rust call path this time.
#[tokio::test]
async fn observation_cannot_reference_an_artifact_from_another_case() {
    let Some(pool) = pool().await else { return };
    let actor = unique_actor(&pool, "cross-case").await;
    let case_a = repository::create_case(&pool, actor).await.unwrap();
    let case_b = repository::create_case(&pool, actor).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let digest = repository::upsert_digest(&mut tx, "sha256", &"cd".repeat(32))
        .await
        .unwrap();
    let provenance = repository::insert_provenance(
        &mut tx,
        case_a,
        "user_provided",
        Some(actor),
        Utc::now(),
        json!({}),
    )
    .await
    .unwrap();
    let ingestion = repository::insert_ingestion_record(
        &mut tx, case_a, Utc::now(), None, None, 10, "accepted",
    )
    .await
    .unwrap();
    let artifact_in_case_a =
        repository::insert_artifact_node(&mut tx, case_a, digest, 10, ingestion, provenance)
            .await
            .unwrap();
    tx.commit().await.unwrap();

    let tool_version = {
        let mut tx = pool.begin().await.unwrap();
        let tv = repository::ensure_tool_version(&mut tx, "nexo-extractor-plaintext", 1)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        tv
    };

    let mut tx = pool.begin().await.unwrap();
    let result = repository::insert_observation_node(
        &mut tx,
        case_b,
        artifact_in_case_a,
        tool_version,
        "line:1",
        Utc::now(),
    )
    .await;
    assert!(result.is_err(), "cross-case artifact reference must be rejected");
}

#[tokio::test]
async fn action_evaluation_round_trips_including_sealed_json_payload() {
    let Some(pool) = pool().await else { return };
    let actor = unique_actor(&pool, "eval").await;
    let case = repository::create_case(&pool, actor).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    let digest = repository::upsert_digest(&mut tx, "sha256", &"ef".repeat(32))
        .await
        .unwrap();
    let provenance = repository::insert_provenance(
        &mut tx, case, "imported_bundle", Some(actor), Utc::now(), json!({}),
    )
    .await
    .unwrap();
    let bundle = repository::insert_policy_bundle(
        &mut tx,
        "AR",
        1,
        "ar-test-1",
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        None,
        digest,
        digest,
        provenance,
    )
    .await
    .unwrap();
    repository::activate_policy_bundle(&mut tx, bundle, actor)
        .await
        .unwrap();

    let source = repository::insert_normative_source(
        &mut tx,
        "primary_official",
        "web_fetch",
        "Test Issuer",
        "https://example.gov.ar/law",
        Utc::now(),
        digest,
        digest,
        provenance,
    )
    .await
    .unwrap();
    let claim = repository::insert_normative_claim(
        &mut tx,
        bundle,
        "test claim",
        "AR",
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        None,
        &[(source, "primary")],
    )
    .await
    .unwrap();
    let route = repository::insert_action_route(&mut tx, bundle, "AR", "test route", &[claim])
        .await
        .unwrap();

    let payload = json!({"factual_support": [{"artifact": 1}]});
    let evaluation = repository::insert_action_evaluation(
        &mut tx,
        case,
        route,
        bundle,
        "0.1.0",
        "actionable",
        Some("supported"),
        None,
        1,
        payload.clone(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let listed = repository::list_evaluations_for_case(&pool, case).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, evaluation.0);
    assert_eq!(listed[0].result_kind, "actionable");
    assert_eq!(listed[0].action_status.as_deref(), Some("supported"));
    assert_eq!(listed[0].non_actionable_variant, None);
    assert_eq!(listed[0].result_payload, payload);
}
