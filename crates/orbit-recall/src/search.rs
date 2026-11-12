//! Search over raw evidence: a dense (meaning) channel and a lexical (words) channel, fused.
//!
//! Fusion is reciprocal-rank (k = 60) with the lexical channel at 0.1 of the dense weight: on the
//! owner's history, equal weights lowered accuracy and 0.1 matched or beat dense alone
//! (evals/embed-bench). The lower weight applies only where both channels could see the evidence:
//! text that is never embedded (labelled `sensitive`, very short, tool activity) is ranked by the
//! lexical channel alone at full weight, or it would sink below every match by meaning. Without
//! an embedder or an active embedding space, or when the provider fails, search is lexical only
//! and says so in `channels`.

use crate::{Recall, RecallError, Verdict, text::clip};
use chrono::{DateTime, Utc};
use orbit_embed::{Role, Space};
use pgvector::HalfVector;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

const MAX_LIMIT: i64 = 50;
const MAX_HIT_BYTES: usize = 1_500;
/// Candidates each channel contributes to fusion.
const CANDIDATES: i64 = 50;
/// Chunks read from the dense channel; several may belong to one piece of evidence.
const DENSE_CHUNKS: i64 = 200;
/// Chunks the 1-bit scan passes on to be re-ranked with full vectors.
const BINARY_CANDIDATES: i64 = 1_000;
const RRF_K: f64 = 60.0;
const LEXICAL_WEIGHT: f64 = 0.1;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// What to look for, in plain words. A self-contained question works best: name the project
    /// or topic instead of "it" or "that".
    pub query: String,
    /// How many results, at most 50.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Also search tool calls and tool output, which are excluded by default (lexical only).
    #[serde(default)]
    pub include_tool_activity: bool,
}

