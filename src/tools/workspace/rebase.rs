//! `git_rebase` — start, continue, abort or skip a rebase.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::workspace::{RebaseAction, RebaseOptions, run_rebase};
use crate::tools::output_result;

/// Arguments accepted by `git_rebase`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RebaseArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Rebase operation.
    #[serde(default)]
    pub action: RebaseAction,
    /// Rebase interactively.
    #[serde(default)]
    pub interactive: bool,
    /// Move fixup and squash commits into place.
    #[serde(default)]
    pub autosquash: bool,
    /// Preserve merge commits.
    #[serde(default)]
    pub merges: bool,
    /// New base for `--onto`.
    #[serde(default)]
    pub onto: Option<String>,
    /// Upstream to rebase onto.
    #[serde(default)]
    pub upstream: Option<String>,
    /// Branch to rebase.
    #[serde(default)]
    pub branch: Option<String>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested rebase operation.
pub async fn run(args: &RebaseArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    if args.action == RebaseAction::Start && args.upstream.is_none() {
        return Err(GitError::invalid_input(
            "upstream is required for rebase start.",
        ));
    }

    let output = run_rebase(
        &repo_path,
        &RebaseOptions {
            action: args.action,
            interactive: args.interactive,
            autosquash: args.autosquash,
            merges: args.merges,
            onto: args.onto.clone(),
            upstream: args.upstream.clone(),
            branch: args.branch.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
