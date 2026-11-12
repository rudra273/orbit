//! Stage 2, persist: append to the ledger and the change feed in one transaction.

use crate::admit::Admitted;
use orbit_core::{EvidenceId, SourceId, chain::row_hash};
use serde::Deserialize;
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("the stored hash chain is corrupt")]
    CorruptChain,
}

/// Where evidence comes from: one conversation, document or transcript.
#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct SourceSpec {
    /// conversation | document | repo_file | transcript | import | note
    pub kind: String,
    /// Stable id on the sender's side: a conversation id, `path@commit`, a URL.
    pub external_ref: String,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Anything else the sender knows, such as the working directory.
    #[serde(default)]
    pub meta: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appended {
    pub evidence_id: EvidenceId,
    /// True when this idempotency key had already been stored.
    pub duplicate: bool,
}

/// The ledger of one brain.
#[derive(Clone)]
pub struct Ledger {
    pool: PgPool,
    tenant_id: Uuid,
}

impl Ledger {
    pub fn new(pool: PgPool, tenant_id: Uuid) -> Self {
        Self { pool, tenant_id }
    }

    /// Creates the source, or updates what is known about it.
    pub async fn upsert_source(&self, spec: &SourceSpec) -> Result<SourceId, LedgerError> {
        let mut tx = self.begin().await?;
        let id: Uuid = sqlx::query_scalar(
            "insert into source (tenant_id, kind, external_ref, app, model, title, meta)
             values ($1, $2, $3, $4, $5, $6, $7)
             on conflict (tenant_id, kind, external_ref) do update set
               app   = coalesce(excluded.app, source.app),
               model = coalesce(excluded.model, source.model),
               title = coalesce(excluded.title, source.title),
               meta  = source.meta || excluded.meta
             returning source_id",
        )
        .bind(self.tenant_id)
        .bind(&spec.kind)
        .bind(&spec.external_ref)
        .bind(&spec.app)
        .bind(&spec.model)
        .bind(&spec.title)
        .bind(Json(&spec.meta))
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(SourceId(id))
    }

    /// Appends one admitted event. Safe to retry: the idempotency key decides.
    pub async fn append(&self, source_id: SourceId, event: &Admitted) -> Result<Appended, LedgerError> {
        Ok(self.append_batch(source_id, std::slice::from_ref(event)).await?[0])
    }

    /// Appends events in order, in one transaction. Already-stored keys are reported as duplicates.
    pub async fn append_batch(&self, source_id: SourceId, events: &[Admitted]) -> Result<Vec<Appended>, LedgerError> {
        let mut tx = self.begin().await?;

        let last: Option<(i64, Vec<u8>)> = sqlx::query_as(
            "select chain_seq, row_hash from evidence where tenant_id = $1 order by chain_seq desc limit 1",
        )
        .bind(self.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (mut chain_seq, mut prev) = match last {
            Some((seq, hash)) => (
                seq,
                Some(<[u8; 32]>::try_from(hash).map_err(|_| LedgerError::CorruptChain)?),
            ),
            None => (0, None),
        };
        let mut feed_seq: i64 =
            sqlx::query_scalar("select coalesce(max(tenant_seq), 0) from outbox where tenant_id = $1")
                .bind(self.tenant_id)
                .fetch_one(&mut *tx)
                .await?;

        let mut results = Vec::with_capacity(events.len());
        for event in events {
            let existing: Option<Uuid> =
                sqlx::query_scalar("select evidence_id from evidence where tenant_id = $1 and idempotency_key = $2")
                    .bind(self.tenant_id)
                    .bind(&event.idempotency_key)
                    .fetch_optional(&mut *tx)
                    .await?;
            if let Some(id) = existing {
                results.push(Appended {
                    evidence_id: EvidenceId(id),
                    duplicate: true,
                });
                continue;
            }

            chain_seq += 1;
            let hash = row_hash(
                prev.as_ref(),
                &event.content_sha256,
                &event.idempotency_key,
                event.occurred_at.timestamp_micros(),
            );
            let evidence_id = EvidenceId::generate();
            sqlx::query(
                "insert into evidence
                   (tenant_id, evidence_id, source_id, seq, author_role, author_ref, occurred_at,
                    content, content_sha256, integrity, sensitivity, taint, idempotency_key,
                    chain_seq, prev_hash, row_hash)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::integrity, $11::sensitivity, $12, $13, $14, $15, $16)",
            )
            .bind(self.tenant_id)
            .bind(evidence_id.0)
            .bind(source_id.0)
            .bind(event.seq)
            .bind(event.author_role.as_str())
            .bind(&event.author_ref)
            .bind(event.occurred_at)
            .bind(&event.content)
            .bind(&event.content_sha256[..])
            .bind(event.integrity.as_str())
            .bind(event.sensitivity.as_str())
            .bind(&event.taint)
            .bind(&event.idempotency_key)
            .bind(chain_seq)
            .bind(prev.as_ref().map(|p| &p[..]))
            .bind(&hash[..])
            .execute(&mut *tx)
            .await?;

            feed_seq += 1;
            sqlx::query(
                "insert into outbox (tenant_id, tenant_seq, topic, payload) values ($1, $2, 'evidence.appended', $3)",
            )
            .bind(self.tenant_id)
            .bind(feed_seq)
            .bind(Json(
                serde_json::json!({ "evidence_id": evidence_id.0, "source_id": source_id.0 }),
            ))
            .execute(&mut *tx)
            .await?;

            prev = Some(hash);
            results.push(Appended {
                evidence_id,
                duplicate: false,
            });
        }

        tx.commit().await?;
        Ok(results)
    }

    /// Opens a transaction scoped to this brain and serialised against its other writers.
    async fn begin(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let tenant = self.tenant_id.to_string();
        sqlx::query("select set_config('orbit.tenant_id', $1, true)")
            .bind(&tenant)
            .execute(&mut *tx)
            .await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended('ledger:' || $1, 0))")
            .bind(&tenant)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}
