//! External VCS awareness tools: `git_but_check`, `git_jj_check`,
//! `git_tangled_check` and `git_entire_check`.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};
use crate::services::{but, entire, jj, tangled};
use crate::tools::ok_result;

/// Arguments accepted by `git_but_check`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ButCheckArgs {
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Arguments accepted by the repository-scoped check tools.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RepoCheckArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Reports whether the GitButler CLI is available.
pub async fn run_but(args: &ButCheckArgs) -> Result<CallToolResult, GitError> {
    check_result(&to_value(but::check_but().await)?, args.response_format)
}

/// Reports whether Jujutsu is available and whether the repo is jj-managed.
pub async fn run_jj(args: &RepoCheckArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    check_result(
        &to_value(jj::check_jj(&repo_path).await)?,
        args.response_format,
    )
}

/// Reports whether the repository is hosted on Tangled.
pub async fn run_tangled(args: &RepoCheckArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    check_result(
        &to_value(tangled::check_tangled(&repo_path).await?)?,
        args.response_format,
    )
}

/// Reports whether the Entire CLI is available and whether the repo is managed.
pub async fn run_entire(args: &RepoCheckArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    check_result(
        &to_value(entire::check_entire(&repo_path).await)?,
        args.response_format,
    )
}

fn check_result(
    result: &serde_json::Value,
    format: ResponseFormat,
) -> Result<CallToolResult, GitError> {
    let text = render_content(result, format)?;
    Ok(ok_result(text, json!({ "result": result })))
}
