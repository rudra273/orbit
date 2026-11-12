//! Dense indexing: embed conversation text into the configured embedding space.
//!
//! What is embedded: user and assistant words — not tool calls, tool output or injected client
//! context, which lexical search also leaves out by default — with at least [`MIN_CHARS`]
//! characters, cut into chunks ([`crate::chunk`]). For a hosted space, evidence labelled
//! `sensitive` is never sent (docs/architecture/11-security-privacy.md).
//!
//! A space added while another is active is `building`: it reads the change feed from the start
//! and becomes `active` once it has caught up, retiring the old one. Search never sees a
//! half-built space.

use crate::{IndexError, chunk};
use orbit_embed::{Embedder, Role, Space};
use pgvector::HalfVector;
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use uuid::Uuid;

/// Shorter texts ("ok", "yes do it") carry nothing to find by meaning. The length the
/// embedding benchmark used.
pub const MIN_CHARS: usize = 40;
/// Text beyond this is not embedded, matching the full-text index.
const EMBEDDED_CHARS: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceState {
    Building,
    Active,
    Retired,
}

impl SpaceState {
    fn parse(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "retired" => Self::Retired,
            _ => Self::Building,
        }
    }
}

/// Keeps one embedding space of one brain up to date.
#[derive(Clone)]
pub struct DenseIndexer {
    pool: PgPool,
    tenant_id: Uuid,
    space: Space,
    embedder: Arc<dyn Embedder>,
}

/// A chunk waiting for its vector.
struct Pending {
    evidence_id: Uuid,
    chunk: i32,
    start: usize,
    end: usize,
}

