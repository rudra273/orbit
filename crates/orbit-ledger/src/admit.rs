//! Stage 1, admission: validate, redact secrets, label. Pure — no database.

use crate::secrets::redact_secrets;
use chrono::{DateTime, Utc};
use orbit_core::{AuthorRole, Integrity, Sensitivity, sha256};
use serde::{Deserialize, Serialize};

/// Largest single piece of evidence accepted inline. Bigger bodies will go to object storage.
pub const MAX_CONTENT_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IncomingEvent {
    /// Position within its source (turn number, chunk number).
    pub seq: i32,
    pub author_role: AuthorRole,
    #[serde(default)]
    pub author_ref: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub content: String,
    /// Chosen by the sender; re-sending the same key is harmless.
    pub idempotency_key: String,
    /// Trust in the channel this arrived through. Connectors set it; it is never raised later.
    #[serde(default = "default_integrity")]
    pub integrity: Integrity,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: Sensitivity,
}

fn default_integrity() -> Integrity {
    Integrity::UserRelayed
}

fn default_sensitivity() -> Sensitivity {
    Sensitivity::Work
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AdmissionError {
    #[error("idempotency_key is empty")]
    MissingIdempotencyKey,
    #[error("content is empty")]
    EmptyContent,
    #[error("content is {0} bytes; the limit is {MAX_CONTENT_BYTES}")]
    TooLarge(usize),
}

/// An event that passed admission and is ready to append.
#[derive(Clone, Debug)]
pub struct Admitted {
    pub seq: i32,
    pub author_role: AuthorRole,
    pub author_ref: Option<String>,
    pub occurred_at: DateTime<Utc>,
    /// Exact bytes that will be stored (after secret redaction).
    pub content: Vec<u8>,
    pub content_sha256: [u8; 32],
    pub idempotency_key: String,
    pub integrity: Integrity,
    pub sensitivity: Sensitivity,
    pub taint: Vec<String>,
}

pub fn admit(event: IncomingEvent) -> Result<Admitted, AdmissionError> {
    if event.idempotency_key.trim().is_empty() {
        return Err(AdmissionError::MissingIdempotencyKey);
    }
    if event.content.is_empty() {
        return Err(AdmissionError::EmptyContent);
    }
    if event.content.len() > MAX_CONTENT_BYTES {
        return Err(AdmissionError::TooLarge(event.content.len()));
    }

    let (content, secrets) = redact_secrets(&event.content);
    let taint = if secrets > 0 {
        vec!["secret_redacted".to_string()]
    } else {
        Vec::new()
    };
    let content = content.into_bytes();

    Ok(Admitted {
        seq: event.seq,
        author_role: event.author_role,
        author_ref: event.author_ref,
        occurred_at: event.occurred_at,
        content_sha256: sha256(&content),
        content,
        idempotency_key: event.idempotency_key,
        integrity: event.integrity,
        sensitivity: event.sensitivity,
        taint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(content: &str) -> IncomingEvent {
        IncomingEvent {
            seq: 0,
            author_role: AuthorRole::User,
            author_ref: None,
            occurred_at: Utc::now(),
            content: content.to_string(),
            idempotency_key: "k1".to_string(),
            integrity: Integrity::UserDirect,
            sensitivity: Sensitivity::Work,
        }
    }

    #[test]
    fn keeps_ordinary_content_byte_for_byte() {
        let text = "I prefer Rust for the core Orbit engine";
        let admitted = admit(event(text)).unwrap();
        assert_eq!(admitted.content, text.as_bytes());
        assert!(admitted.taint.is_empty());
    }

    #[test]
    fn redacts_a_secret_and_marks_the_evidence() {
        let admitted = admit(event("token: ghp_abcdefghijklmnopqrstuvwx")).unwrap();
        assert!(!String::from_utf8(admitted.content).unwrap().contains("ghp_"));
        assert_eq!(admitted.taint, ["secret_redacted"]);
    }

    #[test]
    fn rejects_an_event_without_an_idempotency_key() {
        let mut e = event("hello");
        e.idempotency_key = " ".to_string();
        assert_eq!(admit(e).unwrap_err(), AdmissionError::MissingIdempotencyKey);
    }
}
