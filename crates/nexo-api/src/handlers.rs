use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::Json;
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path as FsPath, PathBuf};

use nexo_integrity::{hash_bytes, Sha256Digest};
use nexo_app::repository::{self, CaseRowId};
use nexo_core::evaluate;

use crate::auth::{authorize_case, AuthenticatedActor};
use crate::{explain, projection, AppState};

type ApiError = (StatusCode, &'static str);

fn internal<E: std::fmt::Debug>(context: &'static str) -> impl FnOnce(E) -> ApiError {
    move |err| {
        tracing::error!(?err, context, "internal error");
        (StatusCode::INTERNAL_SERVER_ERROR, context)
    }
}

#[derive(Serialize)]
pub struct CaseResponse {
    pub case_id: i64,
}

pub async fn create_case(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<CaseResponse>, ApiError> {
    let case = repository::create_case(&state.pool, actor)
        .await
        .map_err(internal("could not create case"))?;
    Ok(Json(CaseResponse { case_id: case.0 }))
}

#[derive(Serialize)]
pub struct CaseNodeResponse {
    pub node_id: i64,
    pub kind: String,
    pub created_at: chrono::DateTime<Utc>,
    pub confirmed: Option<bool>,
}

#[derive(Serialize)]
pub struct CaseDetailResponse {
    pub case_id: i64,
    pub nodes: Vec<CaseNodeResponse>,
}

pub async fn read_case(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
) -> Result<Json<CaseDetailResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;
    let nodes = repository::list_case_nodes_with_kind(&state.pool, case)
        .await
        .map_err(internal("could not read case"))?;

    Ok(Json(CaseDetailResponse {
        case_id,
        nodes: nodes
            .into_iter()
            .map(|node| CaseNodeResponse {
                node_id: node.node_id,
                kind: node.kind,
                created_at: node.created_at,
                confirmed: node.confirmed,
            })
            .collect(),
    }))
}

#[derive(Deserialize)]
pub struct AddEvidenceRequest {
    pub filename: Option<String>,
    pub text: String,
}

#[derive(Serialize)]
pub struct AddEvidenceResponse {
    pub artifact_node_id: i64,
    pub observation_count: usize,
    pub rejection_reason: Option<String>,
}

pub async fn add_evidence(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
    Json(request): Json<AddEvidenceRequest>,
) -> Result<Json<AddEvidenceResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let outcome = crate::evidence::ingest_plaintext_evidence(
        &state.pool,
        &state.store,
        case,
        actor,
        request.filename.as_deref(),
        request.text.as_bytes(),
    )
    .await
    .map_err(internal("could not ingest evidence"))?;

    Ok(Json(AddEvidenceResponse {
        artifact_node_id: outcome.artifact_node_id,
        observation_count: outcome.observation_count,
        rejection_reason: outcome.rejection_reason,
    }))
}

#[derive(Deserialize)]
pub struct AddAssertionRequest {
    /// Whether the actor confirms this assertion now, per
    /// `nexo_core::ConfirmationState` — matching the case-graph contract
    /// exactly: a `UserAssertionNode` carries no free-text content field
    /// (see `docs/CASE_GRAPH_CONTRACT.md`'s ontology table), only actor,
    /// time, and confirmation state. Free-text assertion content belongs
    /// on an `Observation` derived from an artifact, not invented here as
    /// an API-only field nexo-core does not model.
    pub confirmed: bool,
}

#[derive(Serialize)]
pub struct AddAssertionResponse {
    pub case_node_id: i64,
}

pub async fn add_assertion(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
    Json(request): Json<AddAssertionRequest>,
) -> Result<Json<AddAssertionResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(internal("could not start transaction"))?;
    let node = repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), request.confirmed)
        .await
        .map_err(internal("could not insert assertion"))?;
    tx.commit().await.map_err(internal("could not commit"))?;

    Ok(Json(AddAssertionResponse {
        case_node_id: node.0,
    }))
}

#[derive(Serialize)]
pub struct EvaluateResponse {
    pub evaluation_id: i64,
    pub result: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    pub evaluation_id: i64,
    pub kind: String,
}

#[derive(Serialize)]
pub struct PrepareResponse {
    pub preparation_id: i64,
    pub kind: &'static str,
    pub status: String,
}

