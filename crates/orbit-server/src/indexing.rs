//! Background tasks that keep the indexes up to date with the ledger.

use orbit_index::{DenseIndexer, RawIndexer};
use std::time::Duration;

const RAW_BATCH: i64 = 500;
/// Feed entries per dense round; each round is one or a few provider requests.
const DENSE_BATCH: i64 = 200;

/// Full-text and literal indexing.
pub async fn raw(indexer: RawIndexer) {
    loop {
        match indexer.run_once(RAW_BATCH).await {
            Ok(n) if n > 0 => tracing::debug!(indexed = n, "raw index advanced"),
            Ok(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            Err(error) => {
                tracing::error!(%error, "raw indexing failed; retrying");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// Embedding. A failing provider only delays the dense channel; it backs off up to five minutes.
pub async fn dense(indexer: DenseIndexer) {
    let mut backoff = Duration::from_secs(10);
    loop {
        match indexer.run_once(DENSE_BATCH).await {
            Ok(n) => {
                backoff = Duration::from_secs(10);
                if n > 0 {
                    tracing::debug!(chunks = n, "dense index advanced");
                } else {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
            Err(error) => {
                tracing::error!(%error, retry_in = ?backoff, "dense indexing failed");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(300));
            }
        }
    }
}
