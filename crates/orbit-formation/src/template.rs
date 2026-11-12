//! Handle-locked templates: derived text refers to exact strings by handle.
//!
//! `Prefers ⟦lt:k1⟧ for the ⟦en:orbit⟧ engine`. A model may emit handles; it may
//! never type an exact string itself (INV-3). See section 5.2 of the formation doc.

use crate::literals::detect_literals;
use serde::{Deserialize, Serialize};

const OPEN: char = '⟦';
const CLOSE: char = '⟧';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HandleKind {
    /// `lt:` an exact string.
    Literal,
    /// `en:` an entity, rendered as its name.
    Entity,
    /// `bl:` a belief, rendered as a citation.
    Belief,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handle {
    pub kind: HandleKind,
    pub key: String,
    /// Byte span of the whole handle, brackets included.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("unterminated handle at byte {0}")]
    Unterminated(usize),
    #[error("malformed handle `{0}`")]
    Malformed(String),
    #[error("no value for handle `{0}`")]
    Unresolved(String),
}

/// Why a template may not be stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A handle the model was not given.
    UnknownHandle(String),
    /// An exact-looking string typed outside a handle.
    LiteralOutsideHandle(String),
    /// A digit typed outside a handle; numbers must come from the source.
    DigitOutsideHandle,
}

pub fn parse_handles(template: &str) -> Result<Vec<Handle>, TemplateError> {
    let mut handles = Vec::new();
    let mut rest = 0;
    while let Some(offset) = template[rest..].find(OPEN) {
        let start = rest + offset;
        let body_start = start + OPEN.len_utf8();
        let close = template[body_start..]
            .find(CLOSE)
            .ok_or(TemplateError::Unterminated(start))?;
        let body = &template[body_start..body_start + close];
        let end = body_start + close + CLOSE.len_utf8();

        let (prefix, key) = body
            .split_once(':')
            .ok_or_else(|| TemplateError::Malformed(body.to_string()))?;
        let kind = match prefix {
            "lt" => HandleKind::Literal,
            "en" => HandleKind::Entity,
            "bl" => HandleKind::Belief,
            _ => return Err(TemplateError::Malformed(body.to_string())),
        };
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(TemplateError::Malformed(body.to_string()));
        }
        handles.push(Handle {
            kind,
            key: key.to_string(),
            start,
            end,
        });
        rest = end;
    }
    Ok(handles)
}

/// Replaces every handle with the value `resolve` gives it.
pub fn render(template: &str, resolve: impl Fn(&Handle) -> Option<String>) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut last = 0;
    for h in parse_handles(template)? {
        out.push_str(&template[last..h.start]);
        out.push_str(&resolve(&h).ok_or_else(|| TemplateError::Unresolved(h.key.clone()))?);
        last = h.end;
    }
    out.push_str(&template[last..]);
    Ok(out)
}

/// Checks a template a model produced. `known` says whether a handle was in the
/// table the model was shown.
pub fn validate(template: &str, known: impl Fn(&Handle) -> bool) -> Result<(), Vec<Violation>> {
    let handles = parse_handles(template).map_err(|_| vec![Violation::UnknownHandle(template.to_string())])?;
    let mut violations: Vec<Violation> = handles
        .iter()
        .filter(|h| !known(h))
        .map(|h| Violation::UnknownHandle(h.key.clone()))
        .collect();

    let mut outside = String::with_capacity(template.len());
    let mut last = 0;
    for h in &handles {
        outside.push_str(&template[last..h.start]);
        outside.push(' ');
        last = h.end;
    }
    outside.push_str(&template[last..]);

    violations.extend(
        detect_literals(&outside)
            .into_iter()
            .map(|l| Violation::LiteralOutsideHandle(l.exact)),
    );
    if outside.chars().any(|c| c.is_ascii_digit()) {
        violations.push(Violation::DigitOutsideHandle);
    }

    if violations.is_empty() { Ok(()) } else { Err(violations) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TPL: &str = "Prefers ⟦lt:k1⟧ for the ⟦en:orbit⟧ engine ⟦bl:b7⟧";

    #[test]
    fn parses_each_kind_of_handle() {
        let kinds: Vec<_> = parse_handles(TPL)
            .unwrap()
            .into_iter()
            .map(|h| (h.kind, h.key))
            .collect();
        assert_eq!(
            kinds,
            [
                (HandleKind::Literal, "k1".to_string()),
                (HandleKind::Entity, "orbit".to_string()),
                (HandleKind::Belief, "b7".to_string())
            ]
        );
    }

    #[test]
    fn renders_by_substituting_values() {
        let out = render(TPL, |h| match h.kind {
            HandleKind::Literal => Some("Rust".into()),
            HandleKind::Entity => Some("Orbit".into()),
            HandleKind::Belief => Some("[bl_7]".into()),
        })
        .unwrap();
        assert_eq!(out, "Prefers Rust for the Orbit engine [bl_7]");
    }

    #[test]
    fn accepts_a_template_that_only_uses_known_handles() {
        assert_eq!(validate(TPL, |_| true), Ok(()));
    }

    #[test]
    fn rejects_a_handle_the_model_was_not_given() {
        let err = validate(TPL, |h| h.key != "k1").unwrap_err();
        assert_eq!(err, [Violation::UnknownHandle("k1".into())]);
    }

    #[test]
    fn rejects_exact_strings_typed_by_the_model() {
        let err = validate("Run `cargo test` on version ⟦lt:k1⟧", |_| true).unwrap_err();
        assert_eq!(err, [Violation::LiteralOutsideHandle("cargo test".into())]);
        let err = validate("The limit is 200 per minute", |_| true).unwrap_err();
        assert_eq!(err, [Violation::DigitOutsideHandle]);
    }

    #[test]
    fn reports_a_malformed_handle() {
        assert_eq!(parse_handles("x ⟦lt:k1"), Err(TemplateError::Unterminated(2)));
        assert!(matches!(parse_handles("⟦zz:k⟧"), Err(TemplateError::Malformed(_))));
    }
}
