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

    let mut mismatched_payload_tx = pool.begin().await.unwrap();
    let digest = repository::upsert_digest(&mut mismatched_payload_tx, "sha256", &"aa".repeat(32))
        .await
        .unwrap();
    let provenance = repository::insert_provenance(
        &mut mismatched_payload_tx,
        case,
        "user_provided",
        Some(actor),
        Utc::now(),
        json!({}),
    )
    .await
    .unwrap();
    let ingestion = repository::insert_ingestion_record(
        &mut mismatched_payload_tx,
        case,
        Utc::now(),
        Some("wrong-kind.bin"),
        Some("application/octet-stream"),
        1,
        "accepted",
    )
    .await
    .unwrap();
    let mismatched_payload = sqlx::query(
        "INSERT INTO artifact_nodes
            (case_id, node_id, digest_id, size_bytes, ingestion_record_id, source_provenance_id)
         VALUES ($1, $2, $3, 1, $4, $5)",
    )
    .bind(case.0)
    .bind(a1.0)
    .bind(digest.0)
    .bind(ingestion.0)
    .bind(provenance.0)
    .execute(&mut *mismatched_payload_tx)
    .await;
    assert!(
        mismatched_payload.is_err(),
        "a payload table must match the case node discriminator"
    );
    mismatched_payload_tx.rollback().await.unwrap();

    let mut invalid_derived_input_tx = pool.begin().await.unwrap();
    let tool_version = repository::ensure_tool_version(&mut invalid_derived_input_tx, "graph-test", 1)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO case_nodes (case_id, node_id, kind)
         VALUES ($1, 4, 'derived_fact'), ($1, 5, 'inference')",
    )
    .bind(case.0)
    .execute(&mut *invalid_derived_input_tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO derived_fact_nodes (case_id, node_id, transformation_tool_version)
         VALUES ($1, 4, $2)",
    )
    .bind(case.0)
    .bind(tool_version.0)
    .execute(&mut *invalid_derived_input_tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inference_nodes (case_id, node_id, method_tool_version, confidence)
         VALUES ($1, 5, $2, 'low'::confidence_bound)",
    )
    .bind(case.0)
    .bind(tool_version.0)
    .execute(&mut *invalid_derived_input_tx)
    .await
    .unwrap();
    let invalid_derived_input = sqlx::query(
        "INSERT INTO derived_fact_inputs (case_id, node_id, ordinal, input_node_id)
         VALUES ($1, 4, 0, 5)",
    )
    .bind(case.0)
    .execute(&mut *invalid_derived_input_tx)
    .await;
    assert!(
        invalid_derived_input.is_err(),
        "derived facts must not consume inference nodes"
    );
    invalid_derived_input_tx.rollback().await.unwrap();

    let mut oversized_derived_tx = pool.begin().await.unwrap();
    let tool_version = repository::ensure_tool_version(
        &mut oversized_derived_tx,
        "graph-bound-test",
        1,
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO case_nodes (case_id, node_id, kind)
         VALUES ($1, 4, 'derived_fact')",
    )
    .bind(case.0)
    .execute(&mut *oversized_derived_tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO derived_fact_nodes (case_id, node_id, transformation_tool_version)
         VALUES ($1, 4, $2)",
    )
    .bind(case.0)
    .bind(tool_version.0)
    .execute(&mut *oversized_derived_tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO derived_fact_inputs (case_id, node_id, ordinal, input_node_id)
         SELECT $1, 4, ordinal, 1
         FROM generate_series(0, 64) AS ordinal",
    )
    .bind(case.0)
    .execute(&mut *oversized_derived_tx)
    .await
    .unwrap();
    let oversized_derived_commit = oversized_derived_tx.commit().await;
    assert!(
        oversized_derived_commit.is_err(),
        "a derived fact must not commit more than 64 inputs"
    );
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
    let other_case = repository::create_case(&pool, actor).await.unwrap();

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
    sqlx::query(
        "INSERT INTO action_route_requirements (route_id, description, ordinal)
         VALUES ($1, 'identity proof', 0)",
    )
    .bind(route.0)
    .execute(&mut *tx)
    .await
    .unwrap();
    repository::activate_policy_bundle(&mut tx, bundle, actor)
        .await
        .unwrap();

    let bundle_two = repository::insert_policy_bundle(
        &mut tx,
        "AR",
        1,
        "ar-test-2",
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        None,
        digest,
        digest,
        provenance,
    )
    .await
    .unwrap();
    let claim_two = repository::insert_normative_claim(
        &mut tx,
        bundle_two,
        "second test claim",
        "AR",
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        None,
        &[(source, "primary")],
    )
    .await
    .unwrap();
    let route_two = repository::insert_action_route(
        &mut tx,
        bundle_two,
        "AR",
        "second policy route",
        &[claim_two],
    )
    .await
    .unwrap();
    repository::activate_policy_bundle(&mut tx, bundle_two, actor)
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
    let input_manifest_digest = repository::upsert_digest(&mut tx, "sha256", &"11".repeat(32))
        .await
        .unwrap();
    let result_digest = repository::upsert_digest(&mut tx, "sha256", &"22".repeat(32))
        .await
        .unwrap();
    let action_digest = repository::upsert_digest(&mut tx, "sha256", &"33".repeat(32))
        .await
        .unwrap();
    let receipt = repository::insert_evaluation_receipt(
        &mut tx,
        evaluation,
        case,
        route,
        bundle,
        input_manifest_digest,
        result_digest,
        action_digest,
        1,
        1,
        "0.1.0",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut empty_claim_tx = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO normative_claims
            (policy_bundle_id, proposition, jurisdiction, validity_from)
         VALUES ($1, 'claim without source', 'AR', DATE '2026-01-01')",
    )
    .bind(bundle_two.0)
    .execute(&mut *empty_claim_tx)
    .await
    .unwrap();
    let empty_claim_commit = empty_claim_tx.commit().await;
    assert!(
        empty_claim_commit.is_err(),
        "a normative claim without a source must not commit"
    );

    let mut mismatched_source_tx = pool.begin().await.unwrap();
    let other_digest = repository::upsert_digest(
        &mut mismatched_source_tx,
        "sha256",
        &"44".repeat(32),
    )
    .await
    .unwrap();
    let mismatched_source = repository::insert_normative_source(
        &mut mismatched_source_tx,
        "primary_official",
        "web_fetch",
        "Test Issuer",
        "https://example.gov.ar/changed-law",
        Utc::now(),
        digest,
        other_digest,
        provenance,
    )
    .await;
    assert!(
        mismatched_source.is_err(),
        "a normative source digest must match its captured artifact digest"
    );
    mismatched_source_tx.rollback().await.unwrap();

    let mut mismatched_bundle_tx = pool.begin().await.unwrap();
    let other_digest = repository::upsert_digest(
        &mut mismatched_bundle_tx,
        "sha256",
        &"55".repeat(32),
    )
    .await
    .unwrap();
    let mismatched_bundle = repository::insert_policy_bundle(
        &mut mismatched_bundle_tx,
        "AR",
        1,
        "ar-test-mismatched-digest",
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        None,
        digest,
        other_digest,
        provenance,
    )
    .await;
    assert!(
        mismatched_bundle.is_err(),
        "a policy bundle digest must match its captured artifact digest"
    );
    mismatched_bundle_tx.rollback().await.unwrap();

    let mut policy_preparation_tx = pool.begin().await.unwrap();
    let policy_preparation = repository::insert_preparation(
        &mut policy_preparation_tx,
        evaluation,
        route,
        digest,
        input_manifest_digest,
        "test",
        "draft_request",
        result_digest,
        provenance,
    )
    .await
    .unwrap();
    policy_preparation_tx.commit().await.unwrap();

    let mut next_policy_tx = pool.begin().await.unwrap();
    let next_bundle = repository::insert_policy_bundle(
        &mut next_policy_tx,
        "AR",
        1,
        "ar-test-next",
        chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
        None,
        digest,
        digest,
        provenance,
    )
    .await
    .unwrap();
    repository::activate_policy_bundle(&mut next_policy_tx, next_bundle, actor)
        .await
        .unwrap();
    next_policy_tx.commit().await.unwrap();
    let policy_preparation_status: String = sqlx::query_scalar(
        "SELECT status::text FROM preparations WHERE id = $1",
    )
    .bind(policy_preparation)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(policy_preparation_status, "invalidated");

    let mut claim_jurisdiction_mismatch_tx = pool.begin().await.unwrap();
    let claim_jurisdiction_mismatch = repository::insert_normative_claim(
        &mut claim_jurisdiction_mismatch_tx,
        bundle,
        "foreign jurisdiction claim",
        "UY",
        chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        None,
        &[(source, "primary")],
    )
    .await;
    assert!(
        claim_jurisdiction_mismatch.is_err(),
        "a claim must use its policy bundle jurisdiction"
    );
    claim_jurisdiction_mismatch_tx.rollback().await.unwrap();

    let mut route_jurisdiction_mismatch_tx = pool.begin().await.unwrap();
    let route_jurisdiction_mismatch = repository::insert_action_route(
        &mut route_jurisdiction_mismatch_tx,
        bundle,
        "UY",
        "route with foreign jurisdiction",
        &[claim],
    )
    .await;
    assert!(
        route_jurisdiction_mismatch.is_err(),
        "a route must use its policy bundle jurisdiction"
    );
    route_jurisdiction_mismatch_tx.rollback().await.unwrap();

    let mut route_claim_mismatch_tx = pool.begin().await.unwrap();
    let route_claim_mismatch = repository::insert_action_route(
        &mut route_claim_mismatch_tx,
        bundle_two,
        "AR",
        "route with foreign claim",
        &[claim],
    )
    .await;
    assert!(
        route_claim_mismatch.is_err(),
        "a route must not include a claim from another policy bundle"
    );
    route_claim_mismatch_tx.rollback().await.unwrap();

    let mut route_bundle_mismatch_tx = pool.begin().await.unwrap();
    let route_bundle_mismatch = repository::insert_action_evaluation(
        &mut route_bundle_mismatch_tx,
        case,
        route_two,
        bundle,
        "0.1.0",
        "actionable",
        Some("supported"),
        None,
        1,
        json!({"factual_support": []}),
    )
    .await;
    assert!(
        route_bundle_mismatch.is_err(),
        "an evaluation must bind its route to the same policy bundle"
    );
    route_bundle_mismatch_tx.rollback().await.unwrap();

    let mut activated_bundle_update_tx = pool.begin().await.unwrap();
    let activated_bundle_update = sqlx::query(
        "UPDATE policy_bundles
         SET policy_version = 'tampered-after-activation'
         WHERE id = $1",
    )
    .bind(bundle.0)
    .execute(&mut *activated_bundle_update_tx)
    .await;
    assert!(
        activated_bundle_update.is_err(),
        "an activated policy bundle must be immutable"
    );
    activated_bundle_update_tx.rollback().await.unwrap();

    let mut activation_update_tx = pool.begin().await.unwrap();
    let activation_update = sqlx::query(
        "UPDATE policy_bundle_activations
         SET activated_by_actor_id = $1
         WHERE policy_bundle_id = $2",
    )
    .bind(actor.0)
    .bind(bundle.0)
    .execute(&mut *activation_update_tx)
    .await;
    assert!(
        activation_update.is_err(),
        "policy bundle activations must be append-only"
    );
    activation_update_tx.rollback().await.unwrap();

    let mut activation_delete_tx = pool.begin().await.unwrap();
    let activation_delete = sqlx::query(
        "DELETE FROM policy_bundle_activations WHERE policy_bundle_id = $1",
    )
    .bind(bundle.0)
    .execute(&mut *activation_delete_tx)
    .await;
    assert!(
        activation_delete.is_err(),
        "policy bundle activations must not be deletable"
    );
    activation_delete_tx.rollback().await.unwrap();

    let mut activation_duplicate_tx = pool.begin().await.unwrap();
    let activation_duplicate = sqlx::query(
        "INSERT INTO policy_bundle_activations
            (policy_bundle_id, activated_by_actor_id)
         VALUES ($1, $2)",
    )
    .bind(bundle.0)
    .bind(actor.0)
    .execute(&mut *activation_duplicate_tx)
    .await;
    assert!(
        activation_duplicate.is_err(),
        "a policy bundle must not have duplicate activation events"
    );
    activation_duplicate_tx.rollback().await.unwrap();

    let mut activated_route_update_tx = pool.begin().await.unwrap();
    let activated_route_update = sqlx::query(
        "UPDATE action_routes SET title = 'tampered route' WHERE id = $1",
    )
    .bind(route.0)
    .execute(&mut *activated_route_update_tx)
    .await;
    assert!(
        activated_route_update.is_err(),
        "a route in an activated policy bundle must be immutable"
    );
    activated_route_update_tx.rollback().await.unwrap();

    let mut activated_claim_update_tx = pool.begin().await.unwrap();
    let activated_claim_update = sqlx::query(
        "UPDATE normative_claims SET proposition = 'tampered claim' WHERE id = $1",
    )
    .bind(claim.0)
    .execute(&mut *activated_claim_update_tx)
    .await;
    assert!(
        activated_claim_update.is_err(),
        "a claim in an activated policy bundle must be immutable"
    );
    activated_claim_update_tx.rollback().await.unwrap();

    let mut activated_route_claim_delete_tx = pool.begin().await.unwrap();
    let activated_route_claim_delete = sqlx::query(
        "DELETE FROM action_route_claims
         WHERE route_id = $1 AND claim_id = $2",
    )
    .bind(route.0)
    .bind(claim.0)
    .execute(&mut *activated_route_claim_delete_tx)
    .await;
    assert!(
        activated_route_claim_delete.is_err(),
        "claims of an activated route must not be removable"
    );
    activated_route_claim_delete_tx.rollback().await.unwrap();

    let mut activated_route_requirement_insert_tx = pool.begin().await.unwrap();
    let activated_route_requirement_insert = sqlx::query(
        "INSERT INTO action_route_requirements (route_id, description, ordinal)
         VALUES ($1, 'tampered requirement', 0)",
    )
    .bind(route.0)
    .execute(&mut *activated_route_requirement_insert_tx)
    .await;
    assert!(
        activated_route_requirement_insert.is_err(),
        "requirements cannot be added to an activated route"
    );
    activated_route_requirement_insert_tx.rollback().await.unwrap();

    let mut activated_route_requirement_update_tx = pool.begin().await.unwrap();
    let activated_route_requirement_update = sqlx::query(
        "UPDATE action_route_requirements
         SET description = 'tampered requirement'
         WHERE route_id = $1",
    )
    .bind(route.0)
    .execute(&mut *activated_route_requirement_update_tx)
    .await;
    assert!(
        activated_route_requirement_update.is_err(),
        "requirements of an activated route must be immutable"
    );
    activated_route_requirement_update_tx.rollback().await.unwrap();

    let mut activated_route_requirement_delete_tx = pool.begin().await.unwrap();
    let activated_route_requirement_delete = sqlx::query(
        "DELETE FROM action_route_requirements WHERE route_id = $1",
    )
    .bind(route.0)
    .execute(&mut *activated_route_requirement_delete_tx)
    .await;
    assert!(
        activated_route_requirement_delete.is_err(),
        "requirements of an activated route cannot be deleted"
    );
    activated_route_requirement_delete_tx.rollback().await.unwrap();

    let mut activated_source_update_tx = pool.begin().await.unwrap();
    let activated_source_update = sqlx::query(
        "UPDATE normative_sources SET locator = 'https://tampered.example'
         WHERE id = $1",
    )
    .bind(source.0)
    .execute(&mut *activated_source_update_tx)
    .await;
    assert!(
        activated_source_update.is_err(),
        "a source used by an activated claim must be immutable"
    );
    activated_source_update_tx.rollback().await.unwrap();

    let mut preparation_without_receipt_tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM evaluation_receipts WHERE action_evaluation_id = $1")
        .bind(evaluation.0)
        .execute(&mut *preparation_without_receipt_tx)
        .await
        .unwrap();
    let preparation_without_receipt = sqlx::query(
        "INSERT INTO preparations
            (action_evaluation_id, action_route_id, policy_bundle_digest_id,
             input_manifest_digest_id, generator_version, kind,
             output_digest_id, output_provenance_id)
         VALUES ($1, $2, $3, $4, 'test', 'draft_request'::preparation_kind, $5, $6)",
    )
    .bind(evaluation.0)
    .bind(route.0)
    .bind(digest.0)
    .bind(input_manifest_digest.0)
    .bind(result_digest.0)
    .bind(provenance.0)
    .execute(&mut *preparation_without_receipt_tx)
    .await;
    assert!(
        preparation_without_receipt.is_err(),
        "preparation requires a verified evaluation receipt"
    );
    preparation_without_receipt_tx.rollback().await.unwrap();

    let mut wrong_algorithm_tx = pool.begin().await.unwrap();
    let wrong_algorithm_digest = repository::upsert_digest(
        &mut wrong_algorithm_tx,
        "md5",
        "not-a-sha256-digest",
    )
    .await;
    assert!(
        wrong_algorithm_digest.is_err(),
        "digest rows must use the canonical sha256 algorithm"
    );
    wrong_algorithm_tx.rollback().await.unwrap();

    let mut malformed_digest_tx = pool.begin().await.unwrap();
    let malformed_sha256 = repository::upsert_digest(
        &mut malformed_digest_tx,
        "sha256",
        "not-a-sha256-digest",
    )
    .await;
    assert!(
        malformed_sha256.is_err(),
        "sha256 digest rows must contain exactly 64 lowercase hex characters"
    );
    malformed_digest_tx.rollback().await.unwrap();

    let mut preparation_lifecycle_tx = pool.begin().await.unwrap();
    let preparation_id = repository::insert_preparation(
        &mut preparation_lifecycle_tx,
        evaluation,
        route,
        digest,
        input_manifest_digest,
        "test",
        "draft_request",
        result_digest,
        provenance,
    )
    .await
    .unwrap();
    sqlx::query(
        "UPDATE preparations SET status = 'exported'::preparation_status
         WHERE id = $1",
    )
    .bind(preparation_id)
    .execute(&mut *preparation_lifecycle_tx)
    .await
    .unwrap();
    let backwards_transition = sqlx::query(
        "UPDATE preparations SET status = 'prepared'::preparation_status
         WHERE id = $1",
    )
    .bind(preparation_id)
    .execute(&mut *preparation_lifecycle_tx)
    .await;
    assert!(
        backwards_transition.is_err(),
        "an exported preparation must not return to prepared"
    );
    let preparation_delete = sqlx::query("DELETE FROM preparations WHERE id = $1")
        .bind(preparation_id)
        .execute(&mut *preparation_lifecycle_tx)
        .await;
    assert!(
        preparation_delete.is_err(),
        "preparation evidence must not be deleted"
    );
    preparation_lifecycle_tx.rollback().await.unwrap();

    let mut foreign_provenance_tx = pool.begin().await.unwrap();
    let foreign_provenance = repository::insert_provenance(
        &mut foreign_provenance_tx,
        other_case,
        "user_provided",
        Some(actor),
        Utc::now(),
        json!({"foreign": true}),
    )
    .await
    .unwrap();
    foreign_provenance_tx.commit().await.unwrap();

    let mut preparation_foreign_provenance_tx = pool.begin().await.unwrap();
    let preparation_foreign_provenance = sqlx::query(
        "INSERT INTO preparations
            (action_evaluation_id, action_route_id, policy_bundle_digest_id,
             input_manifest_digest_id, generator_version, kind,
             output_digest_id, output_provenance_id)
         VALUES ($1, $2, $3, $4, 'test', 'draft_request'::preparation_kind, $5, $6)",
    )
    .bind(evaluation.0)
    .bind(route.0)
    .bind(digest.0)
    .bind(input_manifest_digest.0)
    .bind(result_digest.0)
    .bind(foreign_provenance.0)
    .execute(&mut *preparation_foreign_provenance_tx)
    .await;
    assert!(
        preparation_foreign_provenance.is_err(),
        "preparation output provenance must belong to the evaluation case"
    );
    preparation_foreign_provenance_tx.rollback().await.unwrap();

    assert!(receipt.0 > 0);

    let mut receipt_update_tx = pool.begin().await.unwrap();
    let receipt_update = sqlx::query(
        "UPDATE evaluation_receipts SET action_digest_id = $1 WHERE id = $2",
    )
    .bind(result_digest.0)
    .bind(receipt.0)
    .execute(&mut *receipt_update_tx)
    .await;
    assert!(
        receipt_update.is_err(),
        "receipt binding evidence must be immutable"
    );
    receipt_update_tx.rollback().await.unwrap();

    let mut evaluation_update_tx = pool.begin().await.unwrap();
    let evaluation_update = sqlx::query(
        "UPDATE action_evaluations
         SET result_payload = '{\"tampered\":true}'::jsonb
         WHERE id = $1",
    )
    .bind(evaluation.0)
    .execute(&mut *evaluation_update_tx)
    .await;
    assert!(
        evaluation_update.is_err(),
        "a receipted evaluation must be immutable"
    );
    evaluation_update_tx.rollback().await.unwrap();

    let listed = repository::list_evaluations_for_case(&pool, case).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, evaluation.0);
    assert_eq!(listed[0].result_kind, "actionable");
    assert_eq!(listed[0].action_status.as_deref(), Some("supported"));
    assert_eq!(listed[0].non_actionable_variant, None);
    assert_eq!(listed[0].result_payload, payload);

    let mut mismatched_tx = pool.begin().await.unwrap();
    let mismatched = repository::insert_evaluation_receipt(
        &mut mismatched_tx,
        evaluation,
        other_case,
        route,
        bundle,
        input_manifest_digest,
        result_digest,
        action_digest,
        1,
        1,
        "0.1.0",
    )
    .await;
    assert!(mismatched.is_err(), "receipt must bind to the evaluation case");
    mismatched_tx.rollback().await.unwrap();

    let mut non_actionable_tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM evaluation_receipts WHERE action_evaluation_id = $1")
        .bind(evaluation.0)
        .execute(&mut *non_actionable_tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE action_evaluations
         SET result_kind = 'non_actionable', action_status = NULL,
             non_actionable_variant = 'insufficient_facts'
         WHERE id = $1",
    )
    .bind(evaluation.0)
    .execute(&mut *non_actionable_tx)
    .await
    .unwrap();
    let non_actionable_receipt = repository::insert_evaluation_receipt(
        &mut non_actionable_tx,
        evaluation,
        case,
        route,
        bundle,
        input_manifest_digest,
        result_digest,
        action_digest,
        1,
        1,
        "0.1.0",
    )
    .await;
    assert!(
        non_actionable_receipt.is_err(),
        "non-actionable evaluations cannot produce preparation evidence"
    );
    non_actionable_tx.rollback().await.unwrap();
}