pub async fn prepare_case(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
    Json(request): Json<PrepareRequest>,
) -> Result<Json<PrepareResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;
    if request.evaluation_id <= 0 || request.kind != "draft_request" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "only draft_request preparation is supported",
        ));
    }

    let (projection, resolver, _) = crate::projection::build_projection(
        &state.pool,
        case,
        &state.fixture,
    )
    .await
    .map_err(internal("could not build preparation projection"))?;
    let today = Utc::now().date_naive();
    let reference_date = nexo_core::CivilDate::try_new(
        today.year(),
        today.month() as u8,
        today.day() as u8,
    )
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "invalid reference date"))?;
    let evaluation = nexo_core::evaluate(
        &projection,
        &state.fixture.bundle,
        &state.fixture.context,
        &state.fixture.route,
        reference_date,
    );
    let action = match &evaluation {
        nexo_core::ActionEvaluation::Actionable(action) if action.is_available() => action.clone(),
        _ => return Err((StatusCode::CONFLICT, "evaluation is not currently preparable")),
    };
    let rendered = explain::render(&evaluation, &resolver, &state.fixture);
    let bytes = serde_json::to_vec(&rendered)
        .map_err(internal("could not render preparation material"))?;
    let preparation_id = crate::preparation::persist_prepared_material_with_provenance(
        &state.pool,
        &state.store,
        actor,
        case,
        nexo_app::repository::ActionEvaluationRowId(request.evaluation_id),
        action,
        "draft_request",
        concat!("nexo-api/", env!("CARGO_PKG_VERSION")),
        &bytes,
    )
    .await
    .map_err(|error| match error {
        crate::preparation::PreparationVerificationError::ReceiptNotFound
        | crate::preparation::PreparationVerificationError::EvaluationNotSupported
        | crate::preparation::PreparationVerificationError::ActionFingerprintMismatch
        | crate::preparation::PreparationVerificationError::InputManifestChanged
        | crate::preparation::PreparationVerificationError::PolicyBundleChanged => (
            StatusCode::CONFLICT,
            "evaluation is stale or unsupported",
        ),
        _ => internal("could not persist preparation")(error),
    })?;
    let status = repository::preparation_status(&state.pool, preparation_id)
        .await
        .map_err(internal("could not read preparation status"))?
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "preparation disappeared"))?;
    Ok(Json(PrepareResponse {
        preparation_id,
        kind: "draft_request",
        status,
    }))
}

async fn verified_export_manifest(
    state: &AppState,
    case_id: i64,
    preparation_id: i64,
) -> Result<(PathBuf, Value), ApiError> {
    let case = CaseRowId(case_id);
    let status = repository::preparation_status_for_case(&state.pool, case, preparation_id)
        .await
        .map_err(internal("could not read export status"))?;
    if status.as_deref() != Some("exported") {
        return Err((StatusCode::CONFLICT, "preparation is not exported"));
    }
    let export_dir = state
        .export_root
        .join(format!("case-{case_id}"))
        .join(format!("preparation-{preparation_id}"));
    let manifest_path = export_dir.join("manifest.json");
    let report = nexo_verifier::verify_export(&manifest_path)
        .map_err(|_| (StatusCode::CONFLICT, "export failed independent verification"))?;
    let expected_manifest = repository::preparation_export_manifest_digest_for_case(
        &state.pool,
        case,
        preparation_id,
    )
    .await
    .map_err(internal("could not read export identity"))?
    .ok_or((StatusCode::CONFLICT, "export has no durable manifest identity"))?;
    if report.manifest_digest.to_string() != expected_manifest {
        return Err((StatusCode::CONFLICT, "export manifest identity mismatch"));
    }
    let manifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(internal("could not read exported manifest"))?,
    )
    .map_err(internal("could not parse exported manifest"))?;
    Ok((export_dir, manifest))
}

pub async fn read_export_manifest(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path((case_id, preparation_id)): Path<(i64, i64)>,
) -> Result<Json<Value>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;
    let (_, manifest) = verified_export_manifest(&state, case_id, preparation_id).await?;
    Ok(Json(manifest))
}

#[derive(Serialize)]
pub struct ExportResponse {
    pub preparation_id: i64,
    pub status: &'static str,
    pub manifest_digest: String,
    pub artifact_count: usize,
}

