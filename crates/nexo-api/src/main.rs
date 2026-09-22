use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;

use nexo_api::{router, AppState, BundleEntry};
use nexo_app::object_store::FilesystemObjectStore;
use nexo_app::repository;
use axum::http::HeaderValue;
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (see scripts/run_api.sh)");
    let object_store_root =
        std::env::var("NEXO_OBJECT_STORE_ROOT").unwrap_or_else(|_| "./data/objects".to_string());
    let export_root = PathBuf::from(
        std::env::var("NEXO_EXPORT_ROOT").unwrap_or_else(|_| "./data/exports".to_string()),
    );
    let bind_addr = std::env::var("NEXO_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let bootstrap_owner_credential = std::env::var("NEXO_BOOTSTRAP_OWNER")
        .expect("NEXO_BOOTSTRAP_OWNER must be set to the single owner's bearer token");
    let web_origin = std::env::var("NEXO_WEB_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:5173".to_string());

    let pool = repository::connect(&database_url).await?;
    repository::apply_migration(&pool).await?;

    let owner = match repository::find_actor_by_identity(&pool, &bootstrap_owner_credential).await? {
        Some(actor) => actor,
        None => repository::create_actor(&pool, &bootstrap_owner_credential).await?,
    };
    tracing::info!(actor_id = owner.0, "bootstrap owner ready");

    let ley_25326_fixture = nexo_policy_ar::build();
    let (ley_25326_handle, ley_25326_seeded) =
        nexo_api::seed::seed_ley_25326(&pool, &ley_25326_fixture, owner).await?;
    tracing::info!(bundle = ley_25326_handle.key, "policy bundle seeded");

    let ley_27736_fixture = nexo_policy_ar_digital_violence::build();
    let (ley_27736_handle, ley_27736_seeded) =
        nexo_api::seed::seed_ley_27736(&pool, &ley_27736_fixture, owner).await?;
    tracing::info!(bundle = ley_27736_handle.key, "policy bundle seeded");

    let mut bundles = HashMap::new();
    bundles.insert(
        ley_25326_handle.key,
        BundleEntry {
            handle: ley_25326_handle,
            seeded: ley_25326_seeded,
        },
    );
    bundles.insert(
        ley_27736_handle.key,
        BundleEntry {
            handle: ley_27736_handle,
            seeded: ley_27736_seeded,
        },
    );

    let store = FilesystemObjectStore::open(&object_store_root)?;

    let state = AppState {
        pool,
        store: Arc::new(store),
        bundles: Arc::new(bundles),
        export_root,
    };

    let origin = web_origin.parse::<HeaderValue>()?;
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);
    let app = router(state)
        .layer(cors)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(%bind_addr, "nexo-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}
