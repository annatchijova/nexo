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

use nexo_app::repository::{self, CaseRowId};
use nexo_core::evaluate;
use nexo_integrity::{hash_bytes, Sha256Digest};

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

#[derive(Serialize)]
pub struct BundleSummary {
    pub key: &'static str,
    pub display_name: &'static str,
}

/// Unauthenticated on purpose: this names only which legal routes exist,
/// the same information `docs/API_CONTRACT.md` already documents, not case
/// data. Lets a client (or a curious `curl`) discover valid `?bundle=`
/// values without needing a token first.
pub async fn list_bundles(State(state): State<AppState>) -> Json<Vec<BundleSummary>> {
    let mut summaries: Vec<BundleSummary> = state
        .bundles
        .values()
        .map(|entry| BundleSummary {
            key: entry.handle.key,
            display_name: entry.handle.display_name,
        })
        .collect();
    summaries.sort_by_key(|summary| summary.key);
    Json(summaries)
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
    /// Plain UTF-8 text for `kind: "plain_text"` (default) and `kind:
    /// "eml"`; base64-encoded bytes for `kind: "pdf"`, since a PDF is
    /// binary and a JSON string must be valid UTF-8.
    pub text: String,
    /// Which sandboxed extractor to route this evidence through:
    /// `"plain_text"` (default, when omitted), `"eml"`, or `"pdf"`. This
    /// only selects which extractor image runs — the extractor itself
    /// never trusts this or `filename` for anything beyond routing, and
    /// rejects hostile or malformed content as a bounded failure
    /// regardless of what was claimed here
    /// (`docs/EXTRACTOR_PLAINTEXT_CONTRACT.md`,
    /// `docs/EXTRACTOR_EML_CONTRACT.md`, `docs/EXTRACTOR_PDF_CONTRACT.md`).
    #[serde(default)]
    pub kind: Option<String>,
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

    let outcome = match request.kind.as_deref().unwrap_or("plain_text") {
        "plain_text" => crate::evidence::ingest_plaintext_evidence(
            &state.pool,
            &state.store,
            case,
            actor,
            request.filename.as_deref(),
            request.text.as_bytes(),
        )
        .await
        .map_err(internal("could not ingest evidence"))?,
        "eml" => crate::evidence::ingest_eml_evidence(
            &state.pool,
            &state.store,
            case,
            actor,
            request.filename.as_deref(),
            request.text.as_bytes(),
        )
        .await
        .map_err(internal("could not ingest evidence"))?,
        "pdf" => crate::evidence::ingest_pdf_evidence(
            &state.pool,
            &state.store,
            case,
            actor,
            request.filename.as_deref(),
            &request.text,
        )
        .await
        .map_err(|error| match error {
            crate::evidence::IngestError::InvalidBase64 => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "kind: \"pdf\" requires text to be base64-encoded PDF bytes",
            ),
            other => internal("could not ingest evidence")(other),
        })?,
        _ => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "only plain_text, eml, and pdf evidence kinds are supported",
            ))
        }
    };

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
    let node =
        repository::insert_user_assertion_node(&mut tx, case, actor, Utc::now(), request.confirmed)
            .await
            .map_err(internal("could not insert assertion"))?;
    nexo_app::audit::append(
        &mut tx,
        actor,
        Some(case),
        "case.user_assertion_added",
        serde_json::json!({"node_id": node.0, "confirmed": request.confirmed}),
        serde_json::json!([]),
    )
    .await
    .map_err(internal("could not append assertion audit event"))?;
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
    let kind: &'static str = match request.kind.as_str() {
        "draft_request" => "draft_request",
        "evidence_package" => "evidence_package",
        _ => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "only draft_request and evidence_package preparation are supported",
            ))
        }
    };
    if request.evaluation_id <= 0 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "evaluation_id is required"));
    }

    // The evaluation being prepared pins which bundle produced it — look
    // that up from its receipt binding rather than assuming a fixed
    // bundle, so preparation works for whichever bundle the case was
    // actually evaluated against.
    let binding = repository::find_evaluation_receipt_binding(
        &state.pool,
        case,
        nexo_app::repository::ActionEvaluationRowId(request.evaluation_id),
    )
    .await
    .map_err(internal("could not look up evaluation receipt"))?
    .ok_or((StatusCode::CONFLICT, "evaluation is stale or unsupported"))?;
    let entry = state
        .bundles
        .values()
        .find(|entry| entry.seeded.policy_bundle.0 == binding.policy_bundle_id)
        .ok_or((
            StatusCode::CONFLICT,
            "evaluation's policy bundle is no longer seeded",
        ))?;

    let (projection, resolver, _) =
        crate::projection::build_projection(&state.pool, case, &entry.handle)
            .await
            .map_err(internal("could not build preparation projection"))?;
    let today = Utc::now().date_naive();
    let reference_date =
        nexo_core::CivilDate::try_new(today.year(), today.month() as u8, today.day() as u8)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "invalid reference date"))?;
    let evaluation = nexo_core::evaluate(
        &projection,
        &entry.handle.bundle,
        &entry.handle.context,
        &entry.handle.route,
        reference_date,
    );
    let action = match &evaluation {
        nexo_core::ActionEvaluation::Actionable(action) if action.is_available() => action.clone(),
        _ => {
            return Err((
                StatusCode::CONFLICT,
                "evaluation is not currently preparable",
            ))
        }
    };
    let bytes = match kind {
        "draft_request" => {
            let rendered = explain::render(&evaluation, &resolver, &entry.handle.citations);
            // The prepared material is the same human-readable report a
            // person downloads from GET .../evaluations/{id}/report — a raw
            // JSON dump was never a "draft request" a person could actually
            // read, print, or hand to someone. Markdown, not HTML/PDF,
            // because the prepared artifact is meant to be the plain-text
            // substance of the request, not a styled presentation of it.
            let result_sha256 = result_digest(&rendered);
            let report_input = nexo_report::ReportInput {
                case_id,
                evaluation_id: request.evaluation_id,
                bundle_key: entry.handle.key,
                bundle_display_name: entry.handle.display_name,
                generated_at: Utc::now(),
                result: &rendered,
                result_sha256: &result_sha256,
            };
            nexo_report::render_markdown(&report_input).into_bytes()
        }
        "evidence_package" => {
            let artifacts = repository::list_case_artifacts(&state.pool, case)
                .await
                .map_err(internal("could not list case artifacts"))?;
            render_evidence_package_markdown(case_id, request.evaluation_id, Utc::now(), &artifacts)
                .into_bytes()
        }
        _ => unreachable!("kind was validated above"),
    };
    let preparation_id = crate::preparation::persist_prepared_material_with_provenance(
        &state.pool,
        &state.store,
        actor,
        case,
        nexo_app::repository::ActionEvaluationRowId(request.evaluation_id),
        action,
        kind,
        concat!("nexo-api/", env!("CARGO_PKG_VERSION")),
        &bytes,
    )
    .await
    .map_err(|error| match error {
        crate::preparation::PreparationVerificationError::ReceiptNotFound
        | crate::preparation::PreparationVerificationError::EvaluationNotSupported
        | crate::preparation::PreparationVerificationError::ActionFingerprintMismatch
        | crate::preparation::PreparationVerificationError::InputManifestChanged
        | crate::preparation::PreparationVerificationError::PolicyBundleChanged => {
            (StatusCode::CONFLICT, "evaluation is stale or unsupported")
        }
        _ => internal("could not persist preparation")(error),
    })?;
    let status = repository::preparation_status(&state.pool, preparation_id)
        .await
        .map_err(internal("could not read preparation status"))?
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "preparation disappeared"))?;
    Ok(Json(PrepareResponse {
        preparation_id,
        kind,
        status,
    }))
}

