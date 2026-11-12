//! REST routes, and where MCP is mounted.

use crate::{AppState, auth::require_token, config::Config, mcp};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
};
use orbit_ledger::{IncomingEvent, SourceSpec, admit};
use orbit_recall::{ExactRequest, ExactResult, SearchRequest, SearchResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Ingest batches can carry many conversation turns.
const INGEST_BODY_LIMIT: usize = 32 * 1024 * 1024;

type ApiError = (StatusCode, String);

pub fn router(state: Arc<AppState>, config: &Config) -> Router {
    let api = Router::new()
        .route(
            "/v1/ingest/events",
            post(ingest).layer(DefaultBodyLimit::max(INGEST_BODY_LIMIT)),
        )
        .route("/v1/recall", post(recall))
        .route("/v1/recall/exact", post(exact))
        .nest_service("/mcp", mcp::service(state.clone(), config))
        .layer(middleware::from_fn_with_state(state.clone(), require_token))
        .with_state(state);
    Router::new().route("/healthz", get(|| async { "ok" })).merge(api)
}

#[derive(Deserialize)]
struct IngestRequest {
    source: SourceSpec,
    events: Vec<IncomingEvent>,
}

#[derive(Serialize)]
struct IngestResult {
    evidence_id: String,
    duplicate: bool,
}

async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(request): Json<IngestRequest>,
) -> Result<Json<Vec<IngestResult>>, ApiError> {
    // Admit everything first, so one bad event rejects the batch before anything is written.
    let admitted = request
        .events
        .into_iter()
        .map(admit)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;
    let source_id = state.ledger.upsert_source(&request.source).await.map_err(internal)?;
    let appended = state
        .ledger
        .append_batch(source_id, &admitted)
        .await
        .map_err(internal)?;
    Ok(Json(
        appended
            .into_iter()
            .map(|a| IngestResult {
                evidence_id: a.evidence_id.to_string(),
                duplicate: a.duplicate,
            })
            .collect(),
    ))
}

async fn recall(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResult>, ApiError> {
    state.recall.search(&request).await.map(Json).map_err(internal)
}

async fn exact(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExactRequest>,
) -> Result<Json<ExactResult>, ApiError> {
    state.recall.exact(&request).await.map(Json).map_err(internal)
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    tracing::error!(%error, "request failed");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
}
