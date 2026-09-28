//! Context service: repository summary, history search and git config access.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::constants::CHARACTER_LIMIT;
use crate::error::GitError;
use crate::git::Git;
use crate::services::inspect::{GitLogOptions, get_log, get_status};

/// A commit summarized for the context overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentCommit {
    /// Full commit hash.
    pub hash: String,
    /// Commit subject line.
    pub subject: String,
    /// Author date, ISO-8601 strict.
    pub date_iso: String,
}

/// Operations currently in progress in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InProgressState {
    /// A rebase is in progress.
    pub rebasing: bool,
    /// A merge is in progress.
    pub merging: bool,
    /// A cherry-pick is in progress.
    pub cherry_picking: bool,
    /// A bisect session is in progress.
    pub bisecting: bool,
}

/// High-level repository context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContextSummary {
    /// Current branch.
    pub branch: String,
    /// Commits ahead of the upstream.
    pub ahead: i64,
    /// Commits behind the upstream.
    pub behind: i64,
    /// True when the working tree has no changes.
    pub is_clean: bool,
    /// Number of changed files.
    pub changed_files: usize,
    /// The five most recent commits.
    pub recent_commits: Vec<RecentCommit>,
    /// Names of the configured remotes.
    pub remotes: Vec<String>,
    /// Multi-step operations currently in progress.
    pub in_progress: InProgressState,
}

static BLOCKED_CONFIG_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^credential\.|^url\.|^core\.sshcommand$|^http\..*extraheader$")
        .expect("valid blocked key pattern")
});

static SENSITIVE_CONFIG_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(password|token|secret|auth|passphrase)").expect("valid sensitive key pattern")
});

static URL_CREDENTIALS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(https?://)[^@\s]+@").expect("valid url credentials pattern"));

static TOKEN_VALUE_PATTERNS: LazyLock<[Regex; 4]> = LazyLock::new(|| {
    [
        Regex::new(r"(?i)\b[0-9a-f]{40,}\b").expect("valid hex token pattern"),
        Regex::new(r"\bgh[pousr]_[A-Z0-9]{20,}\b").expect("valid github token pattern"),
        Regex::new(r"\bglpat-[A-Z0-9_-]{20,}\b").expect("valid gitlab token pattern"),
        Regex::new(r"\bxox[baprs]-[A-Z0-9-]{10,}\b").expect("valid slack token pattern"),
    ]
});

/// True when a git config key must not be read or written.
#[must_use]
pub fn is_blocked_config_key(key: &str) -> bool {
    BLOCKED_CONFIG_KEY.is_match(key)
}

/// Redacts secrets from a git config value.
#[must_use]
pub fn redact_config_entry(key: &str, value: &str) -> String {
    if SENSITIVE_CONFIG_KEY.is_match(key) {
        return "***".to_owned();
    }

    let stripped = URL_CREDENTIALS.replace_all(value, "$1***@").into_owned();
    if TOKEN_VALUE_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(&stripped))
    {
        return "***".to_owned();
    }
    stripped
}

/// Summarizes branch, worktree and in-progress state.
pub async fn get_context_summary(repo_path: &Path) -> Result<ContextSummary, GitError> {
    let git = Git::open(repo_path)?;
    let options = GitLogOptions {
        limit: 5,
        offset: 0,
        ..GitLogOptions::default()
    };

    let (status, commits, remotes_output, git_dir) = tokio::join!(
        get_status(repo_path),
        get_log(repo_path, &options),
        git.raw(&["remote"]),
        git.raw(&["rev-parse", "--absolute-git-dir"]),
    );

    let status = status?;
    let commits = commits?;
    let git_dir = git_dir?;
    let git_dir = Path::new(git_dir.trim()).to_path_buf();

    let remotes = remotes_output
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();

    Ok(ContextSummary {
        branch: status.current,
        ahead: status.ahead,
        behind: status.behind,
        is_clean: status.is_clean,
        changed_files: status.files.len(),
        recent_commits: commits
            .into_iter()
            .map(|commit| RecentCommit {
                hash: commit.hash,
                subject: commit.subject,
                date_iso: commit.date_iso,
            })
            .collect(),
        remotes,
        in_progress: InProgressState {
            rebasing: git_dir.join("rebase-merge").exists()
                || git_dir.join("rebase-apply").exists(),
            merging: git_dir.join("MERGE_HEAD").exists(),
            cherry_picking: git_dir.join("CHERRY_PICK_HEAD").exists(),
            bisecting: git_dir.join("BISECT_LOG").exists(),
        },
    })
}