/// An evidence package is a self-verifying index, not the evidence itself:
/// for each artifact in the case it names the real content digest NEXO
/// computed at ingestion time, the ingestion-declared (unverified) filename
/// and MIME type, and the locators of every observation actually extracted
/// from it — so a reader can tell "NEXO hashed this" apart from "the
/// uploader claimed this" without opening the object store.
fn render_evidence_package_markdown(
    case_id: i64,
    evaluation_id: i64,
    generated_at: chrono::DateTime<Utc>,
    artifacts: &[repository::ArtifactSummary],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "# Evidence package — case {case_id}");
    let _ = writeln!(out);
    let _ = writeln!(out, "Prepared for evaluation {evaluation_id} at {generated_at}.");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "This is an index of the artifacts in this case, not the artifact \
         bytes themselves. The digest column is NEXO's own SHA-256 hash of \
         each artifact's content, computed when it was added; the filename \
         and MIME columns are declared by whoever uploaded the artifact and \
         are not independently verified."
    );
    let _ = writeln!(out);
    if artifacts.is_empty() {
        let _ = writeln!(out, "No artifacts are recorded on this case.");
        return out;
    }
    let _ = writeln!(
        out,
        "| Artifact | SHA-256 digest | Declared filename | Declared MIME | Size (bytes) | Extractions |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|");
    for artifact in artifacts {
        let filename = artifact.declared_filename.as_deref().unwrap_or("—");
        let mime = artifact.declared_mime.as_deref().unwrap_or("—");
        let extractions = if artifact.observation_locators.is_empty() {
            "none recorded".to_string()
        } else {
            artifact.observation_locators.join(", ")
        };
        let _ = writeln!(
            out,
            "| {} | `{}` | {} | {} | {} | {} |",
            artifact.node_id,
            artifact.digest_hex,
            filename,
            mime,
            artifact.size_bytes,
            extractions
        );
    }
    out
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
    let report = nexo_verifier::verify_export(&manifest_path).map_err(|_| {
        (
            StatusCode::CONFLICT,
            "export failed independent verification",
        )
    })?;
    let expected_manifest =
        repository::preparation_export_manifest_digest_for_case(&state.pool, case, preparation_id)
            .await
            .map_err(internal("could not read export identity"))?
            .ok_or((
                StatusCode::CONFLICT,
                "export has no durable manifest identity",
            ))?;
    if report.manifest_digest.to_string() != expected_manifest {
        return Err((StatusCode::CONFLICT, "export manifest identity mismatch"));
    }
    let manifest = serde_json::from_slice(
        &fs::read(&manifest_path).map_err(internal("could not read exported manifest"))?,
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
        | crate::preparation::PreparationVerificationError::ExistingExportInvalid(_) => {
            (StatusCode::CONFLICT, "preparation cannot be exported")
        }
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
            artifacts
                .iter()
                .find(|artifact| artifact["digest"].as_str() == Some(digest.as_str()))
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
    let bytes =
        fs::read(canonical_artifact).map_err(internal("could not read exported artifact"))?;
    if hash_bytes(&bytes) != expected {
        return Err((StatusCode::CONFLICT, "export artifact failed verification"));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not build artifact response",
            )
        })
}

