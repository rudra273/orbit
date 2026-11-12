//! Lexical search over raw evidence.

use crate::{Recall, RecallError, Verdict, text::clip};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_LIMIT: i64 = 50;
const MAX_HIT_BYTES: usize = 1_500;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// What to look for, in plain words.
    pub query: String,
    /// How many results, at most 50.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Also search tool calls and tool output, which are excluded by default.
    #[serde(default)]
    pub include_tool_activity: bool,
}

fn default_limit() -> i64 {
    8
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Hit {
    pub evidence_id: String,
    /// The recorded words, cut to a readable length when long (see `truncated`).
    pub text: String,
    pub truncated: bool,
    /// user | assistant | tool | system
    pub author: String,
    pub at: DateTime<Utc>,
    /// The client it came through, such as claude-desktop.
    pub via: Option<String>,
    pub conversation: Option<String>,
    pub integrity: String,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SearchResult {
    pub status: Verdict,
    pub hits: Vec<Hit>,
}

type Row = (
    Uuid,
    Vec<u8>,
    String,
    DateTime<Utc>,
    Option<String>,
    Option<String>,
    String,
    f32,
);

impl Recall {
    /// Ranked full-text search. Any query word may match; rarer and denser matches rank higher.
    pub async fn search(&self, request: &SearchRequest) -> Result<SearchResult, RecallError> {
        let mut tx = self.begin().await?;
        let rows: Vec<Row> = sqlx::query_as(
            "with q as (select nullif(replace(plainto_tsquery('english', $2)::text, '&', '|'), '')::tsquery as q)
             select e.evidence_id, e.content, e.author_role, e.occurred_at,
                    s.app, s.title, e.integrity::text, ts_rank_cd(x.tsv, q.q)
             from q, evidence_search x
             join evidence e using (tenant_id, evidence_id)
             join source s using (tenant_id, source_id)
             where x.tenant_id = $1 and q.q is not null and x.tsv @@ q.q and e.erased_at is null and e.content is not null
               and ($4 or (e.author_role in ('user', 'assistant') and coalesce(e.author_ref, '') not like 'tool_use:%'
                           and coalesce(e.author_ref, '') <> 'command_execution'))
             order by 8 desc, e.occurred_at desc
             limit $3",
        )
        .bind(self.tenant_id)
        .bind(&request.query)
        .bind(request.limit.clamp(1, MAX_LIMIT))
        .bind(request.include_tool_activity)
        .fetch_all(&mut *tx)
        .await?;

        let hits: Vec<Hit> = rows
            .into_iter()
            .map(|(id, content, author, at, via, conversation, integrity, score)| {
                let (text, truncated) = clip(&String::from_utf8_lossy(&content), MAX_HIT_BYTES);
                Hit {
                    evidence_id: format!("ev_{}", id.simple()),
                    text,
                    truncated,
                    author,
                    at,
                    via,
                    conversation,
                    integrity,
                    score,
                }
            })
            .collect();
        let status = if hits.is_empty() {
            Verdict::NotFound
        } else {
            Verdict::Grounded
        };
        Ok(SearchResult { status, hits })
    }
}
