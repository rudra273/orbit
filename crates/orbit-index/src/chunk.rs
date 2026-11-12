//! Splitting long text into pieces that are embedded one by one.
//!
//! One vector for a long reply blurs its topics together, so text is cut into chunks of at most
//! [`MAX_CHARS`] characters — the length measured on the owner's history (evals/embed-bench) —
//! preferring to cut between paragraphs, then lines, then sentences, then words.

use std::ops::Range;

/// Longest chunk, in characters.
pub const MAX_CHARS: usize = 1_600;

/// Byte ranges of the chunks of `text`, in order. Whitespace between chunks is left out, and a
/// text with nothing but whitespace has no chunks. The ranges are exact: `&text[range]`.
pub fn chunks(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        start += text[start..].len() - text[start..].trim_start().len();
        if start == text.len() {
            return out;
        }
        let rest = &text[start..];
        let window_end = rest.char_indices().nth(MAX_CHARS).map_or(rest.len(), |(i, _)| i);
        let end = if window_end == rest.len() {
            window_end
        } else {
            cut(&rest[..window_end])
        };
        let piece = rest[..end].trim_end();
        out.push(start..start + piece.len());
        start += end;
    }
}

/// Where to cut a full window: after the last good break in its second half, or at its end.
fn cut(window: &str) -> usize {
    let half = window.len() / 2;
    let late = |i: &usize| *i >= half;
    let after = |pattern: &str| window.rmatch_indices(pattern).map(|(i, m)| i + m.len()).find(late);
    after("\n\n")
        .or_else(|| after("\n"))
        .or_else(|| [". ", "? ", "! ", "; "].iter().filter_map(|p| after(p)).max())
        .or_else(|| after(" "))
        .unwrap_or(window.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(text: &str) -> Vec<&str> {
        chunks(text).into_iter().map(|r| &text[r]).collect()
    }

    #[test]
    fn short_text_is_one_chunk_without_surrounding_whitespace() {
        assert_eq!(pieces("  hello there \n"), vec!["hello there"]);
        assert!(pieces(" \n\t").is_empty());
    }

    #[test]
    fn long_text_is_cut_between_paragraphs() {
        let a = "a".repeat(1_000);
        let b = "b".repeat(1_000);
        assert_eq!(pieces(&format!("{a}\n\n{b}")), vec![a.as_str(), b.as_str()]);
    }

    #[test]
    fn falls_back_to_sentences_then_a_hard_cut() {
        let sentence = format!("{}. ", "word ".repeat(100).trim_end());
        let text = sentence.repeat(10);
        for piece in pieces(&text) {
            assert!(piece.chars().count() <= MAX_CHARS);
            assert!(piece.ends_with('.'));
        }
        let solid = "x".repeat(4_000);
        assert_eq!(
            pieces(&solid).iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![1_600, 1_600, 800]
        );
    }

    #[test]
    fn never_splits_a_character_and_covers_all_text() {
        let text = "é".repeat(3_333);
        let got = pieces(&text);
        assert_eq!(got.concat(), text);
        assert!(got.iter().all(|p| p.chars().count() <= MAX_CHARS));
    }
}
