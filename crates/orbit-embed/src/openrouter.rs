//! Hosted embeddings through OpenRouter's OpenAI-compatible embeddings API.
//!
//! Latency, measured from the owner's laptop on 2026-11-13: one query embedding took 1.1–4.6 s,
//! almost all of it OpenRouter and provider time, not the network. So documents go to the first
//! configured provider (one provider keeps a space consistent), while a query is sent to every
//! configured provider at once and the first answer wins: that cut the median from 1.43 s to
//! 1.16 s and the worst case from 3.99 s to 1.73 s. The providers' vectors for the same text agree
//! to a cosine of at least 0.998, the same order as the variation already in the benchmark.

use crate::{EmbedError, EmbedFuture, Embedder, Role, Space, shorten};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;

const URL: &str = "https://openrouter.ai/api/v1/embeddings";
/// Inputs per request.
const BATCH: usize = 64;
/// Retries for document batches on dropped connections, 429 and 5xx answers; waits 1, 2, 4, 8, 16 s.
const RETRIES: u32 = 5;
/// A query is waited on this long at most; then search goes on by words only.
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

/// An OpenRouter client. Providers are asked not to collect or train on the data
/// (`provider.data_collection = deny`), and only providers that agree are used.
pub struct OpenRouter {
    http: reqwest::Client,
    key: Arc<str>,
    /// OpenRouter provider slugs, such as `deepinfra`; empty lets OpenRouter choose.
    providers: Vec<String>,
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

impl OpenRouter {
    /// `providers` in order of preference: documents use the first that is up, queries race all.
    pub fn new(key: String, providers: Vec<String>) -> Result<Self, EmbedError> {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(90)).build()?;
        Ok(Self {
            http,
            key: key.into(),
            providers,
        })
    }

    /// Routing for one request: `providers` only, in that order, or OpenRouter's choice when empty.
    fn body(space: &Space, inputs: &[String], providers: &[String]) -> serde_json::Value {
        let mut provider = serde_json::json!({ "data_collection": "deny" });
        if !providers.is_empty() {
            provider["order"] = serde_json::json!(providers);
            provider["allow_fallbacks"] = serde_json::json!(false);
        }
        serde_json::json!({
            "model": space.model,
            "input": inputs,
            "encoding_format": "float",
            "provider": provider,
        })
    }

    /// One query, sent to each provider at once; the first good answer wins.
    async fn race(&self, space: &Space, input: String) -> Result<Vec<Vec<f32>>, EmbedError> {
        let routes: Vec<Vec<String>> = if self.providers.is_empty() {
            vec![Vec::new()]
        } else {
            self.providers.iter().map(|p| vec![p.clone()]).collect()
        };
        let mut set = JoinSet::new();
        for route in routes {
            let (http, key, space) = (self.http.clone(), self.key.clone(), space.clone());
            let body = Self::body(&space, std::slice::from_ref(&input), &route);
            set.spawn(async move {
                let attempt = send(&http, &key, &space, &body, 1, 0);
                tokio::time::timeout(QUERY_TIMEOUT, attempt)
                    .await
                    .unwrap_or(Err(EmbedError::Timeout))
            });
        }
        let mut last = EmbedError::Timeout;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(vectors)) => return Ok(vectors),
                Ok(Err(error)) => last = error,
                Err(_) => {}
            }
        }
        Err(last)
    }
}

/// Sends one request, retrying dropped connections, 429 and 5xx up to `retries` times.
async fn send(
    http: &reqwest::Client,
    key: &str,
    space: &Space,
    body: &serde_json::Value,
    inputs: usize,
    retries: u32,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let mut attempt = 0;
    loop {
        let retry = match http.post(URL).bearer_auth(key).json(body).send().await {
            Ok(response) if response.status().is_success() => {
                let mut data = response.json::<Response>().await?.data;
                if data.len() != inputs {
                    return Err(EmbedError::Count {
                        asked: inputs,
                        got: data.len(),
                    });
                }
                data.sort_by_key(|item| item.index.unwrap_or(0));
                return data.into_iter().map(|item| fit(space, &item.embedding)).collect();
            }
            Ok(response) => {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                if status != 429 && status < 500 || attempt >= retries {
                    return Err(EmbedError::Provider { status, body });
                }
                format!("status {status}")
            }
            Err(error) if attempt < retries => error.to_string(),
            Err(error) => return Err(error.into()),
        };
        tracing::warn!(model = %space.model, attempt, reason = %retry, "embedding request failed; retrying");
        tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
        attempt += 1;
    }
}

/// Cuts a returned vector to the space's size, and rejects one that is too short.
fn fit(space: &Space, v: &[f32]) -> Result<Vec<f32>, EmbedError> {
    if v.len() < space.dims {
        return Err(EmbedError::Dimensions {
            space: space.id.clone(),
            needs: space.dims,
            got: v.len(),
        });
    }
    Ok(shorten(v, space.dims))
}

impl Embedder for OpenRouter {
    fn embed<'a>(&'a self, space: &'a Space, role: Role, texts: &'a [String]) -> EmbedFuture<'a> {
        Box::pin(async move {
            if role == Role::Query && texts.len() == 1 {
                return self.race(space, space.prepare(role, &texts[0])).await;
            }
            let mut out = Vec::with_capacity(texts.len());
            for batch in texts.chunks(BATCH) {
                let inputs: Vec<String> = batch.iter().map(|t| space.prepare(role, t)).collect();
                let body = Self::body(space, &inputs, &self.providers);
                out.extend(send(&self.http, &self.key, space, &body, inputs.len(), RETRIES).await?);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vector_shorter_than_the_space_is_rejected() {
        let space = Space::hosted("m", 4);
        assert!(matches!(fit(&space, &[1.0, 0.0]), Err(EmbedError::Dimensions { .. })));
        assert_eq!(
            fit(&space, &[0.0, 0.0, 3.0, 4.0, 9.0]).unwrap(),
            vec![0.0, 0.0, 0.6, 0.8]
        );
    }
}
