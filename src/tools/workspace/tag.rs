//! `git_tag` — list, create or delete tags.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;

use crate::error::GitError;
use crate::render::ResponseFormat;
use crate::services::advanced::{TagAction, TagOptions, run_tag};
use crate::tools::output_result;

/// Arguments accepted by `git_tag`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TagArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Tag operation.
    #[serde(default)]
    pub action: TagAction,
    /// Tag name.
    #[serde(default)]
    pub name: Option<String>,
    /// Commit the tag points at.
    #[serde(default)]
    pub target: Option<String>,
    /// Annotation message.
    #[serde(default)]
    pub message: Option<String>,
    /// Sign the tag. Defaults to the server `GIT_AUTO_SIGN_TAGS` setting.
    #[serde(default)]
    pub sign: Option<bool>,
    /// Signing key to use.
    #[serde(default)]
    pub signing_key: Option<String>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

/// Performs the requested tag operation.
pub async fn run(args: &TagArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let output = run_tag(
        &repo_path,
        &TagOptions {
            action: args.action,
            name: args.name.clone(),
            target: args.target.clone(),
            message: args.message.clone(),
            sign: args.sign,
            signing_key: args.signing_key.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}