pub async fn export_preparation(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path((case_id, preparation_id)): Path<(i64, i64)>,
) -> Result<Json<ExportResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;
    if preparation_id <= 0 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "invalid preparation id"));
    }
    let destination = state
        .export_root
        .join(format!("case-{case_id}"))
        .join(format!("preparation-{preparation_id}"));
    let result = crate::preparation::export_preparation(
        &state.pool,
        &state.store,
        actor,
        case,
        preparation_id,
        destination,
    )
    .await
    .map_err(|error| match error {
        crate::preparation::PreparationVerificationError::PreparationNotFound => {
            (StatusCode::NOT_FOUND, "preparation not found")
        }
        crate::preparation::PreparationVerificationError::PreparationNotPrepared
        | crate::preparation::PreparationVerificationError::ExistingExportInvalid(_) => (
            StatusCode::CONFLICT,
            "preparation cannot be exported",
        ),
        _ => internal("could not export preparation")(error),
    })?;
    Ok(Json(ExportResponse {
        preparation_id,
        status: "exported",
        manifest_digest: result.manifest_digest.to_string(),
        artifact_count: result.artifact_count,
    }))
}

pub async fn read_export_artifact(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path((case_id, preparation_id, digest)): Path<(i64, i64, String)>,
) -> Result<Response, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;
    let expected = Sha256Digest::from_hex(&digest)
        .map_err(|_| (StatusCode::UNPROCESSABLE_ENTITY, "invalid artifact digest"))?;
    let (export_dir, manifest) = verified_export_manifest(&state, case_id, preparation_id).await?;
    let artifact = manifest["artifacts"]
        .as_array()
        .and_then(|artifacts| {
            artifacts.iter().find(|artifact| {
                artifact["digest"].as_str() == Some(digest.as_str())
            })
        })
        .ok_or((StatusCode::NOT_FOUND, "artifact not found in export"))?;
    let relative = artifact["path"]
        .as_str()
        .ok_or((StatusCode::CONFLICT, "export artifact path is invalid"))?;
    let relative_path = FsPath::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err((StatusCode::CONFLICT, "export artifact path is invalid"));
    }
    let root = export_dir
        .canonicalize()
        .map_err(internal("could not access export directory"))?;
    let artifact_path = export_dir.join(relative_path);
    let canonical_artifact = artifact_path
        .canonicalize()
        .map_err(internal("could not access exported artifact"))?;
    if !canonical_artifact.starts_with(&root) {
        return Err((StatusCode::CONFLICT, "export artifact escaped export root"));
    }
    let bytes = fs::read(canonical_artifact)
        .map_err(internal("could not read exported artifact"))?;
    if hash_bytes(&bytes) != expected {
        return Err((StatusCode::CONFLICT, "export artifact failed verification"));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "could not build artifact response"))
}

