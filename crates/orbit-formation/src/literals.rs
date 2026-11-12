//! Literal detection: exact strings found by grammar, before any model runs.
//!
//! A literal's value is always a byte copy of its span and is never generated
//! (INV-3). Kinds are tried in priority order; a later kind never claims bytes
//! an earlier one took.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiteralKind {
    /// Contents of a fenced code block.
    Code,
    /// Contents of an inline code span.
    Command,
    Url,
    Path,
    Version,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Literal {
    pub kind: LiteralKind,
    pub exact: String,
    /// Byte offsets into the text the literal was found in.
    pub start: usize,
    pub end: usize,
}

static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)```[^\n]*\n(.*?)```").unwrap());
static INLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`\n]+)`").unwrap());
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s<>"'`)\]]*[^\s<>"'`)\].,;:!?]"#).unwrap());
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    // Anchored paths (`/x`, `~/x`, `./x`, `../x`), or relative ones whose last segment has an extension.
    Regex::new(r"(?:~|\.{1,2})?/[\w.-]+(?:/[\w.-]+)*|[\w-]+(?:/[\w.-]+)+\.[A-Za-z0-9]{1,8}\b").unwrap()
});
static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bv?\d+\.\d+(?:\.\d+)?(?:-[0-9A-Za-z]+(?:\.[0-9A-Za-z]+)*)?\b").unwrap());

/// Returns the literals in `text`, ordered by position.
pub fn detect_literals(text: &str) -> Vec<Literal> {
    let mut found: Vec<Literal> = Vec::new();
    let take = |kind: LiteralKind, start: usize, end: usize, found: &mut Vec<Literal>| {
        if start < end && !found.iter().any(|l| start < l.end && l.start < end) {
            found.push(Literal {
                kind,
                exact: text[start..end].to_string(),
                start,
                end,
            });
        }
    };

    for c in FENCE.captures_iter(text) {
        let m = c.get(1).unwrap();
        let body = m.as_str().trim_end_matches('\n');
        take(LiteralKind::Code, m.start(), m.start() + body.len(), &mut found);
    }
    // Fence markers themselves must not be read as inline code.
    let fenced: Vec<(usize, usize)> = FENCE.find_iter(text).map(|m| (m.start(), m.end())).collect();
    for c in INLINE.captures_iter(text) {
        let m = c.get(1).unwrap();
        if !fenced.iter().any(|&(s, e)| m.start() >= s && m.end() <= e) {
            take(LiteralKind::Command, m.start(), m.end(), &mut found);
        }
    }
    for m in URL.find_iter(text) {
        take(LiteralKind::Url, m.start(), m.end(), &mut found);
    }
    for m in PATH.find_iter(text) {
        if starts_word(text, m.start()) {
            take(LiteralKind::Path, m.start(), m.end(), &mut found);
        }
    }
    for m in VERSION.find_iter(text) {
        if !continues_as_number(text, m.start(), m.end()) {
            take(LiteralKind::Version, m.start(), m.end(), &mut found);
        }
    }

    found.sort_by_key(|l| l.start);
    found
}

/// A path must not begin in the middle of a word (`and/or` is not a path; `a/b.rs` is).
fn starts_word(text: &str, start: usize) -> bool {
    text[..start]
        .chars()
        .next_back()
        .is_none_or(|c| c.is_whitespace() || "([{\"'<,".contains(c))
}

/// Rejects matches that are part of a longer dotted number, such as an IP address.
fn continues_as_number(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let mut after = text[end..].chars();
    let dotted_after = after.next() == Some('.') && after.next().is_some_and(|c| c.is_ascii_digit());
    before == Some('.') || dotted_after
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(LiteralKind, &str)> {
        detect_literals(text)
            .into_iter()
            .map(|l| (l.kind, &text[l.start..l.end]))
            .collect()
    }

    #[test]
    fn every_literal_is_a_byte_copy_of_its_span() {
        let text = "Déjà vu: run `cargo test -p orbit-core` on PostgreSQL 18.1, see https://example.com/docs.";
        for lit in detect_literals(text) {
            assert_eq!(&text[lit.start..lit.end], lit.exact);
        }
    }

    #[test]
    fn finds_each_kind() {
        let text = "run `make db` with v0.8.1, edit docs/README.md and ~/.zshrc, see https://example.com/a.";
        assert_eq!(
            kinds(text),
            [
                (LiteralKind::Command, "make db"),
                (LiteralKind::Version, "v0.8.1"),
                (LiteralKind::Path, "docs/README.md"),
                (LiteralKind::Path, "~/.zshrc"),
                (LiteralKind::Url, "https://example.com/a"),
            ]
        );
    }

    #[test]
    fn a_fenced_block_is_one_literal() {
        let text = "Try:\n```bash\ncargo build\ncargo test\n```\ndone";
        assert_eq!(kinds(text), [(LiteralKind::Code, "cargo build\ncargo test")]);
    }

    #[test]
    fn nested_matches_are_reported_once() {
        assert_eq!(
            kinds("use `pip install foo==1.2.3`"),
            [(LiteralKind::Command, "pip install foo==1.2.3")]
        );
        assert_eq!(
            kinds("open https://x.dev/a/b.md now"),
            [(LiteralKind::Url, "https://x.dev/a/b.md")]
        );
    }

    #[test]
    fn ordinary_words_and_addresses_are_not_literals() {
        assert!(kinds("read and/or write, at 192.168.1.10").is_empty());
    }
}
