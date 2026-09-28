//! `git_context` — repository summary, history search and config access.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::git::Git;
use crate::render::{ResponseFormat, render_content};
use crate::services::context::{get_config, get_context_summary, search_history, set_config};
use crate::tools::{ok_result, output_result};

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextAction {
    /// Summarize repository state (default).
    #[default]
    Summary,
    /// Search history and the working tree.
    Search,
    /// Read git config.
    GetConfig,
    /// Write a local git config key.
    SetConfig,
    /// List configured aliases.
    Aliases,
}

/// Arguments accepted by `git_context`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ContextArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which context operation to perform.
    #[serde(default)]
    pub action: ContextAction,
    /// Search query.
    #[serde(default)]
    pub query: Option<String>,
    /// Maximum number of results.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Git config key.
    #[serde(default)]
    pub key: Option<String>,
    /// Git config value.
    #[serde(default)]
    pub value: Option<String>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_limit() -> usize {
    20
}

/// Performs the requested context operation.
pub async fn run(args: &ContextArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    match args.action {
        ContextAction::Summary => {
            let summary = get_context_summary(&repo_path).await?;
            let text = render_content(&to_value(&summary)?, args.response_format)?;
            Ok(ok_result(text, json!({ "summary": summary })))
        }
        ContextAction::Search => {
            let Some(query) = &args.query else {
                return Err(GitError::invalid_input(
                    "query is required for context search.",
                ));
            };
            let output = search_history(&repo_path, query, args.limit).await?;
            output_result(&output, args.response_format)
        }
        ContextAction::GetConfig => {
            let output = get_config(&repo_path, args.key.as_deref()).await?;
            output_result(&output, args.response_format)
        }
        ContextAction::SetConfig => {
            let (Some(key), Some(value)) = (&args.key, &args.value) else {
                return Err(GitError::invalid_input(
                    "key and value are required for context set_config.",
                ));
            };
            let output = set_config(&repo_path, key, value).await?;
            output_result(&output, args.response_format)
        }
        ContextAction::Aliases => {
            let git = Git::open(&repo_path)?;
            // An unset `alias.*` namespace makes git exit non-zero; that is not
            // an error here, it just means there are no aliases.
            let output = git
                .raw(&["config", "--get-regexp", r"^alias\."])
                .await
                .unwrap_or_default();
            let output = if output.trim().is_empty() {
                "No aliases configured.".to_owned()
            } else {
                output.trim().to_owned()
            };
            output_result(&output, args.response_format)
        }
    }
}
