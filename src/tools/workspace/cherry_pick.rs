//! `git_cherry_pick` — start, continue or abort a cherry-pick.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::workspace::{CherryPickAction, CherryPickOptions, run_cherry_pick};
use crate::tools::output_result;

/// Arguments accepted by `git_cherry_pick`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CherryPickArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Cherry-pick operation.
    #[serde(default)]
    pub action: CherryPickAction,
    /// Commits to cherry-pick.
    #[serde(default)]
    pub refs: Option<Vec<String>>,
    /// Parent number when cherry-picking a merge commit.
    #[serde(default)]
    pub mainline: Option<u32>,
    /// Record the source commit in the message.
    #[serde(default)]
    pub record_origin: bool,
    /// Apply without committing.
    #[serde(default)]
    pub no_commit: bool,
    /// Merge strategy.
    #[serde(default)]
    pub strategy: Option<String>,
    /// Strategy-specific options.
    #[serde(default)]
    pub strategy_options: Option<Vec<String>>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested cherry-pick operation.
pub async fn run(args: &CherryPickArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let refs = args.refs.clone().unwrap_or_default();

    if args.action == CherryPickAction::Start && refs.is_empty() {
        return Err(GitError::invalid_input(
            "refs is required for cherry_pick start.",
        ));
    }

    let output = run_cherry_pick(
        &repo_path,
        &CherryPickOptions {
            action: args.action,
            refs,
            mainline: args.mainline,
            record_origin: args.record_origin,
            no_commit: args.no_commit,
            strategy: args.strategy.clone(),
            strategy_options: args.strategy_options.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
