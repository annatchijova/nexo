use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

pub async fn evaluate_case(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
) -> Result<Json<EvaluateResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let (projection_result, resolver) =
        match projection::build_projection(&state.pool, case, &state.fixture).await {
            Ok((projection, resolver)) => (Some(projection), resolver),
            Err(projection::ProjectionError::NoFactualSupportYet) => {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "case has no evidence to evaluate yet",
                ));
            }
            Err(err) => return Err(internal::<projection::ProjectionError>("could not build projection")(err)),
        };
    let projection = projection_result.expect("checked above");

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
    tx.commit().await.map_err(internal("could not commit"))?;

    Ok(Json(EvaluateResponse {
        evaluation_id: evaluation.0,
        result: rendered,
    }))
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
