//! Recall. Phase 0: search over raw evidence, and exact-string lookup.
//!
//! This is the baseline every later layer must beat. See
//! `docs/architecture/08-retrieval.md` and `docs/validation/15-evaluation.md`.

pub mod exact;
pub mod search;
mod text;

pub use exact::{ExactMatch, ExactRequest, ExactResult, LiteralRole};
pub use search::{Hit, SearchRequest, SearchResult};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// What a recall returns alongside its items. `not_found` is a real answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Grounded,
    Partial,
    Conflicting,
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum RecallError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Recall over one brain, backed by PostgreSQL.
#[derive(Clone)]
pub struct Recall {
    pool: PgPool,
    tenant_id: Uuid,
}

impl Recall {
    pub fn new(pool: PgPool, tenant_id: Uuid) -> Self {
        Self { pool, tenant_id }
    }

    async fn begin(&self) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select set_config('orbit.tenant_id', $1, true)")
            .bind(self.tenant_id.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}
