//! Stage 3, index raw: make new evidence searchable.
//!
//! Two independent consumers of the change feed, each with its own cursor, so a slow or
//! unavailable embedding provider never holds back lexical search:
//! - [`RawIndexer`]: full-text vectors and literals ([`raw`]).
//! - [`DenseIndexer`]: embeddings of conversation text, per embedding space ([`dense`]).
//!
//! See `docs/architecture/06-memory-formation.md` section 2.

pub mod chunk;
pub mod dense;
pub mod raw;

pub use dense::{DenseIndexer, SpaceState};
pub use raw::{RawIndexer, kind_name, normalise};

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Embed(#[from] orbit_embed::EmbedError),
    #[error("embedding space {0} already exists with different settings; a space never changes")]
    SpaceChanged(String),
}
