//! `git_worktree` — add, list, remove, lock, unlock, prune or repair worktrees.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::workspace::{WorktreeAction, WorktreeOptions, run_worktree};
use crate::tools::output_result;

/// Arguments accepted by `git_worktree`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorktreeArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Worktree operation.
    #[serde(default)]
    pub action: WorktreeAction,
    /// Worktree path.
    #[serde(default)]
    pub path: Option<String>,
    /// Branch checked out in the new worktree.
    #[serde(default)]
    pub branch: Option<String>,
    /// Force the operation.
    #[serde(default)]
    pub force: bool,
    /// Check out a detached HEAD.
    #[serde(default)]
    pub detached: bool,
    /// Reason recorded when locking.
    #[serde(default)]
    pub lock_reason: Option<String>,
    /// Expiry for `prune`.
    #[serde(default)]
    pub expire: Option<String>,
    /// Paths for `repair`.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested worktree operation.
pub async fn run(args: &WorktreeArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    if args.action == WorktreeAction::Add && args.branch.is_none() && !args.detached {
        return Err(GitError::invalid_input(
            "branch is required for worktree add unless detached=true.",
        ));
    }

    let output = run_worktree(
        &repo_path,
        &WorktreeOptions {
            action: args.action,
            path: args.path.clone(),
            paths: args.paths.clone(),
            branch: args.branch.clone(),
            force: args.force,
            detached: args.detached,
            lock_reason: args.lock_reason.clone(),
            expire: args.expire.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
