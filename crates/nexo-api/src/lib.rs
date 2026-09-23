//! HTTP adapter. Per `docs/ARCHITECTURE.md`'s dependency rule
//! (Web -> API/application -> domain core), this crate contains no domain
//! logic of its own: it owns transactions through `nexo-app`, evidence
//! ingestion through `nexo-sandbox`/`nexo-extraction`, and deterministic
//! explanation rendering (`explain.rs`) of results `nexo-core` already
//! computed.

pub mod auth;
pub mod bundle;
pub mod evidence;
pub mod explain;
pub mod handlers;
pub mod preparation;
pub mod projection;
pub mod seed;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use nexo_app::object_store::FilesystemObjectStore;
use nexo_app::repository::Pool;

use bundle::PolicyBundleHandle;
use seed::SeededArBundle;

/// The bundle every existing endpoint and test used before bundle
/// selection existed. Kept as the default so `?bundle=` stays optional.
pub const DEFAULT_BUNDLE_KEY: &str = "ley-25326";

pub struct BundleEntry {
    pub handle: PolicyBundleHandle,
    pub seeded: SeededArBundle,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub store: Arc<FilesystemObjectStore>,
    pub bundles: Arc<HashMap<&'static str, BundleEntry>>,
    pub export_root: PathBuf,
}

impl AppState {
    pub fn bundle(&self, key: &str) -> Option<&BundleEntry> {
        self.bundles.get(key)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/bundles", get(handlers::list_bundles))
        .route("/v1/credentials", post(handlers::issue_credential))
        .route(
            "/v1/credentials/current",
            axum::routing::delete(handlers::revoke_current_credential),
        )
        .route("/v1/cases", post(handlers::create_case))
        .route("/v1/cases/{case_id}", get(handlers::read_case))
        .route(
            "/v1/cases/{case_id}/evidence",
            post(handlers::add_evidence),
        )
        .route(
            "/v1/cases/{case_id}/assertions",
            post(handlers::add_assertion),
        )
        .route(
            "/v1/cases/{case_id}/evaluate",
            post(handlers::evaluate_case),
        )
        .route(
            "/v1/cases/{case_id}/evaluations",
            get(handlers::list_evaluations),
        )
        .route(
            "/v1/cases/{case_id}/evaluations/{evaluation_id}/report",
            get(handlers::download_report),
        )
        .route(
            "/v1/cases/{case_id}/preparations",
            post(handlers::prepare_case),
        )
        .route(
            "/v1/cases/{case_id}/preparations/{preparation_id}/export/manifest",
            get(handlers::read_export_manifest),
        )
        .route(
            "/v1/cases/{case_id}/preparations/{preparation_id}/export",
            post(handlers::export_preparation),
        )
        .route(
            "/v1/cases/{case_id}/preparations/{preparation_id}/export/artifacts/{digest}",
            get(handlers::read_export_artifact),
        )
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}
