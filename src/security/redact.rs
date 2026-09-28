//! Centralized secret redaction for errors, logs, and subprocess output.
//!
//! Every error path routes through these
//! helpers so credentials, tokens, and signing keys never reach MCP clients.

use std::sync::LazyLock;

use regex::Regex;

static TOKEN_PATTERNS: LazyLock<[Regex; 4]> = LazyLock::new(|| {
    [
        Regex::new(r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}\b")
            .expect("valid github token pattern"),
        Regex::new(r"\bglpat-[A-Za-z0-9_-]{20,}\b").expect("valid gitlab token pattern"),
        Regex::new(r"(?i)\b(?:Bearer|token|auth)\s+[A-Za-z0-9._~+/=-]{8,}\b")
            .expect("valid bearer token pattern"),
        Regex::new(r"\b[A-Za-z0-9_-]{40,}\b").expect("valid opaque secret pattern"),
    ]
});

static SENSITIVE_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(token|secret|key|password|credential|auth)").expect("valid key pattern")
});

/// Masks userinfo (`user[:pass]`) embedded in URLs, e.g.
/// `https://user:secret@host/path` becomes `https://***@host/path`.
///
/// Implemented as a linear string scan rather than a regex so the redaction
/// cannot backtrack on hostile input.
#[must_use]
pub fn redact_url(input: &str) -> String {
    let mut out = input.to_owned();
    let mut cursor = 0_usize;

    while cursor < out.len() {
        let Some(at_offset) = out[cursor..].find('@') else {
            break;
        };
        let at = cursor + at_offset;

        match out[..at].rfind("://") {
            Some(scheme) if !authority_contains_separator(&out[scheme + 3..at]) => {
                out = format!("{}***@{}", &out[..scheme + 3], &out[at + 1..]);
                // Advance past the `***@` just inserted.
                cursor = scheme + 7;
            }
            _ => cursor = at + 1,
        }
    }

    out
}

/// True when the authority slice contains whitespace or `/`, meaning the `@`
/// does not introduce a credential-bearing authority.
fn authority_contains_separator(authority: &str) -> bool {
    authority
        .chars()
        .any(|c| c.is_whitespace() || c == '/' || c == '@')
}

/// Masks bearer tokens, API keys, and long opaque secrets.
#[must_use]
pub fn redact_token(input: &str) -> String {
    TOKEN_PATTERNS
        .iter()
        .fold(input.to_owned(), |acc, pattern| {
            pattern.replace_all(&acc, "***").into_owned()
        })
}

/// Masks sensitive git config values (signing keys, tokens, credentials).
#[must_use]
pub fn redact_config_value(key: &str, value: &str) -> String {
    if SENSITIVE_KEY_PATTERN.is_match(key) {
        "***".to_owned()
    } else {
        value.to_owned()
    }
}

/// Normalizes an error message through every redactor.
#[must_use]
pub fn redact_error(message: &str) -> String {
    redact_token(&redact_url(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_credentials_in_urls() {
        assert_eq!(
            redact_url("https://user:pass@github.com/a/b"),
            "https://***@github.com/a/b"
        );
        assert_eq!(redact_url("x https://a:b@h/p y"), "x https://***@h/p y");
    }

    #[test]
    fn leaves_credential_free_urls_unchanged() {
        assert_eq!(
            redact_url("https://github.com/a/b"),
            "https://github.com/a/b"
        );
        assert_eq!(redact_url("no url here"), "no url here");
        assert_eq!(redact_url("git@github.com:a/b"), "git@github.com:a/b");
    }

    #[test]
    fn masks_bearer_and_opaque_tokens() {
        assert!(redact_token("Bearer abcdefgh12345678").contains("***"));
        assert!(redact_token("token xyzabc1234567890").contains("***"));
        // A complete GitHub-style token is replaced wholesale.
        assert_eq!(redact_token("ghp_123456789012345678901234567890"), "***");
    }

    #[test]
    fn masks_sensitive_config_keys() {
        assert_eq!(redact_config_value("token", "abc"), "***");
        assert_eq!(redact_config_value("user.name", "daniel"), "daniel");
    }

    #[test]
    fn redacts_secrets_in_error_messages() {
        assert!(redact_error("https://u:secret@h failed").contains("***"));
    }
}
