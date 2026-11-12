//! Credentials are not memories: they are removed before anything is stored.
//!
//! This is a deliberately small first detector (well-known key prefixes and
//! private-key blocks). It errs toward redacting too much.

pub const PLACEHOLDER: &str = "⟦secret:redacted⟧";

const KEY_PREFIXES: &[&str] = &["sk-", "ghp_", "gho_", "github_pat_", "AKIA", "xoxb-", "xoxp-"];
const MIN_SECRET_TAIL: usize = 16;

/// Returns the text with credentials replaced, and how many were found.
pub fn redact_secrets(text: &str) -> (String, usize) {
    let (text, mut found) = redact_private_keys(text);

    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            token.push(c);
        } else {
            found += flush(&mut token, &mut out);
            out.push(c);
        }
    }
    found += flush(&mut token, &mut out);
    (out, found)
}

fn flush(token: &mut String, out: &mut String) -> usize {
    let secret = KEY_PREFIXES
        .iter()
        .any(|p| token.starts_with(p) && token.len() >= p.len() + MIN_SECRET_TAIL);
    out.push_str(if secret { PLACEHOLDER } else { token });
    token.clear();
    secret as usize
}

fn redact_private_keys(text: &str) -> (String, usize) {
    const BEGIN: &str = "-----BEGIN ";
    const KEY_END: &str = "PRIVATE KEY-----";

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut found = 0;
    while let Some(start) = rest.find(BEGIN) {
        let block = &rest[start..];
        if !block.contains(KEY_END) {
            break;
        }
        let end = block
            .find("-----END ")
            .and_then(|e| block[e..].find(KEY_END).map(|k| e + k + KEY_END.len()))
            .unwrap_or(block.len());
        out.push_str(&rest[..start]);
        out.push_str(PLACEHOLDER);
        found += 1;
        rest = &block[end..];
    }
    out.push_str(rest);
    (out, found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_an_api_key_and_keeps_the_rest() {
        let (out, n) = redact_secrets("my key is sk-abcdefghijklmnopqrstuv, keep it safe");
        assert_eq!(n, 1);
        assert_eq!(out, format!("my key is {PLACEHOLDER}, keep it safe"));
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        let text = "ask-me-anything about sk- prefixes and `cargo test -p orbit-core`";
        assert_eq!(redact_secrets(text), (text.to_string(), 0));
    }

    #[test]
    fn replaces_a_private_key_block() {
        let text = "before\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----\nafter";
        let (out, n) = redact_secrets(text);
        assert_eq!(n, 1);
        assert_eq!(out, format!("before\n{PLACEHOLDER}\nafter"));
    }
}
