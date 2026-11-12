//! Offline embedders for tests. Not for real use.

use crate::{EmbedError, EmbedFuture, Embedder, Role, Space, shorten};

/// Hashes each lower-cased word into one of the space's dimensions, so texts sharing words get
/// similar vectors. Deterministic and free; it knows nothing about meaning.
pub struct WordHash;

impl WordHash {
    pub fn vector(dims: usize, text: &str) -> Vec<f32> {
        let mut v = vec![0.0; dims];
        for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
            let hash = word.to_lowercase().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            });
            v[(hash % dims as u64) as usize] += 1.0;
        }
        shorten(&v, dims)
    }
}

impl Embedder for WordHash {
    fn embed<'a>(&'a self, space: &'a Space, _role: Role, texts: &'a [String]) -> EmbedFuture<'a> {
        Box::pin(async move { Ok(texts.iter().map(|t| Self::vector(space.dims, t)).collect()) })
    }
}

/// Always fails, as a provider that is down would.
pub struct Down;

impl Embedder for Down {
    fn embed<'a>(&'a self, _space: &'a Space, _role: Role, _texts: &'a [String]) -> EmbedFuture<'a> {
        Box::pin(async {
            Err(EmbedError::Provider {
                status: 503,
                body: "down".into(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shared_words_mean_similar_vectors() {
        let space = Space::hosted("test", 64);
        let texts = [
            "deploy staging".to_string(),
            "Deploy the staging box".to_string(),
            "lunch".to_string(),
        ];
        let v = WordHash.embed(&space, Role::Document, &texts).await.unwrap();
        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        assert!(dot(&v[0], &v[1]) > dot(&v[0], &v[2]));
    }
}
