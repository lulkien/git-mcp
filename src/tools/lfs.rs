//! `git_lfs` — Git Large File Storage management.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::GitError;
use crate::render::{ResponseFormat, render_content};
use crate::services::lfs::{LfsAction, LfsOptions, run_lfs_action};
use crate::tools::ok_result;

/// Arguments accepted by `git_lfs`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LfsArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// LFS operation.
    pub action: LfsAction,
    /// File glob patterns for track/untrack (e.g. `["*.psd", "*.zip"]`).
    #[serde(default)]
    pub patterns: Option<Vec<String>>,
    /// Remote name for pull/push operations.
    #[serde(default)]
    pub remote: Option<String>,
    /// Comma-separated include patterns for migrate or pull operations.
    #[serde(default)]
    pub include: Option<String>,
    /// Comma-separated exclude patterns for migrate or pull operations.
    #[serde(default)]
    pub exclude: Option<String>,
    /// Pass `--all`/`--everything` to include all refs in push/migrate operations.
    #[serde(default)]
    pub everything: bool,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Runs the requested LFS operation.
pub async fn run(args: &LfsArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let output = run_lfs_action(
        &repo_path,
        &LfsOptions {
            action: args.action,
            patterns: args.patterns.clone(),
            remote: args.remote.clone(),
            include: args.include.clone(),
            exclude: args.exclude.clone(),
            everything: args.everything,
        },
    )
    .await?;

    let payload = json!({ "output": output });
    let text = render_content(&payload, args.response_format)?;
    Ok(ok_result(text, payload))
}
