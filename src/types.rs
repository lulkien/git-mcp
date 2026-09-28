//! Shared data transfer objects returned in tool `structuredContent`.
//!
//! Field names are camelCase on the wire.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single commit as reported by `git log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    /// Full commit hash.
    pub hash: String,
    /// Author name.
    pub author_name: String,
    /// Author email.
    pub author_email: String,
    /// Author date, ISO-8601 strict (`%aI`).
    pub date_iso: String,
    /// Commit subject line.
    pub subject: String,
}

/// One entry of `git status --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    /// Path relative to the repository root.
    pub path: String,
    /// Index (staged) status character.
    pub index: String,
    /// Working tree (unstaged) status character.
    pub working_tree: String,
}

/// Working tree and branch state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusResult {
    /// Current branch name, or an empty string when detached.
    pub branch: String,
    /// Current branch name, or an empty string when detached.
    pub current: String,
    /// Upstream tracking branch, or an empty string when untracked.
    pub tracking: String,
    /// Commits ahead of the upstream.
    pub ahead: i64,
    /// Commits behind the upstream.
    pub behind: i64,
    /// Changed files.
    pub files: Vec<FileStatus>,
    /// True when there is nothing to commit.
    pub is_clean: bool,
}

/// Aggregate counts for a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    /// Number of files changed.
    pub files_changed: usize,
    /// Total inserted lines.
    pub insertions: u64,
    /// Total deleted lines.
    pub deletions: u64,
}
