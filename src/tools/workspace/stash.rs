//! `git_stash` — stash, list, apply, pop or drop stashes.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::advanced::{StashAction, StashOptions, run_stash};
use crate::tools::output_result;

/// Arguments accepted by `git_stash`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StashArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Stash operation.
    #[serde(default)]
    pub action: StashAction,
    /// Stash message.
    #[serde(default)]
    pub message: Option<String>,
    /// Stash index.
    #[serde(default)]
    pub index: Option<usize>,
    /// Include untracked files when saving.
    #[serde(default)]
    pub include_untracked: bool,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested stash operation.
pub async fn run(args: &StashArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let output = run_stash(
        &repo_path,
        &StashOptions {
            action: args.action,
            message: args.message.clone(),
            index: args.index,
            include_untracked: args.include_untracked,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
