//! The retrieval contract. No engine yet: this crate fixes the interface that
//! every backend (PostgreSQL first, an in-process index later) must satisfy.
//!
//! See `docs/architecture/08-retrieval.md`.

use orbit_core::{Basis, EvidenceId, Integrity};
use serde::{Deserialize, Serialize};

/// What a recall returns alongside its items. `NotFound` is a real answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Grounded,
    Partial,
    Conflicting,
    NotFound,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecallRequest {
    pub query: String,
    pub limit: usize,
}

/// One piece of evidence returned verbatim, with where it came from.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Quote {
    pub evidence_id: EvidenceId,
    pub text: String,
    pub integrity: Integrity,
    pub basis: Basis,
    pub score: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecallResult {
    pub verdict: Verdict,
    pub items: Vec<Quote>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecallError {
    #[error("index unavailable: {0}")]
    Unavailable(String),
}

/// A searchable projection of one brain. Implementations must apply the
/// caller's policy mask *before* ranking (INV-7); the mask type arrives with grants.
pub trait RecallIndex {
    fn recall(&self, request: &RecallRequest) -> Result<RecallResult, RecallError>;
}
