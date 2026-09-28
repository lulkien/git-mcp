//! `git_branches` — branch listing and lifecycle operations.

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};
use crate::services::branch::{
    CreateBranchOptions, DeleteBranchOptions, checkout_ref, create_branch, delete_branch,
    list_branches, recent_branches, rename_branch, set_upstream,
};
use crate::tools::{ok_result, output_result};

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BranchAction {
    /// List branches (default).
    #[default]
    List,
    /// Create a branch.
    Create,
    /// Delete a branch.
    Delete,
    /// Rename a branch.
    Rename,
    /// Check out a ref.
    Checkout,
    /// Point a branch at an upstream.
    SetUpstream,
    /// List branches by most recent commit.
    Recent,
}

/// Arguments accepted by `git_branches`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BranchArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which branch operation to perform.
    #[serde(default)]
    pub action: BranchAction,
    /// Include remote-tracking branches when listing.
    #[serde(default)]
    pub all: bool,
    /// Branch name.
    #[serde(default)]
    pub name: Option<String>,
    /// Existing branch name for rename.
    #[serde(default)]
    pub old_name: Option<String>,
    /// New branch name for rename.
    #[serde(default)]
    pub new_name: Option<String>,
    /// Starting point for a new branch.
    #[serde(default)]
    pub from_ref: Option<String>,
    /// Ref to check out.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// Create the branch when checking out.
    #[serde(default)]
    pub create: bool,
    /// Force branch deletion.
    #[serde(default)]
    pub force: bool,
    /// Branch to set the upstream of.
    #[serde(default)]
    pub branch: Option<String>,
    /// Upstream to track.
    #[serde(default)]
    pub upstream: Option<String>,
    /// Number of branches returned by `recent`.
    #[serde(default = "default_count")]
    pub count: usize,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_count() -> usize {
    10
}

/// Performs the requested branch operation.
pub async fn run(args: &BranchArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    match args.action {
        BranchAction::List => {
            let branches = list_branches(&repo_path, args.all).await?;
            let text = render_content(&to_value(&branches)?, args.response_format)?;
            Ok(ok_result(text, json!({ "branches": branches })))
        }
        BranchAction::Create => {
            let Some(name) = args.name.clone() else {
                return Err(GitError::invalid_input(
                    "name is required for branch create.",
                ));
            };
            let output = create_branch(
                &repo_path,
                &CreateBranchOptions {
                    name,
                    from_ref: args.from_ref.clone(),
                    checkout: args.create,
                },
            )
            .await?;
            output_result(&output, args.response_format)
        }
        BranchAction::Delete => {
            let Some(name) = args.name.clone() else {
                return Err(GitError::invalid_input(
                    "name is required for branch delete.",
                ));
            };
            let output = delete_branch(
                &repo_path,
                &DeleteBranchOptions {
                    name,
                    force: args.force,
                },
            )
            .await?;
            output_result(&output, args.response_format)
        }
        BranchAction::Rename => {
            let (Some(old_name), Some(new_name)) = (&args.old_name, &args.new_name) else {
                return Err(GitError::invalid_input(
                    "old_name and new_name are required for branch rename.",
                ));
            };
            let output = rename_branch(&repo_path, old_name, new_name).await?;
            output_result(&output, args.response_format)
        }
        BranchAction::Checkout => {
            let Some(reference) = args.ref_name.clone() else {
                return Err(GitError::invalid_input(
                    "ref is required for branch checkout.",
                ));
            };
            let output = checkout_ref(&repo_path, &reference, args.create).await?;
            output_result(&output, args.response_format)
        }
        BranchAction::SetUpstream => {
            let (Some(branch), Some(upstream)) = (&args.branch, &args.upstream) else {
                return Err(GitError::invalid_input(
                    "branch and upstream are required for set_upstream.",
                ));
            };
            let output = set_upstream(&repo_path, branch, upstream).await?;
            output_result(&output, args.response_format)
        }
        BranchAction::Recent => {
            let output = recent_branches(&repo_path, args.count).await?;
            let output = if output.is_empty() {
                "No branches found.".to_owned()
            } else {
                output
            };
            output_result(&output, args.response_format)
        }
    }
}
