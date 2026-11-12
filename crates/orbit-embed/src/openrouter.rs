//! Hosted embeddings through OpenRouter's OpenAI-compatible embeddings API.

use crate::{EmbedError, EmbedFuture, Embedder, Role, Space, shorten};
use serde::Deserialize;
use std::time::Duration;

const URL: &str = "https://openrouter.ai/api/v1/embeddings";
/// Inputs per request.
const BATCH: usize = 64;
/// Retries for dropped connections, 429 and 5xx answers; waits 1, 2, 4, 8, 16 s.
const RETRIES: u32 = 5;

/// An OpenRouter client. Providers are asked not to collect or train on the data
/// (`provider.data_collection = deny`), and only providers that agree are used.
pub struct OpenRouter {
    http: reqwest::Client,
    key: String,
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
    pub fn new(key: String) -> Result<Self, EmbedError> {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(90)).build()?;
        Ok(Self { http, key })
    }

    async fn request(&self, space: &Space, inputs: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let body = serde_json::json!({
            "model": space.model,
            "input": inputs,
            "encoding_format": "float",
            "provider": { "data_collection": "deny" },
        });
        let mut attempt = 0;
        loop {
            let retry = match self.http.post(URL).bearer_auth(&self.key).json(&body).send().await {
                Ok(response) if response.status().is_success() => {
                    let mut data = response.json::<Response>().await?.data;
                    if data.len() != inputs.len() {
                        return Err(EmbedError::Count {
                            asked: inputs.len(),
                            got: data.len(),
                        });
                    }
                    data.sort_by_key(|item| item.index.unwrap_or(0));
                    return data.into_iter().map(|item| fit(space, &item.embedding)).collect();
                }
                Ok(response) => {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    if status != 429 && status < 500 || attempt == RETRIES {
                        return Err(EmbedError::Provider { status, body });
                    }
                    format!("status {status}")
                }
                Err(error) if attempt < RETRIES => error.to_string(),
                Err(error) => return Err(error.into()),
            };
            tracing::warn!(model = %space.model, attempt, reason = %retry, "embedding request failed; retrying");
            tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            attempt += 1;
        }
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
            let mut out = Vec::with_capacity(texts.len());
            for batch in texts.chunks(BATCH) {
                let inputs: Vec<String> = batch.iter().map(|t| space.prepare(role, t)).collect();
                out.extend(self.request(space, &inputs).await?);
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
