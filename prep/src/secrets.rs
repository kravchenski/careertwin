//! Keeping keys out of logs, error messages and anything shown on screen.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A key or token. It prints as `***`; only [`Secret::expose`] gives the value back.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into().trim().to_owned())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_empty() { "Secret(empty)" } else { "Secret(***)" })
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

const MASK: &str = "***";
const MIN_SECRET: usize = 6;

static LOOKS_LIKE_A_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?ix)
        (?: \b(?:sk|gsk|xai|pk|rk|hf|ghp|gho|github_pat)[-_][A-Za-z0-9_\-]{8,} )
      | (?: \b(?:bearer|basic|token)\s+[A-Za-z0-9._~+/=\-]{8,} )
      | (?: \b(?:api[_-]?key|xi-api-key|secret|password|passwd|authorization|token)["']?\s*[:=]\s*["']?[^\s"',;)]{8,} )
        "#,
    )
    .expect("the key pattern is valid")
});

/// `text` with every known secret and everything shaped like a key replaced by `***`.
pub fn redact(text: &str, known: &[&str]) -> String {
    let mut out = text.to_owned();
    for secret in known {
        if secret.len() >= MIN_SECRET {
            out = out.replace(secret, MASK);
        }
    }
    LOOKS_LIKE_A_KEY.replace_all(&out, MASK).into_owned()
}

/// Whether `text` holds one of the secrets (used to refuse sending it anywhere).
pub fn contains_secret(text: &str, known: &[&str]) -> bool {
    known.iter().any(|secret| secret.len() >= MIN_SECRET && text.contains(secret)) || LOOKS_LIKE_A_KEY.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_never_show_the_value() {
        let secret = Secret::new("sk-live-1234567890");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(format!("{secret}"), "***");
        assert_eq!(secret.expose(), "sk-live-1234567890");
        assert_eq!(format!("{:?}", Secret::default()), "Secret(empty)");
    }

    #[test]
    fn a_struct_holding_a_secret_does_not_leak_through_debug() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Holder {
            key: Secret,
        }
        let text = format!("{:?}", Holder { key: Secret::new("topsecretvalue99") });
        assert!(!text.contains("topsecretvalue99"), "{text}");
    }

    #[test]
    fn known_secrets_are_masked_wherever_they_appear() {
        let text = "failed: https://x.test/?k=abcdef123456 and again abcdef123456";
        let out = redact(text, &["abcdef123456"]);
        assert!(!out.contains("abcdef123456"), "{out}");
        assert_eq!(out.matches("***").count(), 2);
    }

    #[test]
    fn short_known_values_are_left_alone() {
        assert_eq!(redact("a b c", &["a"]), "a b c");
    }

    #[test]
    fn things_shaped_like_keys_are_masked_even_when_unknown() {
        for sample in [
            "Authorization: Bearer abcdefghijklmnop",
            "key sk-proj-abcdefghijklmnop1234",
            "gsk_abcdefghijklmnop1234 was rejected",
            r#"{"api_key": "abcdefghijkl"}"#,
            "xi-api-key: abcdefghijkl",
            "password=hunter2hunter2",
        ] {
            let out = redact(sample, &[]);
            assert!(out.contains("***"), "{sample} -> {out}");
            assert!(!out.contains("abcdefghijkl") && !out.contains("hunter2hunter2"), "{sample} -> {out}");
        }
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let text = "The token bucket algorithm limits a request rate, and a password manager helps.";
        assert_eq!(redact(text, &[]), text);
    }

    #[test]
    fn detects_a_secret_before_sending() {
        assert!(contains_secret("my key is sk-abcdefghijkl", &[]));
        assert!(contains_secret("value zzzzzz-known", &["zzzzzz-known"]));
        assert!(!contains_secret("nothing here", &["zzzzzz-known"]));
    }

    #[test]
    fn redaction_is_fast_on_hostile_input() {
        let hostile = "token ".repeat(20_000) + &"a".repeat(50_000);
        let started = std::time::Instant::now();
        let _ = redact(&hostile, &[]);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
