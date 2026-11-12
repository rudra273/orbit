//! Exact recall: commands, paths, URLs, versions and code, returned byte for byte.
//!
//! Matches come from the literal index, never from generated text. A request is
//! matched two ways: as the string itself (typo-tolerant lookup) and as a
//! description, via the literals inside the best search hits.

use crate::{Recall, RecallError, SearchRequest, Verdict, text::floor_boundary};
use chrono::{DateTime, Utc};
use orbit_core::sha256;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const CONTEXT_BYTES: usize = 160;
const DESCRIPTIVE_HITS: i64 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiteralRole {
    TypedByUser,
    SuggestedByAssistant,
    ToolOutput,
    Document,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
pub struct ExactRequest {
    /// The string itself, or a description: "the command I used to run the migrations".
    pub what: String,
    /// Only this kind: command, code, path, url or version.
    #[serde(default)]
    pub kind: Option<String>,
    /// Only literals with this origin.
    #[serde(default)]
    pub role: Option<LiteralRole>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    5
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ExactMatch {
    /// Byte-for-byte what was recorded. Reproduce it unchanged.
    pub exact: String,
    pub sha256: String,
    pub kind: String,
    pub role: String,
    pub evidence_id: String,
    pub at: DateTime<Utc>,
    pub via: Option<String>,
    pub context_before: String,
    pub context_after: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ExactResult {
    pub status: Verdict,
    pub matches: Vec<ExactMatch>,
}

type Row = (
    String,
    String,
    String,
    Uuid,
    i32,
    i32,
    DateTime<Utc>,
    Option<String>,
    Vec<u8>,
);

impl Recall {
    pub async fn exact(&self, request: &ExactRequest) -> Result<ExactResult, RecallError> {
        let limit = request.limit.clamp(1, 20);
        let role = request.role.map(role_name);

        // Evidence that a description points at, best first.
        let described = self
            .search(&SearchRequest {
                query: request.what.clone(),
                limit: DESCRIPTIVE_HITS,
                include_tool_activity: true,
            })
            .await?;
        let ranked: Vec<Uuid> = described
            .hits
            .iter()
            .filter_map(|h| h.evidence_id.strip_prefix("ev_").and_then(|s| Uuid::parse_str(s).ok()))
            .collect();

        let mut tx = self.begin().await?;
        let rows: Vec<Row> = sqlx::query_as(
            "with direct as (
               select l.literal_id, 0 as rank, similarity(l.norm, lower($2)) as sim
               from literal l
               where l.tenant_id = $1 and (l.norm % lower($2) or l.norm like '%' || lower($2) || '%')
             ),
             described as (
               select o.literal_id, array_position($5::uuid[], o.evidence_id) as rank, 0::real as sim
               from literal_occurrence o
               where o.tenant_id = $1 and o.evidence_id = any($5)
             ),
             candidates as (
               select literal_id, min(rank) as rank, max(sim) as sim from (select * from direct union all select * from described) c
               group by literal_id
             )
             select l.exact, l.kind, o.role, o.evidence_id, o.byte_start, o.byte_end, e.occurred_at, s.app, e.content
             from candidates c
             join literal l on l.tenant_id = $1 and l.literal_id = c.literal_id
             join literal_occurrence o on o.tenant_id = $1 and o.literal_id = l.literal_id
             join evidence e on e.tenant_id = $1 and e.evidence_id = o.evidence_id
             join source s on s.tenant_id = $1 and s.source_id = e.source_id
             where e.erased_at is null and e.content is not null and ($3::text is null or l.kind = $3) and ($4::text is null or o.role = $4)
             order by c.sim desc, c.rank asc, e.occurred_at desc
             limit $6",
        )
        .bind(self.tenant_id)
        .bind(&request.what)
        .bind(&request.kind)
        .bind(role)
        .bind(&ranked)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;

        let matches: Vec<ExactMatch> = rows
            .into_iter()
            .filter_map(|(exact, kind, role, evidence_id, start, end, at, via, content)| {
                let (start, end) = (start as usize, end as usize);
                let content = String::from_utf8(content).ok()?;
                // INV-3: only return a string that is still exactly where it was recorded.
                (content.get(start..end) == Some(exact.as_str())).then(|| ExactMatch {
                    sha256: hex(&sha256(exact.as_bytes())),
                    context_before: content[floor_boundary(&content, start.saturating_sub(CONTEXT_BYTES))..start]
                        .to_string(),
                    context_after: content[end..floor_boundary(&content, end + CONTEXT_BYTES)].to_string(),
                    exact,
                    kind,
                    role,
                    evidence_id: format!("ev_{}", evidence_id.simple()),
                    at,
                    via,
                })
            })
            .collect();
        let status = if matches.is_empty() {
            Verdict::NotFound
        } else {
            Verdict::Grounded
        };
        Ok(ExactResult { status, matches })
    }
}

fn role_name(role: LiteralRole) -> &'static str {
    match role {
        LiteralRole::TypedByUser => "typed_by_user",
        LiteralRole::SuggestedByAssistant => "suggested_by_assistant",
        LiteralRole::ToolOutput => "tool_output",
        LiteralRole::Document => "document",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
