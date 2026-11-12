//! Background task: keep the raw index up to date with the ledger.

use orbit_index::Indexer;
use std::time::Duration;

const BATCH: i64 = 500;

pub async fn run(indexer: Indexer) {
    loop {
        match indexer.run_once(BATCH).await {
            Ok(n) if n > 0 => tracing::debug!(indexed = n, "raw index advanced"),
            Ok(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            Err(error) => {
                tracing::error!(%error, "raw indexing failed; retrying");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
