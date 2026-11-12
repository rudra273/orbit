//! Embeddings from a hosted model through OpenRouter's embeddings API.
//!
//! Inputs go in batches; dropped connections, 429 and 5xx answers are retried with backoff. Providers are asked
//! not to collect the data. Vectors are cached on disk so a rerun does not pay again.

use anyhow::{Context, bail};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

const URL: &str = "https://openrouter.ai/api/v1/embeddings";
const BATCH: usize = 64;
const RETRIES: u32 = 6;

/// Query instruction in the format from the Qwen3-Embedding model card; documents get none.
pub fn query_format(model: &str, text: &str) -> String {
    if model.starts_with("qwen/") {
        format!(
            "Instruct: Given a question or topic, retrieve the conversation turns that answer or discuss it\nQuery:{text}"
        )
    } else {
        text.to_string()
    }
}

#[derive(Deserialize)]
struct Response {
    data: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    index: Option<usize>,
    embedding: Vec<f32>,
}

pub struct Client {
    http: reqwest::Client,
    key: String,
    model: String,
}

impl Client {
    pub fn new(model: &str) -> anyhow::Result<Self> {
        let key = std::env::var("OPENROUTER_API_KEY").context("OPENROUTER_API_KEY is not set")?;
        let http = reqwest::Client::builder().timeout(Duration::from_secs(90)).build()?;
        Ok(Self {
            http,
            key,
            model: model.to_string(),
        })
    }

    /// Embeds texts in order, reading from and writing to the cache under `label`.
    pub async fn embed(&self, label: &str, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let path = cache_path(&self.model, label, texts);
        if let Some(cached) = read_cache(&path, texts.len()) {
            println!("  {}: {label} from cache", self.model);
            return Ok(cached);
        }
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(BATCH) {
            out.extend(self.request(chunk).await?);
            print!("\r  {}: {label} {}/{}   ", self.model, out.len(), texts.len());
            std::io::stdout().flush().ok();
        }
        println!();
        write_cache(&path, &out)?;
        Ok(out)
    }

    async fn request(&self, inputs: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": inputs,
            "encoding_format": "float",
            "provider": { "data_collection": "deny" },
        });
        for attempt in 0..=RETRIES {
            // Dropped connections and timeouts are retried like 429 and 5xx answers.
            let response = match self.http.post(URL).bearer_auth(&self.key).json(&body).send().await {
                Ok(r) => r,
                Err(e) if attempt < RETRIES => {
                    eprintln!("\n  request failed ({e}); retrying");
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            let status = response.status();
            if status.is_success() {
                let mut data = response.json::<Response>().await?.data;
                if data.len() != inputs.len() {
                    bail!("asked for {} embeddings, got {}", inputs.len(), data.len());
                }
                data.sort_by_key(|item| item.index.unwrap_or(0));
                return Ok(data
                    .into_iter()
                    .map(|item| crate::gemma2::normalise(item.embedding))
                    .collect());
            }
            if (status.as_u16() == 429 || status.is_server_error()) && attempt < RETRIES {
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
                continue;
            }
            bail!("{status}: {}", response.text().await.unwrap_or_default());
        }
        unreachable!()
    }
}

fn cache_path(model: &str, label: &str, texts: &[String]) -> PathBuf {
    let digest = texts.iter().fold(crate::data::stable_hash(model), |h, t| {
        h ^ crate::data::stable_hash(t).rotate_left(7)
    });
    PathBuf::from("target/embed-bench").join(format!("{}-{label}-{digest:016x}.f32", model.replace('/', "_")))
}

fn read_cache(path: &PathBuf, rows: usize) -> Option<Vec<Vec<f32>>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.read_to_end(&mut bytes).ok()?;
    let floats: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    let dims = floats.len().checked_div(rows)?;
    (dims > 0 && dims * rows == floats.len()).then(|| floats.chunks(dims).map(<[f32]>::to_vec).collect())
}

fn write_cache(path: &PathBuf, vecs: &[Vec<f32>]) -> anyhow::Result<()> {
    std::fs::create_dir_all(path.parent().expect("cache path has a parent"))?;
    let bytes: Vec<u8> = vecs.iter().flatten().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(path, bytes)?;
    Ok(())
}
