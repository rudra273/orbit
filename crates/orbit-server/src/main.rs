//! The Orbit engine: one brain per deployment.
//!
//! Phase 0 surface: ingest evidence, search it by meaning and by words, recall exact strings,
//! over REST and MCP.

mod auth;
mod config;
mod indexing;
mod mcp;
mod routes;

use anyhow::Context;
use orbit_embed::{Embedder, OpenRouter};
use orbit_index::{DenseIndexer, RawIndexer};
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

    tokio::spawn(indexing::raw(RawIndexer::new(pool.clone(), config.tenant_id)));

    let embedder: Option<Arc<dyn Embedder>> = match &config.embedding {
        Some(embedding) => {
            let embedder: Arc<dyn Embedder> = Arc::new(OpenRouter::new(embedding.openrouter_key.clone())?);
            let indexer = DenseIndexer::new(
                pool.clone(),
                config.tenant_id,
                embedding.space.clone(),
                embedder.clone(),
            );
            let state = indexer
                .register()
                .await
                .context("cannot register the embedding space")?;
            tracing::info!(space = %embedding.space.id, ?state, "dense channel on");
            tokio::spawn(indexing::dense(indexer));
            Some(embedder)
        }
        None => {
            tracing::warn!("OPENROUTER_API_KEY is not set: dense channel off, search is by words only");
            None
        }
    };

    let state = Arc::new(AppState {
        ledger: Ledger::new(pool.clone(), config.tenant_id),
        recall: Recall::new(pool, config.tenant_id, embedder),
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
