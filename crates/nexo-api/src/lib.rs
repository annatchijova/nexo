//! HTTP adapter. Per `docs/ARCHITECTURE.md`'s dependency rule
//! (Web -> API/application -> domain core), this crate contains no domain
//! logic of its own: it owns transactions through `nexo-app`, evidence
//! ingestion through `nexo-sandbox`/`nexo-extraction`, and deterministic
//! explanation rendering (`explain.rs`) of results `nexo-core` already
//! computed.

pub mod auth;
pub mod evidence;
pub mod explain;
pub mod handlers;
pub mod projection;
pub mod seed;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use nexo_app::object_store::FilesystemObjectStore;
use nexo_app::repository::Pool;

use seed::SeededArBundle;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub store: Arc<FilesystemObjectStore>,
    pub fixture: Arc<nexo_policy_ar::Fixture>,
    pub seeded: SeededArBundle,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/cases", post(handlers::create_case))
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
        .with_state(state)
}
