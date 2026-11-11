//! The Orbit engine, Phase 0: one brain, one static token, one endpoint that
//! appends evidence. MCP, OAuth and recall are added on top of this.

use anyhow::Context;
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::AUTHORIZATION},
    routing::{get, post},
};
use orbit_ledger::{IncomingEvent, Ledger, MIGRATOR, SourceSpec, admit};
use serde::{Deserialize, Serialize};
use std::{env, sync::Arc};
use uuid::Uuid;

struct App {
    ledger: Ledger,
    token: String,
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

type ApiError = (StatusCode, String);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let tenant_id: Uuid = env::var("ORBIT_TENANT_ID").context("ORBIT_TENANT_ID is not set")?.parse().context("ORBIT_TENANT_ID is not a UUID")?;
    let token = env::var("ORBIT_TOKEN").context("ORBIT_TOKEN is not set")?;
    anyhow::ensure!(token.len() >= 32, "ORBIT_TOKEN must be at least 32 characters");
    let bind = env::var("ORBIT_BIND").unwrap_or_else(|_| "127.0.0.1:8787".to_string());

    let pool = sqlx::PgPool::connect(&database_url).await.context("cannot connect to PostgreSQL")?;
    MIGRATOR.run(&pool).await.context("migrations failed")?;

    let app = Arc::new(App { ledger: Ledger::new(pool, tenant_id), token });
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/ingest/events", post(ingest))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("cannot bind {bind}"))?;
    tracing::info!("orbit listening on {bind}");
    axum::serve(listener, router).with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); }).await?;
    Ok(())
}

async fn ingest(State(app): State<Arc<App>>, headers: HeaderMap, Json(request): Json<IngestRequest>) -> Result<Json<Vec<IngestResult>>, ApiError> {
    authorize(&headers, &app.token)?;

    // Admit everything first, so a bad event rejects the batch before anything is written.
    let admitted = request
        .events
        .into_iter()
        .map(admit)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    let source_id = app.ledger.upsert_source(&request.source).await.map_err(internal)?;
    let mut results = Vec::with_capacity(admitted.len());
    for event in &admitted {
        let appended = app.ledger.append(source_id, event).await.map_err(internal)?;
        results.push(IngestResult { evidence_id: appended.evidence_id.to_string(), duplicate: appended.duplicate });
    }
    Ok(Json(results))
}

fn authorize(headers: &HeaderMap, token: &str) -> Result<(), ApiError> {
    let presented = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    if constant_time_eq(presented.as_bytes(), token.as_bytes()) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "missing or wrong bearer token".to_string()))
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn internal(error: orbit_ledger::LedgerError) -> ApiError {
    tracing::error!(%error, "ledger failure");
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
}
