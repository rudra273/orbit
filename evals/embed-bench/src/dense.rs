//! Dense retrieval with an embedding model: embed the corpus and queries, rank by cosine.

use crate::{
    data::Bench,
    gemma2::{self, Embedder},
    metrics::Ranked,
};
use std::{io::Write, path::Path, time::Instant};

const TOP: usize = 50;
const BATCH: usize = 16;

pub struct Run {
    pub lists: Vec<Ranked>,
    pub dims: usize,
    pub docs_per_sec: f64,
}

/// `file` selects the precision: `model_quantized.onnx` or `model.onnx`.
pub fn gemma2(dir: &Path, file: &str, max_tokens: usize, bench: &Bench) -> anyhow::Result<Run> {
    let mut model = Embedder::load(dir, file, max_tokens)?;
    let docs: Vec<String> = bench
        .docs
        .iter()
        .map(|d| format!("{}{}", gemma2::DOC_PREFIX, d.text))
        .collect();

    let start = Instant::now();
    let total = docs.len();
    let doc_vecs = model.embed(&docs, BATCH, |done| {
        if done % 400 < BATCH || done == total {
            let rate = done as f64 / start.elapsed().as_secs_f64();
            print!(
                "\r  {file}: {done}/{total} docs, {rate:.0}/s, ~{:.0}s left   ",
                (total - done) as f64 / rate.max(0.1)
            );
            std::io::stdout().flush().ok();
        }
    })?;
    let docs_per_sec = total as f64 / start.elapsed().as_secs_f64();
    println!();

    let lists = bench
        .tasks
        .iter()
        .map(|task| {
            let queries: Vec<String> = task
                .queries
                .iter()
                .map(|q| format!("{}{}", gemma2::QUERY_PREFIX, q.text))
                .collect();
            let vecs = model.embed(&queries, BATCH, |_| {})?;
            Ok(vecs.iter().map(|q| top(q, &doc_vecs, bench)).collect())
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Run {
        lists,
        dims: doc_vecs.first().map_or(0, Vec::len),
        docs_per_sec,
    })
}

/// Embeds through an API model (see `api.rs`); queries use the model's query format.
///
/// Each size in `dims` is scored twice: the first `d` numbers of every vector, renormalised (Matryoshka
/// truncation), ranked exactly; and a 1-bit copy (sign of each number) that picks 200 candidates by Hamming
/// distance, which are then re-ranked with the `d`-number vectors.
pub async fn api(model: &str, dims: &[usize], bench: &Bench) -> anyhow::Result<Vec<(String, Run)>> {
    let client = crate::api::Client::new(model)?;
    let docs: Vec<String> = bench.docs.iter().map(|d| d.text.clone()).collect();
    let start = Instant::now();
    let doc_vecs = client.embed("docs", &docs).await?;
    let docs_per_sec = docs.len() as f64 / start.elapsed().as_secs_f64();
    let mut query_vecs = Vec::new();
    for (i, task) in bench.tasks.iter().enumerate() {
        let queries: Vec<String> = task
            .queries
            .iter()
            .map(|q| crate::api::query_format(model, &q.text))
            .collect();
        query_vecs.push(client.embed(&format!("queries{i}"), &queries).await?);
    }
    let full = doc_vecs.first().map_or(0, Vec::len);
    let sizes: Vec<usize> = if dims.is_empty() {
        vec![full]
    } else {
        dims.iter().copied().filter(|d| *d <= full).collect()
    };

    let mut runs = Vec::new();
    for d in sizes {
        let docs_d: Vec<Vec<f32>> = doc_vecs.iter().map(|v| shorten(v, d)).collect();
        let docs_bits: Vec<Vec<u64>> = docs_d.iter().map(|v| bits(v)).collect();
        let mut exact = Vec::new();
        let mut binary = Vec::new();
        for queries in &query_vecs {
            let qs: Vec<Vec<f32>> = queries.iter().map(|v| shorten(v, d)).collect();
            exact.push(qs.iter().map(|q| top(q, &docs_d, bench)).collect());
            binary.push(qs.iter().map(|q| top_binary(q, &docs_d, &docs_bits, bench)).collect());
        }
        runs.push((
            format!("{model} {d}d"),
            Run {
                lists: exact,
                dims: d,
                docs_per_sec,
            },
        ));
        runs.push((
            format!("{model} {d}d 1-bit + rescore"),
            Run {
                lists: binary,
                dims: d,
                docs_per_sec,
            },
        ));
    }
    Ok(runs)
}

/// The first `d` numbers, renormalised to unit length.
fn shorten(v: &[f32], d: usize) -> Vec<f32> {
    gemma2::normalise(v[..d].to_vec())
}

/// One bit per number: set when it is positive.
fn bits(v: &[f32]) -> Vec<u64> {
    v.chunks(64)
        .map(|c| {
            c.iter()
                .enumerate()
                .fold(0u64, |w, (i, x)| if *x > 0.0 { w | 1 << i } else { w })
        })
        .collect()
}

const CANDIDATES: usize = 200;

fn top_binary(query: &[f32], docs: &[Vec<f32>], doc_bits: &[Vec<u64>], bench: &Bench) -> Vec<uuid::Uuid> {
    let qb = bits(query);
    let mut near: Vec<(usize, u32)> = doc_bits
        .iter()
        .enumerate()
        .map(|(i, b)| (i, b.iter().zip(&qb).map(|(x, y)| (x ^ y).count_ones()).sum()))
        .collect();
    near.sort_by_key(|&(_, distance)| distance);
    let mut scored: Vec<(usize, f32)> = near
        .into_iter()
        .take(CANDIDATES)
        .map(|(i, _)| (i, gemma2::cosine(query, &docs[i])))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.into_iter().take(TOP).map(|(i, _)| bench.docs[i].id).collect()
}

fn top(query: &[f32], docs: &[Vec<f32>], bench: &Bench) -> Vec<uuid::Uuid> {
    let mut scored: Vec<(usize, f32)> = docs
        .iter()
        .enumerate()
        .map(|(i, d)| (i, gemma2::cosine(query, d)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.into_iter().take(TOP).map(|(i, _)| bench.docs[i].id).collect()
}
