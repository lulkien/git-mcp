//! History rewrite service: reword, squash, rewrite-messages, backup, restore.
//!
//! Commit messages are staged in a temporary file and read by the
//! `--msg-filter` script, so message content is never interpolated into a shell
//! command string.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::GitError;
use crate::git::Git;
use crate::security::{assert_safe_arg, assert_safe_ref};
use crate::services::preflight::{
    assert_clean_worktree, assert_no_in_progress_operation, assert_not_detached,
};

/// Prefix applied to every backup branch this server creates.
pub const BACKUP_PREFIX: &str = "rewrite-backup/";

/// Options for rewording a commit.
#[derive(Debug, Clone)]
pub struct RewordOptions {
    /// Commit to reword; `HEAD` (the default) is amended in place.
    pub reference: Option<String>,
    /// Replacement message.
    pub message: String,
}

/// Options for squashing commits.
#[derive(Debug, Clone)]
pub struct SquashOptions {
    /// Number of commits to squash.
    pub count: u32,
    /// Replacement message.
    pub message: String,
}

/// Options for rewriting messages across a range.
#[derive(Debug, Clone)]
pub struct RewriteMessagesOptions {
    /// Revision range to rewrite.
    pub range: String,
    /// Map of commit SHA (short or full) to replacement message.
    pub messages: BTreeMap<String, String>,
}

/// Options for creating or restoring a backup branch.
#[derive(Debug, Clone)]
pub struct BackupOptions {
    /// Backup name, suffixed onto [`BACKUP_PREFIX`].
    pub name: String,
}

/// Rewrites the message of a single commit.
///
/// `HEAD` is amended in place; any other commit is rewritten with
/// `git filter-branch --msg-filter` reading the replacement from a temp file.
pub async fn reword_commit(repo_path: &Path, options: &RewordOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let target = options.reference.as_deref().unwrap_or("HEAD");

    if target == "HEAD" {
        git.raw(&["commit", "--amend", "-m", &options.message])
            .await?;
        return Ok(format!("Reworded HEAD to: {}", options.message));
    }

    let reference = assert_safe_ref(target, "ref")?;
    assert_clean_worktree(repo_path).await?;
    assert_not_detached(repo_path).await?;
    assert_no_in_progress_operation(repo_path).await?;

    let dir = tempfile::tempdir().map_err(|error| GitError::classified(error.to_string()))?;
    let message_file = dir.path().join("message.txt");
    std::fs::write(&message_file, &options.message)
        .map_err(|error| GitError::classified(error.to_string()))?;

    // The filter references only fixed paths — never message content — so shell
    // metacharacters in the message cannot execute.
    let filter = format!(
        "if [ \"$GIT_COMMIT\" = \"{reference}\" ]; then cat \"{}\"; else cat; fi",
        message_file.display()
    );
    git.raw(&[
        "filter-branch",
        "--force",
        "--msg-filter",
        &filter,
        "--",
        "HEAD",
    ])
    .await?;

    Ok(format!("Reworded {reference} to: {}", options.message))
}

/// Squashes the last `count` commits into one with the given message.
///
/// Uses `git reset --soft` plus `git commit`, which is non-interactive and safe.
pub async fn squash_commits(repo_path: &Path, options: &SquashOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    if options.count < 2 {
        return Err(GitError::invalid_input(
            "count must be at least 2 to squash commits.",
        ));
    }

    assert_clean_worktree(repo_path).await?;
    assert_not_detached(repo_path).await?;
    assert_no_in_progress_operation(repo_path).await?;

    git.raw(&["reset", "--soft", &format!("HEAD~{}", options.count)])
        .await?;
    git.raw(&["commit", "-m", &options.message]).await?;

    Ok(format!(
        "Squashed last {} commits into: {}",
        options.count, options.message
    ))
}

/// Rewrites commit messages across a range using an explicit SHA-to-message map.
///
/// Commits absent from the map keep their original message.
pub async fn rewrite_messages(
    repo_path: &Path,
    options: &RewriteMessagesOptions,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    if options.messages.is_empty() {
        return Err(GitError::invalid_input(
            "messages mapping must not be empty.",
        ));
    }
    let range = assert_safe_arg(&options.range, "range")?;

    assert_clean_worktree(repo_path).await?;
    assert_not_detached(repo_path).await?;
    assert_no_in_progress_operation(repo_path).await?;

    let dir = tempfile::tempdir().map_err(|error| GitError::classified(error.to_string()))?;
    let map_file = dir.path().join("messages.txt");
    let contents = options
        .messages
        .iter()
        .map(|(sha, message)| format!("{sha} {message}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&map_file, contents).map_err(|error| GitError::classified(error.to_string()))?;

    let filter = format!(
        "line=$(grep \"^$GIT_COMMIT \" \"{}\"); if [ -n \"$line\" ]; then printf '%s' \"${{line#* }}\"; else cat; fi",
        map_file.display()
    );
    git.raw(&[
        "filter-branch",
        "--force",
        "--msg-filter",
        &filter,
        "--",
        &range,
    ])
    .await?;

    Ok(format!(
        "Rewrote messages for {} commit(s) in {range}.",
        options.messages.len()
    ))
}

/// Creates a backup branch at the current HEAD.
pub async fn create_backup(repo_path: &Path, options: &BackupOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let branch = backup_branch(&options.name)?;
    git.raw(&["branch", &branch]).await?;
    Ok(format!("Created backup branch {branch}."))
}

/// Restores the repository to a previously created backup branch.
pub async fn restore_backup(repo_path: &Path, options: &BackupOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let branch = backup_branch(&options.name)?;

    assert_clean_worktree(repo_path).await?;
    assert_no_in_progress_operation(repo_path).await?;

    git.raw(&["reset", "--hard", &branch]).await?;
    Ok(format!("Restored to backup branch {branch}."))
}

fn backup_branch(name: &str) -> Result<String, GitError> {
    let name = assert_safe_arg(name, "name")?;
    Ok(format!("{BACKUP_PREFIX}{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_prefixed_backup_branches() {
        assert_eq!(
            backup_branch("before-rewrite").unwrap(),
            "rewrite-backup/before-rewrite"
        );
    }

    #[test]
    fn rejects_option_like_backup_names() {
        assert!(backup_branch("--force").is_err());
        assert!(backup_branch("").is_err());
    }
}
