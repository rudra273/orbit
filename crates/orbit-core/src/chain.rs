//! The per-brain hash chain over evidence rows (INV-1: tamper evidence).

use sha2::{Digest, Sha256};

/// Hash of one ledger row, bound to the row before it.
pub fn row_hash(prev: Option<&[u8; 32]>, content_sha256: &[u8; 32], idempotency_key: &str, occurred_at_micros: i64) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prev.map_or(&[0u8; 32][..], |p| &p[..]));
    h.update(content_sha256);
    h.update((idempotency_key.len() as u64).to_be_bytes());
    h.update(idempotency_key.as_bytes());
    h.update(occurred_at_micros.to_be_bytes());
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256;

    #[test]
    fn changing_an_earlier_row_changes_every_later_hash() {
        let a = row_hash(None, &sha256(b"one"), "k1", 1);
        let b = row_hash(Some(&a), &sha256(b"two"), "k2", 2);

        let a_tampered = row_hash(None, &sha256(b"ONE"), "k1", 1);
        let b_after_tamper = row_hash(Some(&a_tampered), &sha256(b"two"), "k2", 2);

        assert_ne!(b, b_after_tamper);
    }
}
