//! `git_history` — log, show, reflog, blame, graph and contributor views.

use std::path::Path;

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::git::{Git, validate_path_arguments};
use crate::render::{ResponseFormat, render_content};
use crate::services::inspect::{
    GitLogOptions, blame_file, build_log_args, get_reflog, parse_commit_log, show_ref,
};
use crate::tools::ok_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    /// Commit log (default).
    #[default]
    Log,
    /// Single commit, patch and stats.
    Show,
    /// Reference log.
    Reflog,
    /// Line attribution for a file.
    Blame,
    /// One-line decorated commit graph.
    Lg,
    /// Contributors by commit count.
    Who,
}

/// Ordering applied to `log` output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LogOrder {
    /// Git's default ordering.
    #[default]
    Default,
    /// Topological order.
    Topo,
    /// Commit-date order.
    Date,
    /// Author-date order.
    AuthorDate,
}

impl LogOrder {
    fn as_arg(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::Topo => Some("--topo-order"),
            Self::Date => Some("--date-order"),
            Self::AuthorDate => Some("--author-date-order"),
        }
    }
}

/// Arguments accepted by `git_history`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HistoryArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which history view to produce.
    #[serde(default)]
    pub action: HistoryAction,
    /// Maximum number of commits to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Number of commits to skip.
    #[serde(default)]
    pub offset: usize,
    /// Follow only the first parent of merge commits.
    #[serde(default)]
    pub first_parent: bool,
    /// Skip merge commits.
    #[serde(default)]
    pub no_merges: bool,
    /// Include all branches.
    #[serde(default)]
    pub all_branches: bool,
    /// Simplify history by removing some merges.
    #[serde(default)]
    pub simplify_merges: bool,
    /// Commit ordering.
    #[serde(default)]
    pub order: LogOrder,
    /// Explicit revision range, e.g. `main..feature`.
    #[serde(default)]
    pub revision_range: Option<String>,
    /// Repository-relative paths to restrict history to.
    #[serde(default)]
    pub pathspecs: Option<Vec<String>>,
    /// Author filter.
    #[serde(default)]
    pub author: Option<String>,
    /// Message grep filter.
    #[serde(default)]
    pub grep: Option<String>,
    /// Lower bound on commit date.
    #[serde(default)]
    pub since: Option<String>,
    /// Upper bound on commit date.
    #[serde(default)]
    pub until: Option<String>,
    /// Single repository-relative path to restrict history to.
    #[serde(default)]
    pub file_path: Option<String>,
    /// Ref used by `show` and `blame`.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_limit() -> usize {
    30
}

/// Produces the requested history view.
pub async fn run(args: &HistoryArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let git = Git::open(&repo_path)?;

    match args.action {
        HistoryAction::Log => run_log(args, &repo_path, &git).await,
        HistoryAction::Show => run_show(args, &repo_path).await,
        HistoryAction::Reflog => run_reflog(args, &repo_path).await,
        HistoryAction::Blame => run_blame(args, &repo_path).await,
        HistoryAction::Lg => run_lg(args, &git).await,
        HistoryAction::Who => run_who(args, &repo_path, &git).await,
    }
}

fn log_options(args: &HistoryArgs) -> GitLogOptions {
    GitLogOptions {
        limit: args.limit,
        offset: args.offset,
        author: args.author.clone(),
        grep: args.grep.clone(),
        since: args.since.clone(),
        until: args.until.clone(),
        file_path: None,
    }
}

async fn run_log(
    args: &HistoryArgs,
    repo_path: &Path,
    git: &Git,
) -> Result<CallToolResult, GitError> {
    let mut git_args = build_log_args(&log_options(args));

    for (enabled, flag) in [
        (args.first_parent, "--first-parent"),
        (args.no_merges, "--no-merges"),
        (args.all_branches, "--all"),
        (args.simplify_merges, "--simplify-merges"),
    ] {
        if enabled {
            git_args.push(flag.to_owned());
        }
    }
    if let Some(order) = args.order.as_arg() {
        git_args.push(order.to_owned());
    }
    if let Some(revision_range) = &args.revision_range {
        git_args.push(revision_range.clone());
    }

    let mut pathspecs: Vec<String> = args.pathspecs.clone().unwrap_or_default();
    if let Some(file_path) = &args.file_path {
        pathspecs.push(file_path.clone());
    }
    let pathspecs = validate_path_arguments(repo_path, &pathspecs)?;
    if !pathspecs.is_empty() {
        git_args.push("--".to_owned());
        git_args.extend(pathspecs);
    }

    let output = git.raw(&git_args).await?;
    let commits = parse_commit_log(&output);
    let text = render_content(&to_value(&commits)?, args.response_format)?;
    Ok(ok_result(text, json!({ "commits": commits })))
}