#[derive(Deserialize)]
pub struct EvaluateQuery {
    /// Which seeded policy bundle to evaluate against — see `GET
    /// /v1/bundles` for the available keys. Defaults to
    /// `nexo_api::DEFAULT_BUNDLE_KEY` (Ley 25.326) so every endpoint that
    /// existed before bundle selection did keeps working unchanged.
    #[serde(default)]
    pub bundle: Option<String>,
}

pub async fn evaluate_case(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(case_id): Path<i64>,
    axum::extract::Query(query): axum::extract::Query<EvaluateQuery>,
) -> Result<Json<EvaluateResponse>, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let bundle_key = query.bundle.as_deref().unwrap_or(crate::DEFAULT_BUNDLE_KEY);
    let entry = state
        .bundle(bundle_key)
        .ok_or((StatusCode::NOT_FOUND, "unknown policy bundle"))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(internal("could not start evaluation transaction"))?;
    repository::lock_case(&mut tx, case)
        .await
        .map_err(internal("could not lock case for evaluation"))?;
    let (projection, resolver, manifest) =
        match projection::build_projection_in_tx(&mut tx, case, &entry.handle).await {
            Ok(result) => result,
            Err(projection::ProjectionError::NoFactualSupportYet) => {
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "case has no evidence to evaluate yet",
                ));
            }
            Err(err) => {
                return Err(internal::<projection::ProjectionError>(
                    "could not build projection",
                )(err))
            }
        };

    // The core never reads an ambient clock (docs/CASE_GRAPH_CONTRACT.md,
    // "Time"); the adapter is exactly where "now" must be supplied from.
    // An earlier draft of this handler hardcoded a fixed literal date here
    // — caught before this round shipped, not after: a fixed "today" would
    // have silently become a wrong "today" the moment real time moved past
    // it, and every evaluation after that point would have been dated
    // wrong without any error to signal it.
    let today = Utc::now().date_naive();
    let reference_date =
        nexo_core::CivilDate::try_new(today.year(), today.month() as u8, today.day() as u8)
            .expect("chrono's own calendar validation matches CivilDate's");
    let result = evaluate(
        &projection,
        &entry.handle.bundle,
        &entry.handle.context,
        &entry.handle.route,
        reference_date,
    );

    let rendered = explain::render(&result, &resolver, &entry.handle.citations);
    let result_digest = result_digest(&rendered);
    let input_manifest_digest = projection::input_manifest_digest(&manifest);
    let action_digest = match &result {
        nexo_core::ActionEvaluation::Actionable(action) => {
            Some(crate::preparation::action_fingerprint(action))
        }
        nexo_core::ActionEvaluation::NonActionable(_) => None,
    };

    let (result_kind, action_status, non_actionable_variant) = classify(&result);

    let evaluation = repository::insert_action_evaluation(
        &mut tx,
        case,
        entry.seeded.action_route,
        entry.seeded.policy_bundle,
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
        let input_manifest_digest =
            repository::upsert_digest(&mut tx, "sha256", &input_manifest_digest)
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
            entry.seeded.action_route,
            entry.seeded.policy_bundle,
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
    nexo_app::audit::append(
        &mut tx,
        actor,
        Some(case),
        "case.evaluated",
        serde_json::json!({
            "evaluation_id": evaluation.0,
            "result_kind": result_kind,
            "action_status": action_status,
            "result_digest": result_digest,
            "input_manifest_digest": input_manifest_digest,
        }),
        serde_json::json!([]),
    )
    .await
    .map_err(internal("could not append evaluation audit event"))?;
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
        Value::Array(values) => {
            nexo_integrity::CanonicalValue::List(values.iter().map(canonical_json).collect())
        }
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

#[derive(Deserialize)]
pub struct ReportQuery {
    /// `md` (Markdown) or `html` — see `nexo-report`. Defaults to `html`.
    #[serde(default)]
    pub format: Option<String>,
}

/// Downloads a human-readable report over an already-recorded evaluation.
/// Per `nexo-report`'s own design (adapted from Anna's `zaynor` reporter):
/// this handler renders what `evaluate_case` already sealed — it never
/// re-evaluates, re-derives, or otherwise changes the decision, only
/// projects the stored `result_payload` into a downloadable document.
pub async fn download_report(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path((case_id, evaluation_id)): Path<(i64, i64)>,
    axum::extract::Query(query): axum::extract::Query<ReportQuery>,
) -> Result<Response, ApiError> {
    let case = CaseRowId(case_id);
    authorize_case(&state.pool, case, actor).await?;

    let evaluation = repository::get_evaluation(
        &state.pool,
        case,
        nexo_app::repository::ActionEvaluationRowId(evaluation_id),
    )
    .await
    .map_err(internal("could not read evaluation"))?
    .ok_or((StatusCode::NOT_FOUND, "evaluation not found"))?;

    let entry = state
        .bundles
        .values()
        .find(|entry| entry.seeded.policy_bundle.0 == evaluation.policy_bundle_id)
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "evaluation's policy bundle is no longer seeded",
        ))?;

    // The same canonical seal evaluate_case already computed and stored the
    // evaluation under — recomputed here from the exact stored payload,
    // never trusted from a caller-supplied value, so a report can never
    // claim a digest that does not match what it actually renders.
    let result_sha256 = result_digest(&evaluation.result_payload);

    let input = nexo_report::ReportInput {
        case_id,
        evaluation_id,
        bundle_key: entry.handle.key,
        bundle_display_name: entry.handle.display_name,
        generated_at: Utc::now(),
        result: &evaluation.result_payload,
        result_sha256: &result_sha256,
    };

    let format = query.format.as_deref().unwrap_or("html");
    let (content_type, body, extension): (&str, Vec<u8>, &str) = match format {
        "md" | "markdown" => (
            "text/markdown; charset=utf-8",
            nexo_report::render_markdown(&input).into_bytes(),
            "md",
        ),
        "html" => (
            "text/html; charset=utf-8",
            nexo_report::render_html(&input).into_bytes(),
            "html",
        ),
        "pdf" => {
            let bytes = nexo_report::render_pdf(&input).map_err(|error| match error {
                nexo_report::PdfError::FeatureDisabled => (
                    StatusCode::NOT_IMPLEMENTED,
                    "this deployment was built without PDF report support",
                ),
                nexo_report::PdfError::Render(_) => internal("could not render PDF")(error),
            })?;
            ("application/pdf", bytes, "pdf")
        }
        _ => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                "unknown report format (expected md, html, or pdf)",
            ))
        }
    };

    let filename = format!("nexo-case-{case_id}-evaluation-{evaluation_id}.{extension}");
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .header(
            "content-disposition",
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(body))
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not build report response",
            )
        })
}