/// Searches history with pickaxe and working-tree grep, as a markdown report.
pub async fn search_history(
    repo_path: &Path,
    query: &str,
    limit: usize,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    let pickaxe_args = vec![
        "log".to_owned(),
        "-S".to_owned(),
        query.to_owned(),
        "--oneline".to_owned(),
        "-n".to_owned(),
        limit.to_string(),
    ];
    let grep_args = vec![
        "grep".to_owned(),
        "-n".to_owned(),
        "-m".to_owned(),
        limit.to_string(),
        "--".to_owned(),
        query.to_owned(),
    ];

    let (pickaxe, grep) = tokio::join!(git.raw(&pickaxe_args), git.raw(&grep_args));
    let pickaxe = pickaxe?;
    let grep = grep.unwrap_or_default();

    let sections = [
        "## Pickaxe (-S)".to_owned(),
        fallback(pickaxe.trim(), "No history matches."),
        String::new(),
        "## grep".to_owned(),
        fallback(grep.trim(), "No working-tree matches."),
    ];
    let combined = sections.join("\n");

    Ok(if combined.chars().count() > CHARACTER_LIMIT {
        let head: String = combined.chars().take(CHARACTER_LIMIT).collect();
        format!("{head}\n\n[Output truncated at {CHARACTER_LIMIT} characters]")
    } else {
        combined
    })
}

/// Reads one config key, or the whole config with secrets redacted.
pub async fn get_config(repo_path: &Path, key: Option<&str>) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    if let Some(key) = key {
        if is_blocked_config_key(key) {
            return Err(GitError::invalid_input(format!(
                "Access to git config key '{key}' is not permitted."
            )));
        }
        let value = git.raw(&["config", "--get", key]).await?;
        return Ok(redact_config_entry(key, value.trim()));
    }

    let output = git.raw(&["config", "--list"]).await?;
    let lines: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            if is_blocked_config_key(key) {
                return None;
            }
            Some(format!("{key}={}", redact_config_entry(key, value)))
        })
        .collect();

    Ok(lines.join("\n"))
}

/// Writes a local config key.
pub async fn set_config(repo_path: &Path, key: &str, value: &str) -> Result<String, GitError> {
    if is_blocked_config_key(key) {
        return Err(GitError::invalid_input(format!(
            "Writing git config key '{key}' is not permitted."
        )));
    }

    let git = Git::open(repo_path)?;
    git.raw(&["config", "--local", key, value]).await?;
    Ok(format!("Set {key}."))
}

fn fallback(value: &str, placeholder: &str) -> String {
    if value.is_empty() {
        placeholder.to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_credential_bearing_config_keys() {
        assert!(is_blocked_config_key("credential.helper"));
        assert!(is_blocked_config_key("url.https://x.insteadOf"));
        assert!(is_blocked_config_key("core.sshCommand"));
        assert!(is_blocked_config_key("http.https://x.extraHeader"));
        assert!(!is_blocked_config_key("user.name"));
        assert!(!is_blocked_config_key("core.editor"));
    }

    #[test]
    fn redacts_sensitive_config_values() {
        assert_eq!(redact_config_entry("user.token", "abc"), "***");
        assert_eq!(redact_config_entry("user.name", "daniel"), "daniel");
        assert_eq!(
            redact_config_entry("remote.origin.url", "https://user:pass@host/x"),
            "https://***@host/x"
        );
        assert_eq!(
            redact_config_entry("user.name", "ghp_ABCDEFGHIJKLMNOPQRSTUVWX"),
            "***"
        );
    }
}
