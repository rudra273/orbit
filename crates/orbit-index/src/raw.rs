//! Lexical and literal indexing: full-text vectors and detected literals for new evidence.
//!
//! Reads the change feed after a stored cursor and, for each new piece of evidence,
//! writes its full-text vector and its literals. Idempotent: re-running a batch
//! changes nothing.

use crate::IndexError;
use orbit_core::sha256;
use orbit_formation::{LiteralKind, detect_literals};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const CONSUMER: &str = "raw-index";
const INDEXED_CHARS: usize = 200_000;

/// Keeps the full-text and literal indexes of one brain up to date.
#[derive(Clone)]
pub struct RawIndexer {
    pool: PgPool,
    tenant_id: Uuid,
}

impl RawIndexer {
    pub fn new(pool: PgPool, tenant_id: Uuid) -> Self {
        Self { pool, tenant_id }
    }

    /// Indexes up to `batch` feed entries. Returns how many pieces of evidence were indexed.
    pub async fn run_once(&self, batch: i64) -> Result<usize, IndexError> {
        let mut tx = self.begin().await?;

        let cursor: i64 = sqlx::query_scalar(
            "select coalesce((select last_seq from feed_cursor where tenant_id = $1 and consumer = $2), 0)",
        )
        .bind(self.tenant_id)
        .bind(CONSUMER)
        .fetch_one(&mut *tx)
        .await?;
        let feed: Vec<(i64, String, serde_json::Value)> = sqlx::query_as(
            "select tenant_seq, topic, payload from outbox where tenant_id = $1 and tenant_seq > $2 order by tenant_seq limit $3",
        )
        .bind(self.tenant_id)
        .bind(cursor)
        .bind(batch)
        .fetch_all(&mut *tx)
        .await?;
        let Some(&(last, ..)) = feed.last() else { return Ok(0) };

        let ids: Vec<Uuid> = feed
            .iter()
            .filter(|(_, topic, _)| topic == "evidence.appended")
            .filter_map(|(_, _, p)| p["evidence_id"].as_str().and_then(|s| s.parse().ok()))
            .collect();

        let rows: Vec<(Uuid, String, Vec<u8>)> = sqlx::query_as(
            "select evidence_id, author_role, content from evidence
             where tenant_id = $1 and evidence_id = any($2) and content is not null",
        )
        .bind(self.tenant_id)
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await?;
        for (evidence_id, author, content) in &rows {
            let text = String::from_utf8_lossy(content);
            self.index_text(&mut tx, *evidence_id, &text).await?;
            self.index_literals(&mut tx, *evidence_id, author, &text).await?;
        }

        sqlx::query(
            "insert into feed_cursor (tenant_id, consumer, last_seq) values ($1, $2, $3)
             on conflict (tenant_id, consumer) do update set last_seq = excluded.last_seq",
        )
        .bind(self.tenant_id)
        .bind(CONSUMER)
        .bind(last)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows.len())
    }

    /// Full-text vector of the first 200,000 characters. PostgreSQL text cannot hold NUL,
    /// so NUL becomes a space in the indexed copy only; the evidence keeps its exact bytes.
    async fn index_text(
        &self,
        tx: &mut Transaction<'static, Postgres>,
        evidence_id: Uuid,
        text: &str,
    ) -> Result<(), IndexError> {
        let indexed: String = text
            .chars()
            .take(INDEXED_CHARS)
            .map(|c| if c == '\0' { ' ' } else { c })
            .collect();
        sqlx::query(
            "insert into evidence_search (tenant_id, evidence_id, tsv) values ($1, $2, to_tsvector('english', $3))
             on conflict do nothing",
        )
        .bind(self.tenant_id)
        .bind(evidence_id)
        .bind(indexed)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn index_literals(
        &self,
        tx: &mut Transaction<'static, Postgres>,
        evidence_id: Uuid,
        author: &str,
        text: &str,
    ) -> Result<(), IndexError> {
        let role = match author {
            "user" => "typed_by_user",
            "assistant" => "suggested_by_assistant",
            "tool" | "system" => "tool_output",
            _ => "document",
        };
        for lit in detect_literals(text) {
            let literal_id: Uuid = sqlx::query_scalar(
                "insert into literal (tenant_id, kind, exact, norm, exact_sha256) values ($1, $2, $3, $4, $5)
                 on conflict (tenant_id, kind, exact_sha256) do update set norm = excluded.norm
                 returning literal_id",
            )
            .bind(self.tenant_id)
            .bind(kind_name(lit.kind))
            .bind(lit.exact.replace('\0', " "))
            .bind(normalise(&lit.exact))
            .bind(&sha256(lit.exact.as_bytes())[..])
            .fetch_one(&mut **tx)
            .await?;
            sqlx::query(
                "insert into literal_occurrence (tenant_id, literal_id, evidence_id, byte_start, byte_end, span_sha256, role)
                 values ($1, $2, $3, $4, $5, $6, $7) on conflict do nothing",
            )
            .bind(self.tenant_id)
            .bind(literal_id)
            .bind(evidence_id)
            .bind(lit.start as i32)
            .bind(lit.end as i32)
            .bind(&sha256(lit.exact.as_bytes())[..])
            .bind(role)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }

    async fn begin(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let tenant = self.tenant_id.to_string();
        sqlx::query("select set_config('orbit.tenant_id', $1, true)")
            .bind(&tenant)
            .execute(&mut *tx)
            .await?;
        sqlx::query("select pg_advisory_xact_lock(hashtextextended('raw-index:' || $1, 0))")
            .bind(&tenant)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}

pub fn kind_name(kind: LiteralKind) -> &'static str {
    match kind {
        LiteralKind::Code => "code",
        LiteralKind::Command => "command",
        LiteralKind::Url => "url",
        LiteralKind::Path => "path",
        LiteralKind::Version => "version",
    }
}

/// The lookup key for a literal: lower case, whitespace collapsed. Never returned to callers.
pub fn normalise(exact: &str) -> String {
    exact
        .replace('\0', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_key_ignores_case_and_spacing() {
        assert_eq!(normalise("Cargo   test\n-p X"), "cargo test -p x");
    }
}