async fn run_show(args: &HistoryArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let Some(reference) = &args.ref_name else {
        return Err(GitError::invalid_input("ref is required for history show."));
    };
    let output = show_ref(repo_path, reference).await?;
    let text = render_content(&to_value(&output)?, args.response_format)?;
    Ok(ok_result(
        text,
        json!({ "ref": reference, "output": output }),
    ))
}

async fn run_reflog(args: &HistoryArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let output = get_reflog(repo_path, args.limit).await?;
    let text = render_content(&to_value(&output)?, args.response_format)?;
    Ok(ok_result(text, json!({ "output": output })))
}

async fn run_blame(args: &HistoryArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let Some(file_path) = &args.file_path else {
        return Err(GitError::invalid_input(
            "file_path is required for history blame.",
        ));
    };
    let output = blame_file(repo_path, file_path, args.ref_name.as_deref()).await?;
    let text = render_content(&to_value(&output)?, args.response_format)?;
    Ok(ok_result(
        text,
        json!({ "file_path": file_path, "ref": args.ref_name, "output": output }),
    ))
}

async fn run_lg(args: &HistoryArgs, git: &Git) -> Result<CallToolResult, GitError> {
    let output = git
        .raw(&[
            "log",
            "--oneline",
            "--graph",
            "--decorate",
            "--all",
            "--abbrev-commit",
        ])
        .await?;
    let output = fallback(output.trim(), "No commits.");
    let text = render_content(&to_value(&output)?, args.response_format)?;
    Ok(ok_result(text, json!({ "output": output })))
}

async fn run_who(
    args: &HistoryArgs,
    repo_path: &Path,
    git: &Git,
) -> Result<CallToolResult, GitError> {
    let mut git_args = vec![
        "shortlog".to_owned(),
        "-s".to_owned(),
        "-n".to_owned(),
        "--all".to_owned(),
        "--no-merges".to_owned(),
    ];
    if let Some(file_path) = &args.file_path {
        git_args.push("--".to_owned());
        git_args.extend(validate_path_arguments(
            repo_path,
            std::slice::from_ref(file_path),
        )?);
    }
    let output = git.raw(&git_args).await?;
    let output = fallback(output.trim(), "No contributors found.");
    let text = render_content(&to_value(&output)?, args.response_format)?;
    Ok(ok_result(
        text,
        json!({ "file_path": args.file_path, "output": output }),
    ))
}

fn fallback(value: &str, placeholder: &str) -> String {
    if value.is_empty() {
        placeholder.to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_order_maps_to_git_flags() {
        assert_eq!(LogOrder::Default.as_arg(), None);
        assert_eq!(LogOrder::Topo.as_arg(), Some("--topo-order"));
        assert_eq!(LogOrder::Date.as_arg(), Some("--date-order"));
        assert_eq!(LogOrder::AuthorDate.as_arg(), Some("--author-date-order"));
    }

    #[test]
    fn wire_names_match_the_documented_schema() {
        for (order, expected) in [
            (LogOrder::Default, "default"),
            (LogOrder::Topo, "topo"),
            (LogOrder::Date, "date"),
            (LogOrder::AuthorDate, "author-date"),
        ] {
            assert_eq!(serde_json::to_value(order).unwrap(), json!(expected));
        }
        assert_eq!(
            serde_json::to_value(HistoryAction::Lg).unwrap(),
            json!("lg")
        );
        assert_eq!(
            serde_json::to_value(HistoryAction::Blame).unwrap(),
            json!("blame")
        );
    }
}
