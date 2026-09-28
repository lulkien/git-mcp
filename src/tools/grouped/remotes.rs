//! `git_remotes` — listing, managing and transporting over remotes.

use std::path::Path;

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::config::config;
use crate::constants::error_templates;
use crate::error::{GitError, to_value};
use crate::git::Git;
use crate::render::{ResponseFormat, render_content};
use crate::security::{assert_safe_arg, assert_safe_ref};
use crate::services::remote::{
    FetchOptions, ManageRemoteOptions, PullOptions, PushOptions, RemoteManageAction, fetch_remote,
    list_remotes, manage_remote, pull_remote, push_remote,
};
use crate::tools::{ok_result, output_result};

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteAction {
    /// List remotes (default).
    #[default]
    List,
    /// Add, remove or re-point a remote.
    Manage,
    /// Fetch from a remote.
    Fetch,
    /// Pull from a remote.
    Pull,
    /// Push to a remote.
    Push,
}

/// Rebase strategy used by the advanced pull path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RebaseMode {
    /// Use the configured `pull.rebase` behaviour.
    #[default]
    Default,
    /// Rebase preserving merge commits.
    Merges,
    /// Interactive rebase.
    Interactive,
}

/// Arguments accepted by `git_remotes`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RemoteArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which remote operation to perform.
    #[serde(default)]
    pub action: RemoteAction,
    /// Operation performed by `action=manage`.
    #[serde(default)]
    pub remote_action: Option<RemoteManageAction>,
    /// Remote name.
    #[serde(default)]
    pub name: Option<String>,
    /// Remote URL.
    #[serde(default)]
    pub url: Option<String>,
    /// Remote to fetch, pull or push.
    #[serde(default)]
    pub remote: Option<String>,
    /// Branch to fetch, pull or push.
    #[serde(default)]
    pub branch: Option<String>,
    /// Explicit refspecs, used instead of the branch.
    #[serde(default)]
    pub refspecs: Option<Vec<String>>,
    /// Prune deleted remote-tracking refs when fetching.
    #[serde(default = "default_true")]
    pub prune: bool,
    /// Prune tags when fetching.
    #[serde(default)]
    pub prune_tags: bool,
    /// Negotiation tips for the fetch protocol.
    #[serde(default)]
    pub negotiation_tips: Option<Vec<String>>,
    /// Rebase instead of merging when pulling.
    #[serde(default)]
    pub rebase: bool,
    /// Rebase strategy for the advanced pull path.
    #[serde(default)]
    pub rebase_mode: RebaseMode,
    /// Refuse to pull unless it is a fast-forward.
    #[serde(default)]
    pub ff_only: bool,
    /// Set the upstream of the pushed branch.
    #[serde(default)]
    pub set_upstream: bool,
    /// Force push, but only when the remote ref matches the local one.
    #[serde(default)]
    pub force_with_lease: bool,
    /// Hard force push (requires `GIT_ALLOW_FORCE_PUSH=true`).
    #[serde(default)]
    pub force: bool,
    /// Bypass git hooks (requires `GIT_ALLOW_NO_VERIFY=true`).
    #[serde(default)]
    pub no_verify: bool,
    /// Push all tags.
    #[serde(default)]
    pub tags: bool,
    /// Options forwarded to the receive hook (`--push-option`).
    #[serde(default)]
    pub push_options: Option<Vec<String>>,
    /// Push all refs atomically.
    #[serde(default)]
    pub atomic: bool,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_true() -> bool {
    true
}

/// Performs the requested remote operation.
pub async fn run(args: &RemoteArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;
    let git = Git::open(&repo_path)?;

    match args.action {
        RemoteAction::List => run_list(args, &repo_path).await,
        RemoteAction::Manage => run_manage(args, &repo_path).await,
        RemoteAction::Fetch => run_fetch(args, &repo_path, &git).await,
        RemoteAction::Pull => run_pull(args, &repo_path, &git).await,
        RemoteAction::Push => run_push(args, &repo_path, &git).await,
    }
}

async fn run_list(args: &RemoteArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let remotes = list_remotes(repo_path).await?;
    let text = render_content(&to_value(&remotes)?, args.response_format)?;
    Ok(ok_result(text, json!({ "remotes": remotes })))
}

async fn run_manage(args: &RemoteArgs, repo_path: &Path) -> Result<CallToolResult, GitError> {
    let (Some(remote_action), Some(name)) = (args.remote_action, &args.name) else {
        return Err(GitError::invalid_input(
            "remote_action and name are required for remotes manage.",
        ));
    };
    let output = manage_remote(
        repo_path,
        &ManageRemoteOptions {
            action: remote_action,
            name: name.clone(),
            url: args.url.clone(),
        },
    )
    .await?;
    output_result(&output, args.response_format)
}

