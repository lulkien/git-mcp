//! `git_bisect` — start, good, bad, skip, run or reset a bisect session.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::advanced::{BisectAction, BisectOptions, run_bisect};
use crate::tools::output_result;

/// Arguments accepted by `git_bisect`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BisectArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Bisect operation.
    #[serde(default)]
    pub action: BisectAction,
    /// Commit to mark.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// Known-good commit.
    #[serde(default)]
    pub good_ref: Option<String>,
    /// Known-bad commit.
    #[serde(default)]
    pub bad_ref: Option<String>,
    /// Single executable token for `run`.
    #[serde(default)]
    pub command: Option<String>,
    /// Full argv for `run`.
    #[serde(default)]
    pub command_args: Option<Vec<String>>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested bisect operation.
pub async fn run(args: &BisectArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let output = run_bisect(
        &repo_path,
        &BisectOptions {
            action: args.action,
            reference: args.ref_name.clone(),
            good_ref: args.good_ref.clone(),
            bad_ref: args.bad_ref.clone(),
            command: args.command.clone(),
            command_args: args.command_args.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
