//! `git_analytics` — read-only repository analytics.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};
use crate::services::analytics::{
    get_activity, get_churn, get_contributors, get_file_stats, get_repo_summary,
};
use crate::tools::ok_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AnalyticsAction {
    /// Per-author commit and line statistics.
    Contributors,
    /// Most frequently changed files.
    Churn,
    /// Commits per day.
    Activity,
    /// Repository-level counts and top contributors.
    Summary,
    /// Tracked file statistics.
    FileStats,
}

/// Arguments accepted by `git_analytics`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct AnalyticsArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Analytics operation.
    pub action: AnalyticsAction,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Computes the requested analytics view.
pub async fn run(args: &AnalyticsArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    match args.action {
        AnalyticsAction::Contributors => {
            let result = get_contributors(&repo_path).await?;
            payload("contributors", result, args.response_format)
        }
        AnalyticsAction::Churn => {
            let result = get_churn(&repo_path).await?;
            payload("churn", result, args.response_format)
        }
        AnalyticsAction::Activity => {
            let result = get_activity(&repo_path).await?;
            payload("activity", result, args.response_format)
        }
        AnalyticsAction::Summary => {
            let result = get_repo_summary(&repo_path).await?;
            payload("summary", result, args.response_format)
        }
        AnalyticsAction::FileStats => {
            let result = get_file_stats(&repo_path).await?;
            payload("fileStats", result, args.response_format)
        }
    }
}

/// Wraps a service result under its response key, rendering it for `content`.
fn payload<T: Serialize>(
    key: &str,
    result: T,
    format: ResponseFormat,
) -> Result<CallToolResult, GitError> {
    let value = to_value(&result)?;
    let text = render_content(&value, format)?;

    let mut structured = serde_json::Map::new();
    structured.insert(key.to_owned(), value);
    Ok(ok_result(text, serde_json::Value::Object(structured)))
}