async fn run_fetch(
    args: &RemoteArgs,
    repo_path: &Path,
    git: &Git,
) -> Result<CallToolResult, GitError> {
    let refspecs = args.refspecs.clone().unwrap_or_default();
    let tips = args.negotiation_tips.clone().unwrap_or_default();

    if refspecs.is_empty() && !args.prune_tags && tips.is_empty() {
        let output = fetch_remote(
            repo_path,
            &FetchOptions {
                remote: args.remote.clone(),
                branch: args.branch.clone(),
                prune: args.prune,
            },
        )
        .await?;
        return output_result(&output, args.response_format);
    }

    let mut raw = vec!["fetch".to_owned()];
    if args.prune {
        raw.push("--prune".to_owned());
    }
    if args.prune_tags {
        raw.push("--prune-tags".to_owned());
    }
    for tip in &tips {
        raw.push("--negotiation-tip".to_owned());
        raw.push(assert_safe_ref(tip, "negotiation_tip")?);
    }
    push_remote_and_refspecs(&mut raw, args, &refspecs)?;

    let output = git.raw(&raw).await?;
    output_result(
        &completed(output.trim(), "Fetch completed."),
        args.response_format,
    )
}

async fn run_pull(
    args: &RemoteArgs,
    repo_path: &Path,
    git: &Git,
) -> Result<CallToolResult, GitError> {
    let refspecs = args.refspecs.clone().unwrap_or_default();

    if refspecs.is_empty() && !args.ff_only && args.rebase_mode == RebaseMode::Default {
        let output = pull_remote(
            repo_path,
            &PullOptions {
                remote: args.remote.clone(),
                branch: args.branch.clone(),
                rebase: args.rebase,
            },
        )
        .await?;
        return output_result(&output, args.response_format);
    }

    let mut raw = vec!["pull".to_owned()];
    if args.rebase {
        match args.rebase_mode {
            RebaseMode::Merges => raw.push("--rebase=merges".to_owned()),
            RebaseMode::Interactive => raw.push("--rebase=interactive".to_owned()),
            RebaseMode::Default => raw.push("--rebase".to_owned()),
        }
    }
    if args.ff_only {
        raw.push("--ff-only".to_owned());
    }
    push_remote_and_refspecs(&mut raw, args, &refspecs)?;

    let output = git.raw(&raw).await?;
    output_result(
        &completed(output.trim(), "Pull completed."),
        args.response_format,
    )
}

async fn run_push(
    args: &RemoteArgs,
    repo_path: &Path,
    git: &Git,
) -> Result<CallToolResult, GitError> {
    validate_push_gates(args)?;

    let refspecs = args.refspecs.clone().unwrap_or_default();
    let push_options = args.push_options.clone().unwrap_or_default();

    if refspecs.is_empty() && push_options.is_empty() && !args.atomic {
        let output = push_remote(
            repo_path,
            &PushOptions {
                remote: args.remote.clone(),
                branch: args.branch.clone(),
                set_upstream: args.set_upstream,
                force_with_lease: args.force_with_lease,
                force: args.force,
                no_verify: args.no_verify,
                tags: args.tags,
            },
        )
        .await?;
        return output_result(&output, args.response_format);
    }

    let mut raw = vec!["push".to_owned()];
    for (enabled, flag) in [
        (args.set_upstream, "--set-upstream"),
        (args.force_with_lease, "--force-with-lease"),
        (args.force, "--force"),
        (args.tags, "--tags"),
        (args.no_verify, "--no-verify"),
        (args.atomic, "--atomic"),
    ] {
        if enabled {
            raw.push(flag.to_owned());
        }
    }
    for option in &push_options {
        raw.push(format!(
            "--push-option={}",
            assert_safe_arg(option, "push_option")?
        ));
    }
    push_remote_and_refspecs(&mut raw, args, &refspecs)?;

    let output = git.raw(&raw).await?;
    output_result(
        &completed(output.trim(), "Push completed."),
        args.response_format,
    )
}

/// Appends the remote and either the explicit refspecs or the single branch.
fn push_remote_and_refspecs(
    raw: &mut Vec<String>,
    args: &RemoteArgs,
    refspecs: &[String],
) -> Result<(), GitError> {
    if let Some(remote) = &args.remote {
        raw.push(assert_safe_arg(remote, "remote")?);
    }
    if !refspecs.is_empty() {
        for refspec in refspecs {
            raw.push(assert_safe_arg(refspec, "refspec")?);
        }
    } else if let Some(branch) = &args.branch {
        raw.push(assert_safe_ref(branch, "branch")?);
    }
    Ok(())
}

/// Applies the server-level gates that guard destructive push options.
fn validate_push_gates(args: &RemoteArgs) -> Result<(), GitError> {
    let server_config = config();
    if args.force && !server_config.allow_force_push {
        return Err(GitError::invalid_input(
            "force push is disabled on this server. Set GIT_ALLOW_FORCE_PUSH=true to enable it. \
Consider using force_with_lease instead for a safer alternative.",
        ));
    }
    if args.no_verify && !server_config.allow_no_verify {
        return Err(GitError::invalid_input(
            error_templates::HOOK_BYPASS_DISABLED,
        ));
    }
    Ok(())
}

fn completed(value: &str, placeholder: &str) -> String {
    if value.is_empty() {
        placeholder.to_owned()
    } else {
        value.to_owned()
    }
}
