//! Lexical baselines: PostgreSQL full-text as Orbit runs it today, and BM25.

use crate::{data::Bench, metrics::Ranked};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

const TOP: usize = 50;

/// The production query (any word may match, ranked by `ts_rank_cd`), limited to the corpus.
pub async fn postgres(pool: &PgPool, tenant: Uuid, bench: &Bench) -> anyhow::Result<Vec<Ranked>> {
    let ids: Vec<Uuid> = bench.docs.iter().map(|d| d.id).collect();
    let mut out = Vec::new();
    for task in &bench.tasks {
        let mut lists = Vec::new();
        for q in &task.queries {
            let list: Vec<Uuid> = sqlx::query_scalar(
                "with q as (select nullif(replace(plainto_tsquery('english', $2)::text, '&', '|'), '')::tsquery as q)
                 select x.evidence_id from q, evidence_search x
                 where x.tenant_id = $1 and x.evidence_id = any($3) and q.q is not null and x.tsv @@ q.q
                 order by ts_rank_cd(x.tsv, q.q) desc limit 50",
            )
            .bind(tenant)
            .bind(&q.text)
            .bind(&ids)
            .fetch_all(pool)
            .await?;
            lists.push(list);
        }
        out.push(lists);
    }
    Ok(out)
}

/// Okapi BM25 (k1 = 1.2, b = 0.75) with a plain word tokenizer.
pub fn bm25(bench: &Bench) -> Vec<Ranked> {
    let docs: Vec<Vec<String>> = bench.docs.iter().map(|d| tokens(&d.text)).collect();
    let n = docs.len() as f64;
    let avg = docs.iter().map(Vec::len).sum::<usize>() as f64 / n;
    let mut df: HashMap<&str, f64> = HashMap::new();
    let mut tf: Vec<HashMap<&str, f64>> = Vec::with_capacity(docs.len());
    for d in &docs {
        let mut counts: HashMap<&str, f64> = HashMap::new();
        for t in d {
            *counts.entry(t.as_str()).or_default() += 1.0;
        }
        for t in counts.keys() {
            *df.entry(t).or_default() += 1.0;
        }
        tf.push(counts);
    }

    bench
        .tasks
        .iter()
        .map(|task| {
            task.queries
                .iter()
                .map(|q| {
                    let terms = tokens(&q.text);
                    let mut scored: Vec<(usize, f64)> = tf
                        .iter()
                        .enumerate()
                        .map(|(i, counts)| {
                            let len = docs[i].len() as f64;
                            let s = terms
                                .iter()
                                .filter_map(|t| {
                                    let f = *counts.get(t.as_str())?;
                                    let d = df[t.as_str()];
                                    let idf = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
                                    Some(idf * f * 2.2 / (f + 1.2 * (0.25 + 0.75 * len / avg)))
                                })
                                .sum::<f64>();
                            (i, s)
                        })
                        .filter(|(_, s)| *s > 0.0)
                        .collect();
                    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                    scored.into_iter().take(TOP).map(|(i, _)| bench.docs[i].id).collect()
                })
                .collect()
        })
        .collect()
}

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "is", "it", "for", "on", "with", "as", "at", "be", "this", "that",
    "are", "was", "i", "you", "we", "my", "me", "do", "can", "what", "how", "why", "which", "so", "if", "not", "no",
];

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map(str::to_lowercase)
        .filter(|t| t.len() > 1 && !STOP.contains(&t.as_str()))
        .collect()
}
