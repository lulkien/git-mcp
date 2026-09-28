//! `git_commits` — staging, committing, resetting and reverting.

use std::path::Path;

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};
use crate::services::write::{
    AddOptions, CommitOptions, ResetMode, ResetOptions, RestoreOptions, RevertOptions, add_files,
    commit_changes, reset_changes, restore_files, revert_commit,
};
use crate::tools::ok_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CommitAction {
    /// Stage changes.
    Add,
    /// Restore paths in the index and/or working tree.
    Restore,
    /// Create a commit (default).
    #[default]
    Commit,
    /// Move HEAD and/or unstage paths.
    Reset,
    /// Revert a commit.
    Revert,
    /// Soft-reset the previous commit, keeping its changes staged.
    Undo,
    /// Hard-reset the previous commit, discarding its changes.
    Nuke,
    /// Stage everything and commit it as `WIP`.
    Wip,
    /// Unstage paths.
    Unstage,
    /// Amend the previous commit.
    Amend,
}

/// Arguments accepted by `git_commits`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommitArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which commit-area operation to perform.
    #[serde(default)]
    pub action: CommitAction,
    /// Stage tracked modifications before committing.
    #[serde(default)]
    pub all: bool,
    /// Repository-relative paths to stage, restore or unstage.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    /// Commit message.
    #[serde(default)]
    pub message: Option<String>,
    /// Amend the previous commit.
    #[serde(default)]
    pub amend: bool,
    /// Keep the existing message when amending.
    #[serde(default)]
    pub no_edit: bool,
    /// Sign the commit. Defaults to the server `GIT_AUTO_SIGN_COMMITS` setting.
    #[serde(default)]
    pub sign: Option<bool>,
    /// Signing key to use.
    #[serde(default)]
    pub signing_key: Option<String>,
    /// Bypass git hooks (requires `GIT_ALLOW_NO_VERIFY=true`).
    #[serde(default)]
    pub no_verify: bool,
    /// Reset mode.
    #[serde(default)]
    pub mode: ResetMode,
    /// Commit to reset to or revert.
    #[serde(default)]
    pub target: Option<String>,
    /// Confirm destructive operations.
    #[serde(default)]
    pub confirm: bool,
    /// Restore the index.
    #[serde(default)]
    pub staged: bool,
    /// Restore the working tree.
    #[serde(default = "default_true")]
    pub worktree: bool,
    /// Revision to restore from.
    #[serde(default)]
    pub source: Option<String>,
    /// Commit to revert.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// Apply the inverse of a revert without committing it.
    #[serde(default)]
    pub no_commit: bool,
    /// Parent number for reverting a merge commit.
    #[serde(default)]
    pub mainline: Option<u32>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_true() -> bool {
    true
}

/// Performs the requested commit-area operation.
pub async fn run(args: &CommitArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    match args.action {
        CommitAction::Add => run_add(args, &repo_path).await,
        CommitAction::Restore => run_restore(args, &repo_path).await,
        CommitAction::Commit => run_commit(args, &repo_path).await,
        CommitAction::Reset => run_reset(args, &repo_path).await,
        CommitAction::Revert => run_revert(args, &repo_path).await,
        CommitAction::Undo => run_undo(args, &repo_path).await,
        CommitAction::Nuke => run_nuke(args, &repo_path).await,
        CommitAction::Wip => run_wip(args, &repo_path).await,
        CommitAction::Unstage => run_unstage(args, &repo_path).await,
        CommitAction::Amend => run_amend(args, &repo_path).await,
    }
}

/// Requires a non-empty path list for the actions that operate on paths.
fn required_paths(args: &CommitArgs, message: &str) -> Result<Vec<String>, GitError> {
    match &args.paths {
        Some(paths) if !paths.is_empty() => Ok(paths.clone()),
        _ => Err(GitError::invalid_input(message)),
    }
}

async fn run_add(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let output = add_files(
        repo_path,
        &AddOptions {
            all: args.all,
            paths: args.paths.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_restore(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let paths = required_paths(args, "paths are required for commit restore.")?;
    let output = restore_files(
        repo_path,
        &RestoreOptions {
            paths,
            staged: args.staged,
            worktree: args.worktree,
            source: args.source.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_commit(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let Some(message) = args.message.clone() else {
        return Err(GitError::invalid_input(
            "message is required for commit action.",
        ));
    };
    let output = commit_changes(repo_path, &commit_options(args, message)).await?;
    output_result(&output, args.response_format)
}

async fn run_reset(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    if args.mode == ResetMode::Hard && !args.confirm {
        return Err(GitError::invalid_input("Hard reset requires confirm=true."));
    }
    let output = reset_changes(
        repo_path,
        &ResetOptions {
            mode: args.mode,
            target: args.target.clone(),
            paths: args.paths.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_revert(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let Some(reference) = args.ref_name.clone() else {
        return Err(GitError::invalid_input("ref is required for revert."));
    };
    let output = revert_commit(
        repo_path,
        &RevertOptions {
            reference,
            no_commit: args.no_commit,
            mainline: args.mainline,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_undo(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let output = reset_changes(
        repo_path,
        &ResetOptions {
            mode: ResetMode::Soft,
            target: Some("HEAD~1".to_owned()),
            paths: None,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_nuke(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    if !args.confirm {
        return Err(GitError::invalid_input(
            "nuke requires confirm=true because it performs a hard reset.",
        ));
    }
    let output = reset_changes(
        repo_path,
        &ResetOptions {
            mode: ResetMode::Hard,
            target: Some("HEAD~1".to_owned()),
            paths: None,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_wip(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    add_files(
        repo_path,
        &AddOptions {
            all: true,
            paths: None,
        },
    )
    .await?;
    let output = commit_changes(
        repo_path,
        &CommitOptions {
            message: "WIP".to_owned(),
            all: false,
            amend: false,
            no_edit: false,
            sign: None,
            signing_key: None,
            no_verify: false,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_unstage(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let paths = required_paths(args, "paths are required for unstage.")?;
    let output = restore_files(
        repo_path,
        &RestoreOptions {
            paths,
            staged: true,
            worktree: false,
            source: None,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_amend(args: &CommitArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let message = args.message.clone().unwrap_or_else(|| "amend".to_owned());
    let options = CommitOptions {
        no_edit: true,
        amend: true,
        ..commit_options(args, message)
    };
    let output = commit_changes(repo_path, &options).await?;
    output_result(&output, args.response_format)
}

fn commit_options(args: &CommitArgs, message: String) -> CommitOptions {
    CommitOptions {
        message,
        all: args.all,
        amend: args.amend,
        no_edit: args.no_edit,
        sign: args.sign,
        signing_key: args.signing_key.clone(),
        no_verify: args.no_verify,
    }
}

fn output_result(output: &str, format: ResponseFormat) -> Result<CallToolResult, GitError> {
    let text = render_content(&to_value(output)?, format)?;
    Ok(ok_result(text, json!({ "output": output })))
}
