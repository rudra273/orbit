//! Finding a model's quote in the source, byte for byte.

use regex::Regex;

/// Finds `quote` in `content`, tolerating only differences in whitespace.
/// Returns the byte span of the first occurrence.
pub fn locate(content: &str, quote: &str) -> Option<(usize, usize)> {
    let words: Vec<String> = quote.split_whitespace().map(regex::escape).collect();
    if words.is_empty() {
        return None;
    }
    let pattern = Regex::new(&words.join(r"\s+")).ok()?;
    pattern.find(content).map(|m| (m.start(), m.end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "I prefer Rust\nfor the core   Orbit engine.";

    #[test]
    fn finds_a_quote_across_line_breaks_and_spacing() {
        let (s, e) = locate(TEXT, "Rust for the core Orbit").unwrap();
        assert_eq!(&TEXT[s..e], "Rust\nfor the core   Orbit");
    }

    #[test]
    fn does_not_accept_a_paraphrase() {
        assert_eq!(locate(TEXT, "I like Rust"), None);
    }

    #[test]
    fn treats_regex_characters_literally() {
        assert_eq!(locate("cost is $5 (approx.)", "$5 (approx.)"), Some((8, 20)));
    }
}