pub async fn evaluate_case(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
) -> Result<Json<EvaluateResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let (projection, resolver, manifest) =
        match projection::build_projection(&state.pool, case, &state.fixture).await {
            Ok(result) => result,
            Err(projection::ProjectionError::NoFactualSupportYet) => {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "case has no evidence to evaluate yet",
                ));
            }
            Err(err) => return Err(internal::<projection::ProjectionError>("could not build projection")(err)),
        };

    // The core never reads an ambient clock (docs/CASE_GRAPH_CONTRACT.md,
    // "Time"); the adapter is exactly where "now" must be supplied from.
    // An earlier draft of this handler hardcoded a fixed literal date here
    // — caught before this round shipped, not after: a fixed "today" would
    // have silently become a wrong "today" the moment real time moved past
    // it, and every evaluation after that point would have been dated
    // wrong without any error to signal it.
    let today = Utc::now().date_naive();
    let reference_date = nexo_core::CivilDate::try_new(
        today.year(),
        today.month() as u8,
        today.day() as u8,
    )
    .expect("chrono's own calendar validation matches CivilDate's");
    let result = evaluate(
        &projection,
        &state.fixture.bundle,
        &state.fixture.context,
        &state.fixture.route,
        reference_date,
    );

    let rendered = explain::render(&result, &resolver, &state.fixture);
    let result_digest = result_digest(&rendered);
    let input_manifest_digest = projection::input_manifest_digest(&manifest);
    let action_digest = match &result {
        nexo_core::ActionEvaluation::Actionable(action) => {
            Some(crate::preparation::action_fingerprint(action))
        }
        nexo_core::ActionEvaluation::NonActionable(_) => None,
    };

    let (result_kind, action_status, non_actionable_variant) = classify(&result);

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(internal("could not start transaction"))?;
    let evaluation = repository::insert_action_evaluation(
        &mut tx,
        case,
        state.seeded.action_route,
        state.seeded.policy_bundle,
        env!("CARGO_PKG_VERSION"),
        result_kind,
        action_status,
        non_actionable_variant,
        1,
        rendered.clone(),
    )
    .await
    .map_err(internal("could not record evaluation"))?;
    if result_kind == "actionable" && action_status == Some("supported") {
        let action_digest = action_digest.expect("actionable results have an action digest");
        let input_manifest_digest = repository::upsert_digest(
            &mut tx,
            "sha256",
            &input_manifest_digest,
        )
        .await
        .map_err(internal("could not record input manifest digest"))?;
        let result_digest = repository::upsert_digest(&mut tx, "sha256", &result_digest)
            .await
            .map_err(internal("could not record result digest"))?;
        let action_digest = repository::upsert_digest(&mut tx, "sha256", &action_digest)
            .await
            .map_err(internal("could not record action digest"))?;
        repository::insert_evaluation_receipt(
            &mut tx,
            evaluation,
            case,
            state.seeded.action_route,
            state.seeded.policy_bundle,
            input_manifest_digest,
            result_digest,
            action_digest,
            projection::INPUT_MANIFEST_SCHEMA_VERSION,
            1,
            env!("CARGO_PKG_VERSION"),
        )
        .await
        .map_err(internal("could not record evaluation receipt"))?;
    }
    tx.commit().await.map_err(internal("could not commit"))?;

    Ok(Json(EvaluateResponse {
        evaluation_id: evaluation.0,
        result: rendered,
    }))
}

fn canonical_json(value: &Value) -> nexo_integrity::CanonicalValue {
    match value {
        Value::Null => nexo_integrity::CanonicalValue::Null,
        Value::Bool(value) => nexo_integrity::CanonicalValue::Bool(*value),
        Value::Number(value) => nexo_integrity::CanonicalValue::Text(value.to_string()),
        Value::String(value) => nexo_integrity::CanonicalValue::Text(value.clone()),
        Value::Array(values) => nexo_integrity::CanonicalValue::List(
            values.iter().map(canonical_json).collect(),
        ),
        Value::Object(values) => nexo_integrity::CanonicalValue::Map(
            values
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>(),
        ),
    }
}

fn result_digest(value: &Value) -> String {
    nexo_integrity::seal(&canonical_json(value)).to_string()
}

fn classify(
    result: &nexo_core::ActionEvaluation,
) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    use nexo_core::{ActionEvaluation, ActionStatus, NonActionable};
    match result {
        ActionEvaluation::Actionable(option) => (
            "actionable",
            Some(match option.status() {
                ActionStatus::Supported => "supported",
                ActionStatus::ConditionallySupported => "conditionally_supported",
            }),
            None,
        ),
        ActionEvaluation::NonActionable(non_actionable) => {
            let variant = match non_actionable {
                NonActionable::InsufficientFacts(_) => "insufficient_facts",
                NonActionable::Contraindicated(_) => "contraindicated",
                NonActionable::OutOfJurisdiction(_) => "out_of_jurisdiction",
                NonActionable::PolicyNotCurrent(_) => "policy_not_current",
                NonActionable::Abstain(_) => "abstain",
            };
            ("non_actionable", None, Some(variant))
        }
    }
}

#[derive(Serialize)]
pub struct EvaluationListItem {
    pub evaluation_id: i64,
    pub evaluated_at: chrono::DateTime<Utc>,
    pub result: Value,
}

pub async fn list_evaluations(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
) -> Result<Json<Vec<EvaluationListItem>>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let rows = repository::list_evaluations_for_case(&state.pool, case)
        .await
        .map_err(internal("could not list evaluations"))?;

    Ok(Json(
        rows.into_iter()
            .map(|row| EvaluationListItem {
                evaluation_id: row.id,
                evaluated_at: row.evaluated_at,
                result: row.result_payload,
            })
            .collect(),
    ))
}