fn default_limit() -> i64 {
    8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    /// Close in meaning (embeddings).
    Dense,
    /// Shares words with the query (full-text search).
    Lexical,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Hit {
    pub evidence_id: String,
    /// The recorded words, cut to a readable length when long (see `truncated`). For a match by
    /// meaning, the passage that matched.
    pub text: String,
    pub truncated: bool,
    /// user | assistant | tool | system
    pub author: String,
    pub at: DateTime<Utc>,
    /// The client it came through, such as claude-desktop.
    pub via: Option<String>,
    pub conversation: Option<String>,
    pub integrity: String,
    /// Which channels found it.
    pub matched_by: Vec<Channel>,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    /// `grounded` when a hit shares words with the query; `partial` when hits are only close in
    /// meaning — related material to check, not a confirmed match; `not_found` when nothing came back.
    pub status: Verdict,
    /// The channels that ran.
    pub channels: Vec<Channel>,
    pub hits: Vec<Hit>,
}

/// A candidate from the dense channel: the best-matching chunk of one piece of evidence.
struct DenseHit {
    start: usize,
    end: usize,
}

/// What the dense channel returned, and from which space.
struct DenseRun {
    hits: Vec<(Uuid, DenseHit)>,
    space_id: String,
}

impl Recall {
    /// Ranked search: by meaning when an embedding space is active, and by words, fused.
    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResult, RecallError> {
        let limit = request.limit.clamp(1, MAX_LIMIT) as usize;
        let lexical = self.lexical(request).await?;
        let dense = self.dense(&request.query).await?;
        let embedded = match &dense {
            Some(run) => self.embedded(&run.space_id, &lexical).await?,
            None => Default::default(),
        };
        let dense = dense.map(|run| run.hits);

        let mut channels = Vec::new();
        let mut fused: HashMap<Uuid, f64> = HashMap::new();
        if let Some(dense) = &dense {
            channels.push(Channel::Dense);
            for (rank, (id, _)) in dense.iter().enumerate() {
                *fused.entry(*id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
        }
        channels.push(Channel::Lexical);
        for (rank, id) in lexical.iter().enumerate() {
            let weight = if embedded.contains(id) { LEXICAL_WEIGHT } else { 1.0 };
            *fused.entry(*id).or_default() += weight / (RRF_K + rank as f64 + 1.0);
        }
        let mut ranked: Vec<(Uuid, f64)> = fused.into_iter().collect();
        // On a tie, a hit that shares the query's words (grounded) goes first.
        ranked.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then(lexical.contains(&b.0).cmp(&lexical.contains(&a.0)))
                .then(a.0.cmp(&b.0))
        });
        ranked.truncate(limit);

        let dense_hits: HashMap<Uuid, DenseHit> = dense.unwrap_or_default().into_iter().collect();
        let hits = self.hits(&ranked, &lexical, &dense_hits).await?;
        let status = if hits.is_empty() {
            Verdict::NotFound
        } else if hits.iter().any(|h| h.matched_by.contains(&Channel::Lexical)) {
            Verdict::Grounded
        } else {
            Verdict::Partial
        };
        Ok(SearchResult { status, channels, hits })
    }

    /// Evidence ids ranked by full-text match. Any query word may match; rarer and denser matches rank higher.
    async fn lexical(&self, request: &SearchRequest) -> Result<Vec<Uuid>, RecallError> {
        let mut tx = self.begin().await?;
        let ids = sqlx::query_scalar(
            "with q as (select nullif(replace(plainto_tsquery('english', $2)::text, '&', '|'), '')::tsquery as q)
             select e.evidence_id
             from q, evidence_search x
             join evidence e using (tenant_id, evidence_id)
             where x.tenant_id = $1 and q.q is not null and x.tsv @@ q.q and e.erased_at is null and e.content is not null
               and ($4 or (e.author_role in ('user', 'assistant') and coalesce(e.author_ref, '') not like 'tool_use:%'
                           and coalesce(e.author_ref, '') <> 'command_execution'))
             order by ts_rank_cd(x.tsv, q.q) desc, e.occurred_at desc
             limit $3",
        )
        .bind(self.tenant_id)
        .bind(&request.query)
        .bind(CANDIDATES)
        .bind(request.include_tool_activity)
        .fetch_all(&mut *tx)
        .await?;
        Ok(ids)
    }

    /// Which of `ids` the dense channel can see in a space.
    async fn embedded(&self, space_id: &str, ids: &[Uuid]) -> Result<std::collections::HashSet<Uuid>, RecallError> {
        let mut tx = self.begin().await?;
        let rows: Vec<Uuid> = sqlx::query_scalar(
            "select distinct evidence_id from evidence_embedding
             where tenant_id = $1 and space_id = $2 and evidence_id = any($3)",
        )
        .bind(self.tenant_id)
        .bind(space_id)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Evidence ranked by meaning, each with its best chunk, and the space searched; `None` when
    /// the channel cannot run.
    async fn dense(&self, query: &str) -> Result<Option<DenseRun>, RecallError> {
        let Some(embedder) = &self.embedder else {
            return Ok(None);
        };
        let Some(space) = self.active_space().await? else {
            return Ok(None);
        };
        let vector = match embedder.embed(&space, Role::Query, &[query.to_string()]).await {
            Ok(mut v) if v.len() == 1 && v[0].len() == space.dims => v.remove(0),
            Ok(_) => {
                tracing::warn!(space = %space.id, "embedder returned the wrong shape; searching by words only");
                return Ok(None);
            }
            Err(error) => {
                tracing::warn!(%error, "query embedding failed; searching by words only");
                return Ok(None);
            }
        };

        let mut tx = self.begin().await?;
        // Every chunk is compared by its 1-bit copy; the nearest are re-ranked with the full vectors
        // (see migrations/0005_dense.sql for the measurements). No ANN index at this scale.
        let rows: Vec<(Uuid, i32, i32)> = sqlx::query_as(
            "with q as (select $3::halfvec as v),
             near as (
               select x.evidence_id, x.byte_start, x.byte_end, x.v
               from q, evidence_embedding x
               where x.tenant_id = $1 and x.space_id = $2
               order by x.bits <~> binary_quantize(q.v)
               limit $5)
             select near.evidence_id, near.byte_start, near.byte_end
             from q, near join evidence e on e.tenant_id = $1 and e.evidence_id = near.evidence_id
             where e.erased_at is null and e.content is not null
             order by near.v <=> q.v
             limit $4",
        )
        .bind(self.tenant_id)
        .bind(&space.id)
        .bind(HalfVector::from_f32_slice(&vector))
        .bind(DENSE_CHUNKS)
        .bind(BINARY_CANDIDATES)
        .fetch_all(&mut *tx)
        .await?;

        let mut seen = std::collections::HashSet::new();
        let ranked = rows
            .into_iter()
            .filter(|(id, ..)| seen.insert(*id))
            .take(CANDIDATES as usize)
            .map(|(id, start, end)| {
                (
                    id,
                    DenseHit {
                        start: start as usize,
                        end: end as usize,
                    },
                )
            })
            .collect();
        Ok(Some(DenseRun {
            hits: ranked,
            space_id: space.id,
        }))
    }

    async fn active_space(&self) -> Result<Option<Space>, RecallError> {
        let mut tx = self.begin().await?;
        let row: Option<(String, String, i32, String, String, bool)> = sqlx::query_as(
            "select space_id, model, dims, query_prefix, doc_prefix, hosted from embedding_space
             where tenant_id = $1 and state = 'active'",
        )
        .bind(self.tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        Ok(row.map(|(id, model, dims, query_prefix, doc_prefix, hosted)| Space {
            id,
            model,
            dims: dims as usize,
            query_prefix,
            doc_prefix,
            hosted,
        }))
    }

    async fn hits(
        &self,
        ranked: &[(Uuid, f64)],
        lexical: &[Uuid],
        dense: &HashMap<Uuid, DenseHit>,
    ) -> Result<Vec<Hit>, RecallError> {
        let ids: Vec<Uuid> = ranked.iter().map(|(id, _)| *id).collect();
        let mut tx = self.begin().await?;
        type Row = (
            Uuid,
            Vec<u8>,
            String,
            DateTime<Utc>,
            Option<String>,
            Option<String>,
            String,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "select e.evidence_id, e.content, e.author_role, e.occurred_at, s.app, s.title, e.integrity::text
             from evidence e join source s using (tenant_id, source_id)
             where e.tenant_id = $1 and e.evidence_id = any($2) and e.content is not null",
        )
        .bind(self.tenant_id)
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await?;
        let mut rows: HashMap<Uuid, Row> = rows.into_iter().map(|r| (r.0, r)).collect();

        Ok(ranked
            .iter()
            .filter_map(|(id, score)| {
                let (_, content, author, at, via, conversation, integrity) = rows.remove(id)?;
                let text = String::from_utf8_lossy(&content);
                let passage = dense.get(id).and_then(|d| text.get(d.start..d.end)).unwrap_or(&text);
                let (text, truncated) = clip(passage, MAX_HIT_BYTES);
                let mut matched_by = Vec::new();
                if dense.contains_key(id) {
                    matched_by.push(Channel::Dense);
                }
                if lexical.contains(id) {
                    matched_by.push(Channel::Lexical);
                }
                Some(Hit {
                    evidence_id: format!("ev_{}", id.simple()),
                    text,
                    truncated: truncated || passage.len() < content.len(),
                    author,
                    at,
                    via,
                    conversation,
                    integrity,
                    matched_by,
                    score: *score as f32,
                })
            })
            .collect())
    }
}
