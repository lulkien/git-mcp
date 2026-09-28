//! Centralized argument/ref/path validation to prevent Git option injection.
//!
//! Git interprets leading-dash values as options, so refs, branch names, and
//! other user input are validated before reaching `git`.

use crate::error::GitError;

/// Rejects NUL/control characters and leading-dash values that Git would treat
/// as options. Returns the trimmed value.
pub fn assert_safe_arg(value: &str, name: &str) -> Result<String, GitError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(GitError::invalid_input(format!("{name} cannot be empty.")));
    }
    if trimmed.starts_with('-') {
        return Err(GitError::invalid_input(format!(
            "{name} cannot start with '-'."
        )));
    }
    if contains_control_char(trimmed) {
        return Err(GitError::invalid_input(format!(
            "{name} contains invalid control characters."
        )));
    }
    Ok(trimmed.to_owned())
}

/// Validates a Git ref-like value (branch, tag, commit, `HEAD~N`).
pub fn assert_safe_ref(value: &str, name: &str) -> Result<String, GitError> {
    let safe = assert_safe_arg(value, name)?;
    if safe.chars().any(char::is_whitespace) {
        return Err(GitError::invalid_input(format!(
            "{name} cannot contain whitespace."
        )));
    }
    Ok(safe)
}

/// Validates a remote name (alphanumeric, dash, underscore, dot).
pub fn assert_safe_remote_name(name: &str) -> Result<String, GitError> {
    let safe = assert_safe_arg(name, "remote")?;
    if !safe
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(GitError::invalid_input(format!(
            "Invalid remote name: {name}"
        )));
    }
    Ok(safe)
}

/// Validates a git command name used for documentation lookup.
pub fn assert_safe_command_name(command: &str) -> Result<String, GitError> {
    let safe = assert_safe_arg(command, "command")?;
    let mut chars = safe.chars();
    let starts_lowercase = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_valid = safe
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !starts_lowercase || !rest_valid {
        return Err(GitError::invalid_input(format!(
            "Invalid command name: {command}"
        )));
    }
    Ok(safe)
}

/// Matches the control-character class `[\u0000-\u001f\u007f]`.
fn contains_control_char(value: &str) -> bool {
    value.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_option_like_values() {
        assert!(assert_safe_arg("", "ref").is_err());
        assert!(assert_safe_arg("   ", "ref").is_err());
        assert!(assert_safe_arg("--force", "ref").is_err());
    }

    #[test]
    fn rejects_control_characters() {
        assert!(assert_safe_arg("main\u{0}", "ref").is_err());
        assert!(assert_safe_arg("main\u{7f}", "ref").is_err());
        assert!(assert_safe_arg("main\n", "ref").is_ok());
    }

    #[test]
    fn trims_and_accepts_valid_refs() {
        assert_eq!(assert_safe_ref(" main ", "ref").unwrap(), "main");
        assert_eq!(assert_safe_ref("HEAD~1", "ref").unwrap(), "HEAD~1");
        assert!(assert_safe_ref("feat x", "ref").is_err());
    }

    #[test]
    fn validates_remote_names() {
        assert_eq!(assert_safe_remote_name("origin").unwrap(), "origin");
        assert!(assert_safe_remote_name("my remote").is_err());
        assert!(assert_safe_remote_name("origin;rm -rf /").is_err());
    }

    #[test]
    fn validates_command_names() {
        assert_eq!(assert_safe_command_name("rebase").unwrap(), "rebase");
        assert!(assert_safe_command_name("Rebase").is_err());
        assert!(assert_safe_command_name("-x").is_err());
        assert!(assert_safe_command_name("re base").is_err());
    }
}
