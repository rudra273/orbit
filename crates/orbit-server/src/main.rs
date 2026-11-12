//! The Orbit engine: one brain per deployment.
//!
//! Phase 0 surface: ingest evidence, search it, recall exact strings, over REST and MCP.

mod auth;
mod config;
mod indexing;
mod mcp;
mod routes;

use anyhow::Context;
use orbit_index::Indexer;
use orbit_ledger::{Ledger, MIGRATOR};
use orbit_recall::Recall;
use std::sync::Arc;

/// Everything request handlers share.
pub struct AppState {
    pub ledger: Ledger,
    pub recall: Recall,
    pub token: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let config = config::Config::from_env()?;

    let pool = sqlx::PgPool::connect(&config.database_url)
        .await
        .context("cannot connect to PostgreSQL")?;
    MIGRATOR.run(&pool).await.context("migrations failed")?;

    tokio::spawn(indexing::run(Indexer::new(pool.clone(), config.tenant_id)));

    let state = Arc::new(AppState {
        ledger: Ledger::new(pool.clone(), config.tenant_id),
        recall: Recall::new(pool, config.tenant_id),
        token: config.token.clone(),
    });
    let router = routes::router(state, &config);

    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .with_context(|| format!("cannot bind {}", config.bind))?;
    tracing::info!("orbit listening on {}", config.bind);
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}
