use std::sync::Arc;

use nexo_api::{router, AppState};
use nexo_app::object_store::FilesystemObjectStore;
use nexo_app::repository::{self, ActorRowId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (see scripts/run_api.sh)");
    let object_store_root =
        std::env::var("NEXO_OBJECT_STORE_ROOT").unwrap_or_else(|_| "./data/objects".to_string());
    let bind_addr = std::env::var("NEXO_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let bootstrap_owner_identity = std::env::var("NEXO_BOOTSTRAP_OWNER")
        .expect("NEXO_BOOTSTRAP_OWNER must be set to the single owner's bearer token/identity");

    let pool = repository::connect(&database_url).await?;
    repository::apply_migration(&pool).await?;

    let owner = match repository::find_actor_by_identity(&pool, &bootstrap_owner_identity).await? {
        Some(actor) => actor,
        None => repository::create_actor(&pool, &bootstrap_owner_identity).await?,
    };
    tracing::info!(actor_id = owner.0, "bootstrap owner ready");

    let fixture = nexo_policy_ar::build();
    let seeded = ensure_ar_bundle_seeded(&pool, &fixture, owner).await?;

    let store = FilesystemObjectStore::open(&object_store_root)?;

    let state = AppState {
        pool,
        store: Arc::new(store),
        fixture: Arc::new(fixture),
        seeded,
    };

    let app = router(state).layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(%bind_addr, "nexo-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Ensures the exact AR fixture identity is activated at startup. The seed
/// helper is idempotent and serializes concurrent process starts, so a restart
/// does not create a new policy version or invalidate unchanged preparations.
async fn ensure_ar_bundle_seeded(
    pool: &repository::Pool,
    fixture: &nexo_policy_ar::Fixture,
    owner: ActorRowId,
) -> Result<nexo_api::seed::SeededArBundle, Box<dyn std::error::Error>> {
    Ok(nexo_api::seed::seed_ar_bundle(pool, fixture, owner).await?)
}
