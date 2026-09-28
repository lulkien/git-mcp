//! `git_status` — status, diff and "diff against main" actions.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::git::Git;
use crate::render::{ResponseFormat, render_content};
use crate::services::inspect::{DiffMode, GitDiffOptions, get_diff, get_diff_summary, get_status};
use crate::tools::ok_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StatusAction {
    /// Report working tree state (default).
    #[default]
    Status,
    /// Diff the working tree or two refs.
    Diff,
    /// Diff the current branch against `base_branch`.
    DiffMain,
}

/// Arguments accepted by `git_status`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StatusArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which status view to produce.
    #[serde(default)]
    pub action: StatusAction,
    /// Diff base used by `action=diff`.
    #[serde(default)]
    pub mode: DiffMode,
    /// Left-hand ref, required when `mode=refs`.
    #[serde(default)]
    pub from_ref: Option<String>,
    /// Right-hand ref, required when `mode=refs`.
    #[serde(default)]
    pub to_ref: Option<String>,
    /// Skip dependency directories and binary/resource files in diffs.
    #[serde(default)]
    pub filtered: bool,
    /// Base branch used by `action=diff_main`.
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_base_branch() -> String {
    "main".to_owned()
}

/// Reports working tree state, a diff, or the diff against a base branch.
pub async fn run(args: &StatusArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    match args.action {
        StatusAction::Status => {
            let status = get_status(&repo_path).await?;
            let text = render_content(&to_value(&status)?, args.response_format)?;
            Ok(ok_result(text, json!({ "status": status })))
        }
        StatusAction::Diff => {
            let options = GitDiffOptions {
                mode: args.mode,
                from_ref: args.from_ref.clone(),
                to_ref: args.to_ref.clone(),
                filtered: args.filtered,
            };
            let (summary, output) = tokio::join!(
                get_diff_summary(&repo_path, &options),
                get_diff(&repo_path, &options)
            );
            let payload = json!({ "summary": summary?, "output": output? });
            let text = render_content(&payload, args.response_format)?;
            Ok(ok_result(text, payload))
        }
        StatusAction::DiffMain => {
            let git = Git::open(&repo_path)?;
            let merge_base = git
                .raw(&["merge-base", "HEAD", args.base_branch.as_str()])
                .await?
                .trim()
                .to_owned();
            let options = GitDiffOptions {
                mode: DiffMode::Refs,
                from_ref: Some(merge_base.clone()),
                to_ref: Some("HEAD".to_owned()),
                filtered: false,
            };
            let (summary, output) = tokio::join!(
                get_diff_summary(&repo_path, &options),
                get_diff(&repo_path, &options)
            );
            let payload = json!({
                "base_branch": args.base_branch,
                "merge_base": merge_base,
                "summary": summary?,
                "output": output?,
            });
            let text = render_content(&payload, args.response_format)?;
            Ok(ok_result(text, payload))
        }
    }
}
