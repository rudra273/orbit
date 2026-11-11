//! An anchor is a pointer to exact bytes in a piece of evidence (INV-2).

use crate::ids::EvidenceId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AnchorError {
    #[error("span {start}..{end} is outside content of length {len}")]
    OutOfBounds { start: u32, end: u32, len: usize },
    #[error("span is empty")]
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub evidence_id: EvidenceId,
    pub byte_start: u32,
    pub byte_end: u32,
    pub span_sha256: [u8; 32],
}

impl Anchor {
    pub fn new(evidence_id: EvidenceId, content: &[u8], byte_start: u32, byte_end: u32) -> Result<Self, AnchorError> {
        if byte_start >= byte_end {
            return Err(AnchorError::Empty);
        }
        let span = content.get(byte_start as usize..byte_end as usize).ok_or(AnchorError::OutOfBounds {
            start: byte_start,
            end: byte_end,
            len: content.len(),
        })?;
        Ok(Self { evidence_id, byte_start, byte_end, span_sha256: sha256(span) })
    }

    /// The anchored bytes, if the content still matches the recorded hash.
    pub fn resolve<'a>(&self, content: &'a [u8]) -> Option<&'a [u8]> {
        let span = content.get(self.byte_start as usize..self.byte_end as usize)?;
        (sha256(span) == self.span_sha256).then_some(span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &[u8] = b"I prefer Rust for the core Orbit engine";

    #[test]
    fn resolves_to_the_exact_bytes() {
        let a = Anchor::new(EvidenceId::generate(), TEXT, 9, 13).unwrap();
        assert_eq!(a.resolve(TEXT), Some(&b"Rust"[..]));
    }

    #[test]
    fn does_not_resolve_against_changed_content() {
        let a = Anchor::new(EvidenceId::generate(), TEXT, 9, 13).unwrap();
        assert_eq!(a.resolve(b"I prefer Ruby for the core Orbit engine"), None);
    }

    #[test]
    fn rejects_a_span_past_the_end() {
        let err = Anchor::new(EvidenceId::generate(), TEXT, 30, 99).unwrap_err();
        assert!(matches!(err, AnchorError::OutOfBounds { .. }));
    }
}
