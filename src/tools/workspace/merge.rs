//! `git_merge` — start, continue or abort a merge.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::workspace::{ConflictStyle, MergeAction, MergeOptions, run_merge};
use crate::tools::output_result;

/// Arguments accepted by `git_merge`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct MergeArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Merge operation.
    #[serde(default)]
    pub action: MergeAction,
    /// Commits to merge.
    #[serde(default)]
    pub refs: Option<Vec<String>>,
    /// Always create a merge commit.
    #[serde(default)]
    pub no_ff: bool,
    /// Refuse anything but a fast-forward.
    #[serde(default)]
    pub ff_only: bool,
    /// Squash the merge into a single staged change.
    #[serde(default)]
    pub squash: bool,
    /// Stage the merge result without committing.
    #[serde(default)]
    pub no_commit: bool,
    /// Include a shortlog of merged commits.
    #[serde(default)]
    pub log: bool,
    /// Merge strategy.
    #[serde(default)]
    pub strategy: Option<String>,
    /// Merge strategy options.
    #[serde(default)]
    pub strategy_options: Option<Vec<String>>,
    /// Conflict marker style.
    #[serde(default)]
    pub conflict_style: Option<ConflictStyle>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested merge operation.
pub async fn run(args: &MergeArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let refs = args.refs.clone().unwrap_or_default();

    if args.action == MergeAction::Start && refs.is_empty() {
        return Err(GitError::invalid_input("refs is required for merge start."));
    }

    let output = run_merge(
        &repo_path,
        &MergeOptions {
            action: args.action,
            refs,
            no_ff: args.no_ff,
            ff_only: args.ff_only,
            squash: args.squash,
            no_commit: args.no_commit,
            log: args.log,
            strategy: args.strategy.clone(),
            strategy_options: args.strategy_options.clone(),
            conflict_style: args.conflict_style,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
