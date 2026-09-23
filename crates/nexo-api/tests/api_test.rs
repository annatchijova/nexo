//! End-to-end tests: real PostgreSQL, real object store on a temp
//! directory, real sandboxed extraction through Docker, hit through the
//! actual axum `Router` (via `tower::ServiceExt::oneshot`, no network
//! socket needed). Skips gracefully, printing why, when `DATABASE_URL` is
//! unset or Docker/the extractor image is unavailable — the same pattern
//! every other Docker-dependent test in this workspace uses.
//!
//! Run via `scripts/test_api.sh`, which provisions a disposable Postgres
//! and applies the migration before invoking this suite.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Datelike, Utc};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;
use tower::ServiceExt;

use std::collections::HashMap;

use nexo_api::{router, AppState, BundleEntry};
use nexo_app::object_store::FilesystemObjectStore;
use nexo_app::repository;

fn docker_ready() -> bool {
    std::process::Command::new("docker")
        .args(["image", "inspect", "nexo-extractor-plaintext:local"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn test_state(label: &str) -> Option<AppState> {
    let _ = tracing_subscriber::fmt::try_init();
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL not set (see scripts/test_api.sh)");
            return None;
        }
    };
    if !docker_ready() {
        eprintln!(
            "skipping: nexo-extractor-plaintext:local not built (scripts/build_extractors.sh)"
        );
        return None;
    }
    let pool = repository::connect(&url).await.ok()?;

    let owner_identity = format!(
        "{label}-owner-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let owner = repository::create_actor(&pool, &owner_identity)
        .await
        .expect("create actor");

    let ley_25326_fixture = nexo_policy_ar::build();
    let (ley_25326_handle, seeded) =
        nexo_api::seed::seed_ley_25326(&pool, &ley_25326_fixture, owner)
            .await
            .expect("seed Ley 25.326 bundle");
    let (_, reseeded) = nexo_api::seed::seed_ley_25326(&pool, &ley_25326_fixture, owner)
        .await
        .expect("reseed Ley 25.326 bundle idempotently");
    assert_eq!(reseeded.policy_bundle, seeded.policy_bundle);
    assert_eq!(reseeded.action_route, seeded.action_route);
    let activation_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM policy_bundle_activations WHERE policy_bundle_id = $1",
    )
    .bind(seeded.policy_bundle.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(activation_count, 1);

    let ley_27736_fixture = nexo_policy_ar_digital_violence::build();
    let (ley_27736_handle, ley_27736_seeded) =
        nexo_api::seed::seed_ley_27736(&pool, &ley_27736_fixture, owner)
            .await
            .expect("seed Ley 27.736 bundle");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let store = FilesystemObjectStore::open(temp_dir.path()).expect("open object store");
    std::mem::forget(temp_dir); // keep the directory alive for the test's duration
    let export_root = tempfile::tempdir().expect("export temp dir");
    let export_root_path = export_root.path().to_path_buf();
    std::mem::forget(export_root);

    let mut bundles = HashMap::new();
    bundles.insert(
        ley_25326_handle.key,
        BundleEntry {
            handle: ley_25326_handle,
            seeded,
        },
    );
    bundles.insert(
        ley_27736_handle.key,
        BundleEntry {
            handle: ley_27736_handle,
            seeded: ley_27736_seeded,
        },
    );

    Some(AppState {
        pool,
        store: Arc::new(store),
        bundles: Arc::new(bundles),
        export_root: export_root_path,
    })
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn new_owner_token(pool: &repository::Pool, label: &str) -> String {
    let identity = format!(
        "{label}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    repository::create_actor(pool, &identity).await.unwrap();
    identity
}

#[tokio::test]
async fn full_flow_evidence_to_actionable_citation() {
    let Some(state) = test_state("flow").await else {
        return;
    };
    let token = new_owner_token(&state.pool, "flow").await;
    let app = router(state.clone());

    // 1. create case
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    // 2. add evidence (plain text, real sandboxed extraction)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evidence"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"filename": "note.txt", "text": "Solicito acceso a mis datos.\n"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        panic!(
            "preparation endpoint returned {status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let evidence = body_json(response).await;
    assert_eq!(evidence["observation_count"], 1);
    assert!(evidence["rejection_reason"].is_null());

    // 3. add a confirmed assertion (satisfies the fixture's identity requirement)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/assertions"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"confirmed": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 4. evaluate -> must be Actionable/Supported with real citations
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evaluate"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let evaluation = body_json(response).await;
    let evaluation_id = evaluation["evaluation_id"].as_i64().unwrap();
    assert!(repository::evaluation_receipt_exists(
        &state.pool,
        repository::ActionEvaluationRowId(evaluation_id),
    )
    .await
    .unwrap());
    let result = &evaluation["result"];
    assert_eq!(result["kind"], "actionable");
    assert_eq!(result["status"], "supported");
    assert_eq!(result["available"], true);
    let legal_support = result["legal_support"].as_array().unwrap();
    assert_eq!(legal_support.len(), 2);
    for citation in legal_support {
        assert!(citation["proposition"].as_str().unwrap().contains("Ley 25.326"));
        assert!(citation["source_locator"]
            .as_str()
            .unwrap()
            .contains("infoleg.gob.ar"));
    }
    let factual_support = result["factual_support"].as_array().unwrap();
    // the artifact, the one observation extracted from it (one non-blank
    // line), and the confirmed assertion.
    assert_eq!(factual_support.len(), 3);

    // 5. list evaluations -> the stored rendering round-trips
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/cases/{case_id}/evaluations"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let listed = body_json(response).await;
    let listed = listed.as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["result"], evaluation["result"]);

    // The application-owned bridge can mint a snapshot only for the case
    // owner; no request-supplied IDs are accepted as preparation authority.
    let owner = repository::find_actor_by_identity(&state.pool, &token)
        .await
        .unwrap()
        .unwrap();
    let default_entry = state.bundle(nexo_api::DEFAULT_BUNDLE_KEY).unwrap();
    let (projection, _, _) = nexo_api::projection::build_projection(
        &state.pool,
        repository::CaseRowId(case_id),
        &default_entry.handle,
    )
    .await
    .unwrap();
    let today = Utc::now().date_naive();
    let reference_date = nexo_core::CivilDate::try_new(
        today.year(),
        today.month() as u8,
        today.day() as u8,
    )
    .unwrap();
    let action = match nexo_core::evaluate(
        &projection,
        &default_entry.handle.bundle,
        &default_entry.handle.context,
        &default_entry.handle.route,
        reference_date,
    ) {
        nexo_core::ActionEvaluation::Actionable(action) => action,
        other => panic!("expected the same supported action, got {other:?}"),
    };
    let snapshot = nexo_api::preparation::verify_receipt_and_mint_snapshot(
        &state.pool,
        owner,
        repository::CaseRowId(case_id),
        repository::ActionEvaluationRowId(evaluation_id),
        action.clone(),
    )
    .await
    .unwrap();
    assert!(snapshot.action().action().is_available());

    let mut provenance_tx = state.pool.begin().await.unwrap();
    let preparation_provenance = repository::insert_provenance(
        &mut provenance_tx,
        repository::CaseRowId(case_id),
        "research_connector",
        Some(owner),
        chrono::Utc::now(),
        json!({"generated": true}),
    )
    .await
    .unwrap();
    provenance_tx.commit().await.unwrap();
    let preparation_id = nexo_api::preparation::persist_prepared_material(
        &state.pool,
        &state.store,
        owner,
        repository::CaseRowId(case_id),
        repository::ActionEvaluationRowId(evaluation_id),
        action.clone(),
        "draft_request",
        "test-generator-1",
        preparation_provenance,
        b"deterministic local draft",
    )
    .await
    .unwrap();
    assert!(preparation_id > 0);

    let mismatched_retry = nexo_api::preparation::persist_prepared_material(
        &state.pool,
        &state.store,
        owner,
        repository::CaseRowId(case_id),
        repository::ActionEvaluationRowId(evaluation_id),
        action.clone(),
        "draft_request",
        "test-generator-1",
        preparation_provenance,
        b"different bytes for the same preparation identity",
    )
    .await;
    assert!(matches!(
        mismatched_retry,
        Err(nexo_api::preparation::PreparationVerificationError::PreparationOutputMismatch)
    ));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/preparations"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "evaluation_id": evaluation_id,
                        "kind": "draft_request",
                        "recipient": "https://attacker.example"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/preparations"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "evaluation_id": evaluation_id,
                        "kind": "draft_request"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    if response.status() != StatusCode::OK {
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        panic!(
            "preparation endpoint returned {status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let http_preparation = body_json(response).await;
    let http_preparation_id = http_preparation["preparation_id"].as_i64().unwrap();
    assert!(http_preparation_id > 0);
    assert_eq!(http_preparation["kind"], "draft_request");
    assert_eq!(http_preparation["status"], "prepared");
    let export_dir = state
        .export_root
        .join(format!("case-{case_id}"))
        .join(format!("preparation-{http_preparation_id}"));
    let exported = nexo_api::preparation::export_preparation(
        &state.pool,
        &state.store,
        owner,
        repository::CaseRowId(case_id),
        http_preparation_id,
        &export_dir,
    )
    .await
    .unwrap();
    assert_eq!(exported.artifact_count, 1);
    let verified = nexo_verifier::verify_export(export_dir.join("manifest.json")).unwrap();
    assert_eq!(verified.manifest_digest, exported.manifest_digest);
    let exported_status: String =
        sqlx::query_scalar("SELECT status::text FROM preparations WHERE id = $1")
            .bind(http_preparation_id)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(exported_status, "exported");
    let repeated_export = nexo_api::preparation::export_preparation(
        &state.pool,
        &state.store,
        owner,
        repository::CaseRowId(case_id),
        http_preparation_id,
        &export_dir,
    )
    .await
    .unwrap();
    assert_eq!(repeated_export, exported);
    let manifest_path = export_dir.join("manifest.json");
    let original_manifest = fs::read(&manifest_path).unwrap();
    fs::write(&manifest_path, b"{}").unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/cases/{case_id}/preparations/{http_preparation_id}/export"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    fs::write(&manifest_path, original_manifest).unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/v1/cases/{case_id}/preparations/{http_preparation_id}/export"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let export_response = body_json(response).await;
    assert_eq!(export_response["status"], "exported");
    assert_eq!(
        export_response["manifest_digest"],
        exported.manifest_digest.to_string()
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/cases/{case_id}/preparations/{http_preparation_id}/export/manifest"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let manifest = body_json(response).await;
    let artifact_digest = manifest["artifacts"][0]["digest"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/cases/{case_id}/preparations/{http_preparation_id}/export/artifacts/{artifact_digest}"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.into_body().collect().await.unwrap().to_bytes().is_empty());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/preparations"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "evaluation_id": evaluation_id,
                        "kind": "draft_request"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let retried_preparation = body_json(response).await;
    assert_eq!(
        retried_preparation["preparation_id"],
        http_preparation["preparation_id"]
    );
    assert_eq!(retried_preparation["status"], "exported");

    sqlx::query("DELETE FROM evaluation_receipts WHERE action_evaluation_id = $1")
        .bind(evaluation_id)
        .execute(&state.pool)
        .await
        .unwrap();
    let receipt_removed_status: String = sqlx::query_scalar(
        "SELECT status::text FROM preparations WHERE id = $1",
    )
    .bind(preparation_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(receipt_removed_status, "invalidated");
    let http_receipt_removed_status: String = sqlx::query_scalar(
        "SELECT status::text FROM preparations WHERE id = $1",
    )
    .bind(http_preparation_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(http_receipt_removed_status, "invalidated");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/assertions"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"confirmed":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let preparation_status: String = sqlx::query_scalar(
        "SELECT status::text FROM preparations WHERE id = $1",
    )
    .bind(preparation_id)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(preparation_status, "invalidated");

    let foreign = repository::create_actor(&state.pool, "foreign-preparation-owner")
        .await
        .unwrap();
    let rejected = nexo_api::preparation::verify_receipt_and_mint_snapshot(
        &state.pool,
        foreign,
        repository::CaseRowId(case_id),
        repository::ActionEvaluationRowId(evaluation_id),
        action,
    )
    .await;
    assert!(matches!(
        rejected,
        Err(nexo_api::preparation::PreparationVerificationError::CaseNotOwned)
    ));
}

#[tokio::test]
async fn evaluation_without_confirmed_identity_is_insufficient_facts() {
    let Some(state) = test_state("insufficient").await else {
        return;
    };
    let token = new_owner_token(&state.pool, "insufficient").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    // Evidence only, no confirmed assertion.
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evidence"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"filename": null, "text": "solo evidencia, sin confirmar identidad\n"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evaluate"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let evaluation = body_json(response).await;
    let evaluation_id = evaluation["evaluation_id"].as_i64().unwrap();
    assert!(!repository::evaluation_receipt_exists(
        &state.pool,
        repository::ActionEvaluationRowId(evaluation_id),
    )
    .await
    .unwrap());
    assert_eq!(evaluation["result"]["kind"], "non_actionable");
    assert_eq!(evaluation["result"]["variant"], "insufficient_facts");
}

#[tokio::test]
async fn owner_can_read_case_graph_and_other_actor_cannot() {
    let Some(state) = test_state("case-read").await else {
        return;
    };
    let owner_token = new_owner_token(&state.pool, "case-read-owner").await;
    let other_token = new_owner_token(&state.pool, "case-read-other").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/assertions"))
                .header("Authorization", format!("Bearer {owner_token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"confirmed":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/cases/{case_id}"))
                .header("Authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let case = body_json(response).await;
    assert_eq!(case["case_id"], case_id);
    assert_eq!(case["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(case["nodes"][0]["kind"], "user_assertion");
    assert!(case["nodes"][0]["created_at"].as_str().is_some());
    assert_eq!(case["nodes"][0]["confirmed"], true);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/cases/{case_id}"))
                .header("Authorization", format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_case_cannot_be_read_by_a_different_actor() {
    let Some(state) = test_state("ownership").await else {
        return;
    };
    let owner_token = new_owner_token(&state.pool, "ownership-owner").await;
    let other_token = new_owner_token(&state.pool, "ownership-other").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {owner_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/cases/{case_id}/evaluations"))
                .header("Authorization", format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_request_without_a_valid_token_is_unauthorized() {
    let Some(state) = test_state("unauth").await else {
        return;
    };
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", "Bearer nonexistent-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn invalid_utf8_evidence_is_a_typed_rejection_not_a_crash() {
    let Some(state) = test_state("badutf8").await else {
        return;
    };
    let token = new_owner_token(&state.pool, "badutf8").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    // JSON strings can't carry invalid UTF-8 directly, so this exercises
    // the extractor's own oversized/empty-line handling instead: an empty
    // text body must still produce a clean, non-crashing result.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evidence"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"filename": null, "text": ""}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let evidence = body_json(response).await;
    assert_eq!(evidence["observation_count"], 0);
    assert!(evidence["rejection_reason"].is_null());
}

#[tokio::test]
async fn list_bundles_names_both_seeded_bundles() {
    let Some(state) = test_state("list-bundles").await else {
        return;
    };
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/bundles")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bundles = body_json(response).await;
    let keys: Vec<&str> = bundles
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["ley-25326", "ley-27736"]);
}

#[tokio::test]
async fn ley_27736_bundle_is_actionable_once_content_is_identified() {
    let Some(state) = test_state("ley27736").await else {
        return;
    };
    let token = new_owner_token(&state.pool, "ley27736").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    // The digital-violence bundle's mandatory requirement is satisfied by
    // an Artifact node (the content/URL identified), not a confirmed
    // assertion — evidence alone must be enough here.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evidence"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "filename": "captura.txt",
                        "text": "Publicaron contenido intimo mio sin consentimiento en esta URL: https://example.com/x\n"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Evaluating against the *default* bundle (Ley 25.326) with no
    // confirmed assertion must be InsufficientFacts — proves bundle
    // selection actually changes which route is evaluated, not just which
    // citations are attached to the same result.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evaluate"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let default_result = body_json(response).await;
    assert_eq!(default_result["result"]["kind"], "non_actionable");
    assert_eq!(default_result["result"]["variant"], "insufficient_facts");

    // Evaluating against ley-27736 with the same case must be Actionable.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evaluate?bundle=ley-27736"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let evaluation = body_json(response).await;
    let result = &evaluation["result"];
    assert_eq!(result["kind"], "actionable");
    assert_eq!(result["status"], "supported");
    let legal_support = result["legal_support"].as_array().unwrap();
    assert_eq!(legal_support.len(), 2);
    for citation in legal_support {
        assert!(citation["proposition"].as_str().unwrap().contains("27.736"));
    }
}

#[tokio::test]
async fn evaluate_with_unknown_bundle_key_is_not_found() {
    let Some(state) = test_state("unknown-bundle").await else {
        return;
    };
    let token = new_owner_token(&state.pool, "unknown-bundle").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evaluate?bundle=does-not-exist"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn download_report_in_markdown_and_html_names_the_real_citation() {
    let Some(state) = test_state("report").await else {
        return;
    };
    let token = new_owner_token(&state.pool, "report").await;
    let app = router(state.clone());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/cases")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let case_id = body_json(response).await["case_id"].as_i64().unwrap();

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evidence"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"filename": "nota.txt", "text": "Solicito acceso a mis datos.\n"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/assertions"))
                .header("Authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({"confirmed": true}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/cases/{case_id}/evaluate"))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let evaluation = body_json(response).await;
    let evaluation_id = evaluation["evaluation_id"].as_i64().unwrap();

    // Markdown
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/cases/{case_id}/evaluations/{evaluation_id}/report?format=md"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/markdown; charset=utf-8"
    );
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains(".md"));
    let md_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let md = String::from_utf8(md_bytes.to_vec()).unwrap();
    assert!(md.contains("Ley 25.326"));
    assert!(md.contains("Actionable"));

    // PDF — real bytes, downloadable, hash recoverable from the raw file
    // without a PDF parser (embedded as plain-text metadata).
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/cases/{case_id}/evaluations/{evaluation_id}/report?format=pdf"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "application/pdf");
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains(".pdf"));
    let pdf_bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(pdf_bytes.starts_with(b"%PDF-"));
    assert!(pdf_bytes.len() > 500);
    // The actual digest *value* (not the "result_sha256" label, which
    // lands inside a PDF string object pdfinfo/pdftotext parse correctly
    // but a raw byte search does not, per PDF's own string encoding) must
    // still be recoverable straight from the file bytes — this is what a
    // reader without a PDF library, e.g. `grep`, can actually verify.
    let result_sha256 = md
        .lines()
        .find(|line| line.contains("result_sha256 (deterministic)"))
        .and_then(|line| line.rsplit(' ').next())
        .expect("markdown chain-of-custody line names the digest");
    let pdf_text = String::from_utf8_lossy(&pdf_bytes);
    assert!(
        pdf_text.contains(result_sha256),
        "the digest value itself must be recoverable from the PDF's own bytes"
    );

    // HTML — a different actor must not be able to download it.
    let other_token = new_owner_token(&state.pool, "report-other").await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/cases/{case_id}/evaluations/{evaluation_id}/report"
                ))
                .header("Authorization", format!("Bearer {other_token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/v1/cases/{case_id}/evaluations/{evaluation_id}/report"
                ))
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    let html_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(html_bytes.to_vec()).unwrap();
    assert!(html.contains("Ley 25.326"));
    assert!(html.contains("<!doctype html>"));
}
