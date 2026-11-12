//! Embeddings: turning text into vectors for the dense recall channel.
//!
//! Nothing outside this crate names a model or a provider ([14 §1](../../../docs/system-design/14-tech-stack.md)).
//! A [`Space`] fixes everything that makes two vectors comparable — model, size and the prefixes
//! put in front of queries and documents — and an [`Embedder`] turns text into vectors of a space.

mod openrouter;
mod space;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use openrouter::OpenRouter;
pub use space::Space;

use std::{future::Future, pin::Pin};

/// Whether text is something stored (a document) or something searched for (a query).
/// Retrieval models are trained with different prefixes for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Document,
    Query,
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("embedding request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("embedding provider answered {status}: {body}")]
    Provider { status: u16, body: String },
    #[error("embedding provider returned {got} vectors for {asked} inputs")]
    Count { asked: usize, got: usize },
    #[error("model returned {got} dimensions; space {space} needs at least {needs}")]
    Dimensions { space: String, needs: usize, got: usize },
}

pub type EmbedFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>, EmbedError>> + Send + 'a>>;

/// Turns text into unit-length vectors of a space, one per input, in order.
pub trait Embedder: Send + Sync {
    fn embed<'a>(&'a self, space: &'a Space, role: Role, texts: &'a [String]) -> EmbedFuture<'a>;
}

/// The first `dims` numbers of `v`, scaled to unit length. Matryoshka-trained models keep most of
/// their quality when cut this way; for any other model `dims` should be the full size.
pub fn shorten(v: &[f32], dims: usize) -> Vec<f32> {
    let mut out = v[..dims.min(v.len())].to_vec();
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        out.iter_mut().for_each(|x| *x /= norm);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortened_vectors_have_unit_length() {
        let v = shorten(&[3.0, 4.0, 12.0], 2);
        assert_eq!(v, vec![0.6, 0.8]);
    }
}
