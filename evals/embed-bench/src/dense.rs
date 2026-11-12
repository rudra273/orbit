//! Embedding models, with the query and document formats each model was trained on.

use crate::{data::Bench, metrics::Ranked};
use anyhow::Context;
use fastembed::{EmbeddingModel, Qwen3TextEmbedding, TextEmbedding, TextInitOptions};

const MAX_TOKENS: usize = 512;
const TOP: usize = 50;

pub enum Backend {
    Onnx(EmbeddingModel),
    Qwen3(&'static str),
}

pub struct Spec {
    pub backend: Backend,
    pub query_prefix: &'static str,
    pub doc_prefix: &'static str,
}

/// Prompt formats are from each model's card.
pub fn spec(name: &str) -> anyhow::Result<Spec> {
    let (backend, query_prefix, doc_prefix) = match name {
        "minilm" => (Backend::Onnx(EmbeddingModel::AllMiniLML6V2), "", ""),
        "nomic-v1.5" => (Backend::Onnx(EmbeddingModel::NomicEmbedTextV15), "search_query: ", "search_document: "),
        "gemma-300m" => (
            Backend::Onnx(EmbeddingModel::EmbeddingGemma300M),
            "task: search result | query: ",
            "title: none | text: ",
        ),
        "bge-m3" => (Backend::Onnx(EmbeddingModel::BGEM3), "", ""),
        "e5-large" => (Backend::Onnx(EmbeddingModel::MultilingualE5Large), "query: ", "passage: "),
        "arctic-l" => (
            Backend::Onnx(EmbeddingModel::SnowflakeArcticEmbedL),
            "Represent this sentence for searching relevant passages: ",
            "",
        ),
        "qwen3-0.6b" => (
            Backend::Qwen3("Qwen/Qwen3-Embedding-0.6B"),
            "Instruct: Given a question or topic, retrieve the conversation turns that answer or discuss it\nQuery:",
            "",
        ),
        other => anyhow::bail!("unknown model {other}"),
    };
    Ok(Spec { backend, query_prefix, doc_prefix })
}

pub struct Result {
    pub dims: usize,
    pub lists: Vec<Ranked>,
}

pub fn rank(spec: &Spec, bench: &Bench) -> anyhow::Result<Result> {
    let docs: Vec<String> = bench.docs.iter().map(|d| format!("{}{}", spec.doc_prefix, d.text)).collect();
    let queries: Vec<Vec<String>> = bench
        .tasks
        .iter()
        .map(|t| t.queries.iter().map(|q| format!("{}{}", spec.query_prefix, q.text)).collect())
        .collect();

    let (doc_vecs, query_vecs) = match &spec.backend {
        Backend::Onnx(model) => {
            let mut m = TextEmbedding::try_new(TextInitOptions::new(model.clone()).with_max_length(MAX_TOKENS))
                .context("loading model")?;
            let d = m.embed(&docs, Some(32))?;
            let q = queries.iter().map(|qs| m.embed(qs, Some(32))).collect::<std::result::Result<Vec<_>, _>>()?;
            (d, q)
        }
        Backend::Qwen3(repo) => {
            let device = candle_core::Device::new_metal(0).unwrap_or(candle_core::Device::Cpu);
            let m = Qwen3TextEmbedding::from_hf(repo, &device, candle_core::DType::F32, MAX_TOKENS)?;
            let d = docs.chunks(32).map(|c| m.embed(c)).collect::<std::result::Result<Vec<_>, _>>()?.concat();
            let q = queries.iter().map(|qs| m.embed(qs)).collect::<std::result::Result<Vec<_>, _>>()?;
            (d, q)
        }
    };

    let doc_vecs: Vec<Vec<f32>> = doc_vecs.into_iter().map(normalise).collect();
    let dims = doc_vecs.first().map_or(0, Vec::len);
    let lists = query_vecs
        .into_iter()
        .map(|qs| {
            qs.into_iter()
                .map(|q| {
                    let q = normalise(q);
                    let mut scored: Vec<(usize, f32)> =
                        doc_vecs.iter().enumerate().map(|(i, d)| (i, d.iter().zip(&q).map(|(a, b)| a * b).sum())).collect();
                    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                    scored.into_iter().take(TOP).map(|(i, _)| bench.docs[i].id).collect()
                })
                .collect()
        })
        .collect();
    Ok(Result { dims, lists })
}

fn normalise(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}
