//! Cutting text for display without breaking characters.

/// The largest index `<= at` on a character boundary.
pub(crate) fn floor_boundary(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// At most `max` bytes of `s`, and whether anything was cut.
pub(crate) fn clip(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        (s.to_string(), false)
    } else {
        (s[..floor_boundary(s, max)].to_string(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_splits_a_character() {
        assert_eq!(clip("déjà", 2), ("d".to_string(), true));
        assert_eq!(clip("abc", 10), ("abc".to_string(), false));
    }
}
