//! Builds the corpus and the two query sets from imported evidence.

use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Text kept per document: about 400 tokens, the start of each turn.
const DOC_CHARS: usize = 1600;

pub struct Doc {
    pub id: Uuid,
    pub text: String,
}

pub struct Query {
    pub text: String,
    /// Documents that count as a correct answer.
    pub relevant: HashSet<Uuid>,
    /// A document that must not be returned (the query's own turn).
    pub exclude: Option<Uuid>,
}

pub struct Task {
    pub name: &'static str,
    pub queries: Vec<Query>,
}

pub struct Bench {
    pub docs: Vec<Doc>,
    pub tasks: [Task; 2],
}

type Row = (Uuid, Uuid, i32, String, Vec<u8>, Option<String>);

pub async fn load(pool: &PgPool, tenant: Uuid, max_docs: usize, max_questions: usize) -> anyhow::Result<Bench> {
    let rows: Vec<Row> = sqlx::query_as(
        "select e.evidence_id, e.source_id, e.seq, e.author_role, e.content, s.title
         from evidence e join source s using (tenant_id, source_id)
         where e.tenant_id = $1 and e.author_role in ('user', 'assistant') and e.content is not null
           and coalesce(e.author_ref, '') not like 'tool_use:%' and coalesce(e.author_ref, '') <> 'command_execution'
           and length(e.content) >= 40
         order by e.source_id, e.seq",
    )
    .bind(tenant)
    .fetch_all(pool)
    .await?;

    let text = |b: &[u8]| String::from_utf8_lossy(b).chars().take(DOC_CHARS).collect::<String>();

    // question → answer: a user question and the next assistant turn in the same session.
    let mut questions = Vec::new();
    for w in rows.windows(2) {
        let (q, a) = (&w[0], &w[1]);
        let qt = String::from_utf8_lossy(&q.4);
        let plain = !qt.trim_start().starts_with('<');
        if q.3 == "user"
            && a.3 == "assistant"
            && q.1 == a.1
            && plain
            && qt.contains('?')
            && (20..=400).contains(&qt.len())
        {
            questions.push(Query {
                text: qt.trim().to_string(),
                relevant: HashSet::from([a.0]),
                exclude: Some(q.0),
            });
        }
    }
    questions.sort_by_key(|q| stable_hash(&q.text));
    questions.truncate(max_questions);

    // title → session: the session's title; any of its turns is relevant.
    let mut by_source: HashMap<Uuid, (String, Vec<Uuid>)> = HashMap::new();
    for r in &rows {
        if let Some(title) = &r.5 {
            by_source
                .entry(r.1)
                .or_insert_with(|| (title.clone(), Vec::new()))
                .1
                .push(r.0);
        }
    }

    // Corpus: every relevant document, then deterministic distractors up to the limit.
    let mut keep: HashSet<Uuid> = questions
        .iter()
        .flat_map(|q| q.relevant.iter().chain(q.exclude.iter()).copied())
        .collect();
    let mut others: Vec<&Row> = rows.iter().filter(|r| !keep.contains(&r.0)).collect();
    others.sort_by_key(|r| stable_hash(&r.0.to_string()));
    for r in others {
        if keep.len() >= max_docs {
            break;
        }
        keep.insert(r.0);
    }
    let docs: Vec<Doc> = rows
        .iter()
        .filter(|r| keep.contains(&r.0))
        .map(|r| Doc {
            id: r.0,
            text: text(&r.4),
        })
        .collect();

    let mut titles: Vec<Query> = by_source
        .into_values()
        .filter_map(|(title, ids)| {
            let relevant: HashSet<Uuid> = ids.into_iter().filter(|id| keep.contains(id)).collect();
            (!relevant.is_empty()).then_some(Query {
                text: title,
                relevant,
                exclude: None,
            })
        })
        .collect();
    titles.sort_by_key(|q| stable_hash(&q.text));

    Ok(Bench {
        docs,
        tasks: [
            Task {
                name: "title→session",
                queries: titles,
            },
            Task {
                name: "question→answer",
                queries: questions,
            },
        ],
    })
}

/// FNV-1a: a fixed, seedless order for sampling.
pub fn stable_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}
