//! `git_submodule` — add, list, update, sync or `set_branch` submodules.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::workspace::{SubmoduleAction, SubmoduleOptions, run_submodule};
use crate::tools::output_result;

/// Arguments accepted by `git_submodule`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SubmoduleArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Submodule operation.
    #[serde(default)]
    pub action: SubmoduleAction,
    /// Repository URL for `add`.
    #[serde(default)]
    pub url: Option<String>,
    /// Submodule path.
    #[serde(default)]
    pub path: Option<String>,
    /// Branch to track for `set_branch`.
    #[serde(default)]
    pub branch: Option<String>,
    /// Operate recursively.
    #[serde(default = "default_true")]
    pub recursive: bool,
    /// Track the remote branch when updating.
    #[serde(default)]
    pub remote: bool,
    /// Clone depth.
    #[serde(default)]
    pub depth: Option<u32>,
    /// Number of parallel jobs.
    #[serde(default)]
    pub jobs: Option<u32>,
    /// Paths to limit `update` to.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_true() -> bool {
    true
}

/// Performs the requested submodule operation.
pub async fn run(args: &SubmoduleArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let output = run_submodule(
        &repo_path,
        &SubmoduleOptions {
            action: args.action,
            url: args.url.clone(),
            path: args.path.clone(),
            paths: args.paths.clone(),
            branch: args.branch.clone(),
            recursive: args.recursive,
            remote: args.remote,
            depth: args.depth,
            jobs: args.jobs,
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
