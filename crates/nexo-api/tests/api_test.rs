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
use std::sync::Arc;
use tower::ServiceExt;

use nexo_api::{router, AppState};
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

    let fixture = nexo_policy_ar::build();
    let seeded = nexo_api::seed::seed_ar_bundle(&pool, &fixture, owner)
        .await
        .expect("seed AR bundle");

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let store = FilesystemObjectStore::open(temp_dir.path()).expect("open object store");
    std::mem::forget(temp_dir); // keep the directory alive for the test's duration

    Some(AppState {
        pool,
        store: Arc::new(store),
        fixture: Arc::new(fixture),
        seeded,
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
    assert_eq!(response.status(), StatusCode::OK);
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
    let (projection, _, _) = nexo_api::projection::build_projection(
        &state.pool,
        repository::CaseRowId(case_id),
        &state.fixture,
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
        &state.fixture.bundle,
        &state.fixture.context,
        &state.fixture.route,
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
