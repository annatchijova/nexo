//! Evidence ingestion pipeline: object store -> sandboxed extraction ->
//! durable case-graph nodes. This is the orchestration `nexo-app` itself
//! deliberately does not own (per `docs/ARCHITECTURE.md`'s layer table,
//! extraction adapters are an "Adapters" concern, not "Application") — it
//! composes `nexo-app` (persistence, object store), `nexo-sandbox` +
//! `nexo-extraction` (the hardened worker and its typed result), all
//! inside one durable record per artifact.

use chrono::Utc;
use nexo_app::object_store::FilesystemObjectStore;
use nexo_app::repository::{self, ActorRowId, CaseRowId, Pool};
use nexo_extraction::{extract_plaintext, ExtractionAdapterError, PlaintextExtraction};
use nexo_sandbox::SandboxLimits;

#[derive(Debug)]
pub enum IngestError {
    Repo(repository::RepoError),
    Store(nexo_app::object_store::ObjectStoreError),
    Extraction(ExtractionAdapterError),
    /// The blocking task running the sandboxed extractor panicked or was
    /// cancelled — surfaced distinctly from a normal extraction failure,
    /// which is a typed `PlaintextExtraction::Failure`, not this.
    ExtractionTaskFailed,
}

impl From<repository::RepoError> for IngestError {
    fn from(value: repository::RepoError) -> Self {
        Self::Repo(value)
    }
}
impl From<nexo_app::object_store::ObjectStoreError> for IngestError {
    fn from(value: nexo_app::object_store::ObjectStoreError) -> Self {
        Self::Store(value)
    }
}
impl From<ExtractionAdapterError> for IngestError {
    fn from(value: ExtractionAdapterError) -> Self {
        Self::Extraction(value)
    }
}
impl From<sqlx::Error> for IngestError {
    fn from(value: sqlx::Error) -> Self {
        Self::Repo(repository::RepoError::from(value))
    }
}

pub struct IngestOutcome {
    pub artifact_node_id: i64,
    /// `None` when extraction produced a bounded failure rather than
    /// observations — the artifact is still durably recorded either way;
    /// this only says whether the sandboxed extractor could read it.
    pub observation_count: usize,
    pub rejection_reason: Option<String>,
}

/// Ingests one piece of plain-text evidence for `case`, owned by `actor`.
/// Writes the bytes to the object store first (so they are durable even if
/// extraction fails), then runs the sandboxed plaintext extractor, and
/// records either observation nodes or a bounded rejection reason.
pub async fn ingest_plaintext_evidence(
    pool: &Pool,
    store: &FilesystemObjectStore,
    case: CaseRowId,
    actor: ActorRowId,
    declared_filename: Option<&str>,
    bytes: &[u8],
) -> Result<IngestOutcome, IngestError> {
    // Not spawn_blocking'd like the sandboxed extraction below: this is a
    // local filesystem write, not a multi-second subprocess, so the
    // worst-case stall is small. Revisit if artifact sizes or storage
    // backends change that assumption.
    let digest = store.put(bytes)?;
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    let digest_row = repository::upsert_digest(&mut tx, "sha256", &digest.to_string()).await?;
    let provenance = repository::insert_provenance(
        &mut tx,
        case,
        "user_provided",
        Some(actor),
        now,
        serde_json::json!({}),
    )
    .await?;
    let ingestion = repository::insert_ingestion_record(
        &mut tx,
        case,
        now,
        declared_filename,
        Some("text/plain"),
        bytes.len() as i64,
        "pending",
    )
    .await?;
    let artifact = repository::insert_artifact_node(
        &mut tx,
        case,
        digest_row,
        bytes.len() as i64,
        ingestion,
        provenance,
    )
    .await?;
    nexo_app::audit::append(
        &mut tx,
        actor,
        Some(case),
        "evidence.artifact_ingested",
        serde_json::json!({
            "artifact_node_id": artifact.0,
            "digest": digest.to_string(),
            "byte_size": bytes.len(),
        }),
        serde_json::json!([{"provenance_id": provenance.0}]),
    )
    .await
    .map_err(repository::RepoError::from)?;
    tx.commit().await?;

    // extract_plaintext is synchronous (it shells out to `docker` and
    // blocks on the child process, per nexo-sandbox::run_extraction) — run
    // it on a blocking-safe thread so a slow/hostile artifact cannot stall
    // this async runtime's worker threads and every other in-flight
    // request along with it.
    let owned_bytes = bytes.to_vec();
    let extraction = tokio::task::spawn_blocking(move || {
        extract_plaintext(&owned_bytes, &SandboxLimits::conservative_default())
    })
    .await
    .map_err(|_| IngestError::ExtractionTaskFailed)??;

    match extraction {
        PlaintextExtraction::Observations(items) => {
            let mut tx = pool.begin().await?;
            repository::set_ingestion_status(&mut tx, ingestion, "accepted").await?;
            let tool_version =
                repository::ensure_tool_version(&mut tx, "nexo-extractor-plaintext", 1).await?;
            for item in &items {
                repository::insert_observation_node(
                    &mut tx,
                    case,
                    artifact,
                    tool_version,
                    &item.locator,
                    Utc::now(),
                )
                .await?;
            }
            nexo_app::audit::append(
                &mut tx,
                actor,
                Some(case),
                "evidence.extraction_accepted",
                serde_json::json!({
                    "artifact_node_id": artifact.0,
                    "observation_count": items.len(),
                }),
                serde_json::json!([]),
            )
            .await
            .map_err(repository::RepoError::from)?;
            tx.commit().await?;
            Ok(IngestOutcome {
                artifact_node_id: artifact.0,
                observation_count: items.len(),
                rejection_reason: None,
            })
        }
        PlaintextExtraction::Failure(reason) => {
            let mut tx = pool.begin().await?;
            repository::set_ingestion_status(&mut tx, ingestion, "rejected").await?;
            nexo_app::audit::append(
                &mut tx,
                actor,
                Some(case),
                "evidence.extraction_rejected",
                serde_json::json!({"artifact_node_id": artifact.0, "reason": reason}),
                serde_json::json!([]),
            )
            .await
            .map_err(repository::RepoError::from)?;
            tx.commit().await?;
            Ok(IngestOutcome {
                artifact_node_id: artifact.0,
                observation_count: 0,
                rejection_reason: Some(reason),
            })
        }
    }
}