#[derive(Serialize)]
pub struct IssueCredentialResponse {
    /// Shown exactly once — only its SHA-256 digest is ever stored
    /// (`repository::issue_actor_credential`), so this is the only
    /// opportunity to see the plaintext value.
    pub credential: String,
}

/// Issues a new bearer credential for the *same actor* already
/// authenticated on this request — never for an actor named in the
/// request body, which would make this an account-creation endpoint
/// instead of a self-service rotation one. The caller's existing
/// credential(s) remain valid: this is additive (a second device can
/// start using a new token) rather than a swap, matching
/// docs/API_CONTRACT.md's "credentials overlap during rotation" design.
pub async fn issue_credential(
    State(state): State<AppState>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<IssueCredentialResponse>, ApiError> {
    use rand::RngExt;
    let bytes: [u8; 32] = rand::rng().random();
    let credential: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    repository::issue_actor_credential(&state.pool, actor, &credential)
        .await
        .map_err(internal("could not issue credential"))?;

    Ok(Json(IssueCredentialResponse { credential }))
}

/// Revokes the exact credential presented on *this* request — never a
/// credential named in a request body or path, which would let one
/// actor's valid token revoke a credential belonging to someone else.
/// Proof of possession of the token is the only authorization this
/// endpoint requires or accepts, mirroring "log this device out."
pub async fn revoke_current_credential(
    State(state): State<AppState>,
    AuthenticatedActor(_actor): AuthenticatedActor,
    crate::auth::CurrentCredential(token): crate::auth::CurrentCredential,
) -> Result<StatusCode, ApiError> {
    let revoked = repository::revoke_actor_credential(&state.pool, &token)
        .await
        .map_err(internal("could not revoke credential"))?;
    if !revoked {
        return Err((
            StatusCode::NOT_FOUND,
            "credential not found or already revoked",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}