impl DenseIndexer {
    pub fn new(pool: PgPool, tenant_id: Uuid, space: Space, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            pool,
            tenant_id,
            space,
            embedder,
        }
    }

    fn consumer(&self) -> String {
        format!("dense:{}", self.space.id)
    }

    /// Records the space if it is new (active straight away when the brain has none) and
    /// returns its state. Refuses a space id whose settings differ from the stored ones.
    pub async fn register(&self) -> Result<SpaceState, IndexError> {
        let mut tx = self.begin().await?;
        let s = &self.space;
        sqlx::query(
            "insert into embedding_space (tenant_id, space_id, model, dims, query_prefix, doc_prefix, hosted, state)
             values ($1, $2, $3, $4, $5, $6, $7, 'building') on conflict do nothing",
        )
        .bind(self.tenant_id)
        .bind(&s.id)
        .bind(&s.model)
        .bind(s.dims as i32)
        .bind(&s.query_prefix)
        .bind(&s.doc_prefix)
        .bind(s.hosted)
        .execute(&mut *tx)
        .await?;
        let (model, dims, query_prefix, doc_prefix, hosted, state): (String, i32, String, String, bool, String) =
            sqlx::query_as(
                "select model, dims, query_prefix, doc_prefix, hosted, state from embedding_space
                 where tenant_id = $1 and space_id = $2",
            )
            .bind(self.tenant_id)
            .bind(&s.id)
            .fetch_one(&mut *tx)
            .await?;
        if (model, dims as usize, query_prefix, doc_prefix, hosted)
            != (
                s.model.clone(),
                s.dims,
                s.query_prefix.clone(),
                s.doc_prefix.clone(),
                s.hosted,
            )
        {
            return Err(IndexError::SpaceChanged(s.id.clone()));
        }
        let none_active: bool = sqlx::query_scalar(
            "select not exists (select 1 from embedding_space where tenant_id = $1 and state = 'active')",
        )
        .bind(self.tenant_id)
        .fetch_one(&mut *tx)
        .await?;
        // A brain with no active space gets this one at once; a retired space that is configured
        // again rebuilds from where it stopped and takes over when it catches up.
        let state = match SpaceState::parse(&state) {
            SpaceState::Building if none_active => {
                self.activate(&mut tx).await?;
                SpaceState::Active
            }
            SpaceState::Retired => {
                sqlx::query("update embedding_space set state = 'building' where tenant_id = $1 and space_id = $2")
                    .bind(self.tenant_id)
                    .bind(&s.id)
                    .execute(&mut *tx)
                    .await?;
                SpaceState::Building
            }
            other => other,
        };
        tx.commit().await?;
        Ok(state)
    }

    /// Embeds the evidence in up to `batch` feed entries. Returns how many chunks were embedded.
    ///
    /// The provider is called outside any transaction; the vectors and the cursor are then written
    /// together, and only if no other indexer moved the cursor meanwhile. Re-running is harmless.
    pub async fn run_once(&self, batch: i64) -> Result<usize, IndexError> {
        let consumer = self.consumer();
        let (cursor, last, at_head, texts, pending) = {
            let mut tx = self.begin().await?;
            let cursor = read_cursor(&mut tx, self.tenant_id, &consumer).await?;
            let feed: Vec<(i64, String, serde_json::Value)> = sqlx::query_as(
                "select tenant_seq, topic, payload from outbox where tenant_id = $1 and tenant_seq > $2
                 order by tenant_seq limit $3",
            )
            .bind(self.tenant_id)
            .bind(cursor)
            .bind(batch)
            .fetch_all(&mut *tx)
            .await?;
            let Some(&(last, ..)) = feed.last() else {
                tx.commit().await?;
                self.promote_if_building(cursor).await?;
                return Ok(0);
            };
            let ids: Vec<Uuid> = feed
                .iter()
                .filter(|(_, topic, _)| topic == "evidence.appended")
                .filter_map(|(_, _, p)| p["evidence_id"].as_str().and_then(|s| s.parse().ok()))
                .collect();
            let rows: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
                "select evidence_id, content from evidence
                 where tenant_id = $1 and evidence_id = any($2) and content is not null and erased_at is null
                   and author_role in ('user', 'assistant')
                   and coalesce(author_ref, '') not like 'tool_use:%'
                   and coalesce(author_ref, '') not in ('command_execution', 'client_context')
                   and not ($3 and sensitivity = 'sensitive')
                 order by evidence_id",
            )
            .bind(self.tenant_id)
            .bind(&ids)
            .bind(self.space.hosted)
            .fetch_all(&mut *tx)
            .await?;
            tx.commit().await?;

            let mut texts = Vec::new();
            let mut pending = Vec::new();
            for (evidence_id, content) in rows {
                // Admission only accepts UTF-8; anything else is left to lexical search.
                let Ok(text) = std::str::from_utf8(&content) else {
                    continue;
                };
                let text = &text[..text.char_indices().nth(EMBEDDED_CHARS).map_or(text.len(), |(i, _)| i)];
                if text.trim().chars().count() < MIN_CHARS {
                    continue;
                }
                for (n, range) in chunk::chunks(text).into_iter().enumerate() {
                    texts.push(text[range.clone()].to_string());
                    pending.push(Pending {
                        evidence_id,
                        chunk: n as i32,
                        start: range.start,
                        end: range.end,
                    });
                }
            }
            (cursor, last, feed.len() < batch as usize, texts, pending)
        };

        let vectors = if texts.is_empty() {
            Vec::new()
        } else {
            self.embedder.embed(&self.space, Role::Document, &texts).await?
        };
        if vectors.iter().any(|v| v.len() != self.space.dims) {
            let got = vectors
                .iter()
                .map(Vec::len)
                .find(|n| *n != self.space.dims)
                .unwrap_or(0);
            return Err(orbit_embed::EmbedError::Dimensions {
                space: self.space.id.clone(),
                needs: self.space.dims,
                got,
            }
            .into());
        }

        let mut tx = self.begin().await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended('dense-index:' || $1 || ':' || $2, 0))")
            .bind(self.tenant_id.to_string())
            .bind(&self.space.id)
            .execute(&mut *tx)
            .await?;
        if read_cursor(&mut tx, self.tenant_id, &consumer).await? != cursor {
            return Ok(0);
        }
        for (p, v) in pending.iter().zip(&vectors) {
            sqlx::query(
                "insert into evidence_embedding (tenant_id, space_id, evidence_id, chunk, byte_start, byte_end, v, bits)
                 values ($1, $2, $3, $4, $5, $6, $7, binary_quantize($7)) on conflict do nothing",
            )
            .bind(self.tenant_id)
            .bind(&self.space.id)
            .bind(p.evidence_id)
            .bind(p.chunk)
            .bind(p.start as i32)
            .bind(p.end as i32)
            .bind(HalfVector::from_f32_slice(v))
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "insert into feed_cursor (tenant_id, consumer, last_seq) values ($1, $2, $3)
             on conflict (tenant_id, consumer) do update set last_seq = excluded.last_seq",
        )
        .bind(self.tenant_id)
        .bind(&consumer)
        .bind(last)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if at_head {
            self.promote_if_building(last).await?;
        }
        Ok(vectors.len())
    }

    /// A building space that has read the whole feed takes over from the active one.
    async fn promote_if_building(&self, cursor: i64) -> Result<(), IndexError> {
        let mut tx = self.begin().await?;
        let state: String =
            sqlx::query_scalar("select state from embedding_space where tenant_id = $1 and space_id = $2")
                .bind(self.tenant_id)
                .bind(&self.space.id)
                .fetch_one(&mut *tx)
                .await?;
        let head: i64 = sqlx::query_scalar("select coalesce(max(tenant_seq), 0) from outbox where tenant_id = $1")
            .bind(self.tenant_id)
            .fetch_one(&mut *tx)
            .await?;
        if state == "building" && cursor >= head {
            self.activate(&mut tx).await?;
            tracing::info!(space = %self.space.id, "embedding space caught up and is now active");
        }
        tx.commit().await?;
        Ok(())
    }

    async fn activate(&self, tx: &mut Transaction<'static, Postgres>) -> Result<(), sqlx::Error> {
        sqlx::query("update embedding_space set state = 'retired' where tenant_id = $1 and state = 'active'")
            .bind(self.tenant_id)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "update embedding_space set state = 'active', activated_at = now() where tenant_id = $1 and space_id = $2",
        )
        .bind(self.tenant_id)
        .bind(&self.space.id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn begin(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select set_config('orbit.tenant_id', $1, true)")
            .bind(self.tenant_id.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}

async fn read_cursor(
    tx: &mut Transaction<'static, Postgres>,
    tenant: Uuid,
    consumer: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("select coalesce((select last_seq from feed_cursor where tenant_id = $1 and consumer = $2), 0)")
        .bind(tenant)
        .bind(consumer)
        .fetch_one(&mut **tx)
        .await
}
