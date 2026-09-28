//! Write service: staging, restoring, committing, resetting and reverting.

use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::config::config;
use crate::constants::error_templates;
use crate::error::GitError;
use crate::git::{Git, validate_path_arguments};
use crate::security::assert_safe_ref;

/// Options for staging files.
#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    /// Stage every change in the repository.
    pub all: bool,
    /// Repository-relative paths to stage.
    pub paths: Option<Vec<String>>,
}

/// Options for `git restore`.
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Repository-relative paths to restore.
    pub paths: Vec<String>,
    /// Restore the index from `source`.
    pub staged: bool,
    /// Restore the working tree from the index (or `source`).
    pub worktree: bool,
    /// Revision to restore from.
    pub source: Option<String>,
}

/// Options for `git commit`.
#[derive(Debug, Clone)]
pub struct CommitOptions {
    /// Commit message.
    pub message: String,
    /// Stage tracked modifications before committing (`-a`).
    pub all: bool,
    /// Amend the previous commit.
    pub amend: bool,
    /// Keep the existing message when amending.
    pub no_edit: bool,
    /// Sign the commit; defaults to the server auto-sign setting.
    pub sign: Option<bool>,
    /// Signing key, falling back to the configured default.
    pub signing_key: Option<String>,
    /// Bypass pre-commit and commit-msg hooks.
    pub no_verify: bool,
}

/// Reset mode selected by the caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResetMode {
    /// Move HEAD, keep the index and working tree.
    Soft,
    /// Move HEAD and reset the index (default).
    #[default]
    Mixed,
    /// Move HEAD and discard index and working tree changes.
    Hard,
}

impl ResetMode {
    /// Wire representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Mixed => "mixed",
            Self::Hard => "hard",
        }
    }
}

/// Options for `git reset`.
#[derive(Debug, Clone)]
pub struct ResetOptions {
    /// Reset mode.
    pub mode: ResetMode,
    /// Commit to reset to.
    pub target: Option<String>,
    /// Repository-relative paths to unstage instead of moving HEAD.
    pub paths: Option<Vec<String>>,
}

/// Options for `git revert`.
#[derive(Debug, Clone)]
pub struct RevertOptions {
    /// Commit to revert.
    pub reference: String,
    /// Apply the inverse without committing.
    pub no_commit: bool,
    /// Parent number for merge commits.
    pub mainline: Option<u32>,
}

/// Stages all changes, or the given paths.
pub async fn add_files(repo_path: &Path, options: &AddOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    if options.all {
        git.raw(&["add", "."]).await?;
        return Ok("Staged all changes.".to_owned());
    }

    let paths = options.paths.clone().unwrap_or_default();
    if paths.is_empty() {
        return Err(GitError::invalid_input("Provide paths or set all=true."));
    }

    let safe_paths = validate_path_arguments(repo_path, &paths)?;
    let mut args = vec!["add".to_owned(), "--".to_owned()];
    args.extend(safe_paths.iter().cloned());
    git.raw(&args).await?;
    Ok(format!("Staged {} path(s).", safe_paths.len()))
}

/// Restores paths in the index and/or working tree.
pub async fn restore_files(repo_path: &Path, options: &RestoreOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let safe_paths = validate_path_arguments(repo_path, &options.paths)?;

    if !options.staged && !options.worktree {
        return Err(GitError::invalid_input(
            "At least one of staged/worktree must be true.",
        ));
    }

    let mut args = vec!["restore".to_owned()];
    if options.staged {
        args.push("--staged".to_owned());
    }
    if options.worktree {
        args.push("--worktree".to_owned());
    }
    if let Some(source) = &options.source {
        args.push("--source".to_owned());
        args.push(assert_safe_ref(source, "source")?);
    }
    args.push("--".to_owned());
    args.extend(safe_paths.iter().cloned());

    git.raw(&args).await?;
    Ok(format!("Restored {} path(s).", safe_paths.len()))
}

/// Creates a commit and reports the new abbreviated hash.
pub async fn commit_changes(repo_path: &Path, options: &CommitOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let server_config = config();

    if options.no_verify && !server_config.allow_no_verify {
        return Err(GitError::invalid_input(
            error_templates::HOOK_BYPASS_DISABLED,
        ));
    }

    let mut args = vec!["commit".to_owned()];
    if options.all {
        args.push("-a".to_owned());
    }
    if options.amend {
        args.push("--amend".to_owned());
    }
    if options.no_edit {
        args.push("--no-edit".to_owned());
    }
    if options.no_verify {
        args.push("--no-verify".to_owned());
    }

    let should_sign = options.sign.unwrap_or(server_config.auto_sign_commits);
    if should_sign {
        let key = options
            .signing_key
            .clone()
            .or_else(|| server_config.signing_key.clone());
        args.push(match key {
            Some(key) => format!("--gpg-sign={key}"),
            None => "--gpg-sign".to_owned(),
        });
    }

    args.push("-m".to_owned());
    args.push(options.message.clone());
    git.raw(&args).await?;

    let commit = git.raw(&["rev-parse", "--short", "HEAD"]).await?;
    Ok(format!("Committed {}.", commit.trim()))
}

/// Unstages paths, or moves HEAD in the requested mode.
pub async fn reset_changes(repo_path: &Path, options: &ResetOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    if let Some(paths) = &options.paths
        && !paths.is_empty()
    {
        let safe_paths = validate_path_arguments(repo_path, paths)?;
        let mut args = vec!["reset".to_owned()];
        if let Some(target) = &options.target {
            args.push(assert_safe_ref(target, "target")?);
        }
        args.push("--".to_owned());
        args.extend(safe_paths.iter().cloned());
        git.raw(&args).await?;
        return Ok(format!("Unstaged {} path(s).", safe_paths.len()));
    }

    let mut args = vec!["reset".to_owned(), format!("--{}", options.mode.as_str())];
    if let Some(target) = &options.target {
        args.push(assert_safe_ref(target, "target")?);
    }
    git.raw(&args).await?;
    Ok(format!(
        "Reset completed with mode={}.",
        options.mode.as_str()
    ))
}

/// Reverts a commit.
pub async fn revert_commit(repo_path: &Path, options: &RevertOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    let mut args = vec!["revert".to_owned()];
    if options.no_commit {
        args.push("--no-commit".to_owned());
    }
    if let Some(mainline) = options.mainline {
        args.push("-m".to_owned());
        args.push(mainline.to_string());
    }
    args.push(assert_safe_ref(&options.reference, "ref")?);

    git.raw(&args).await?;
    Ok(format!("Reverted {}.", options.reference))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_mode_wire_names() {
        assert_eq!(ResetMode::Soft.as_str(), "soft");
        assert_eq!(ResetMode::Mixed.as_str(), "mixed");
        assert_eq!(ResetMode::Hard.as_str(), "hard");
        assert_eq!(ResetMode::default(), ResetMode::Mixed);
    }

    #[test]
    fn hook_bypass_is_gated_by_configuration() {
        // The test environment leaves GIT_ALLOW_NO_VERIFY unset, so the gate is on.
        assert!(!config().allow_no_verify);
        assert_eq!(
            error_templates::HOOK_BYPASS_DISABLED,
            "no_verify is disabled on this server. Set GIT_ALLOW_NO_VERIFY=true to permit bypassing git hooks."
        );
    }
}
