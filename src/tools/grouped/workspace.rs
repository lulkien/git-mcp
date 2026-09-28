//! `git_workspace` — stash, rebase, cherry-pick, merge, bisect, tag, worktree
//! and submodule actions.

use std::path::Path;

use rmcp::model::CallToolResult;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::error::{GitError, to_value};
use crate::render::{ResponseFormat, render_content};
use crate::services::advanced::{
    BisectAction, BisectOptions, StashAction, StashOptions, TagAction, TagOptions, run_bisect,
    run_stash, run_tag,
};
use crate::services::workspace::{
    CherryPickAction, CherryPickOptions, ConflictStyle, MergeAction, MergeOptions, RebaseAction,
    RebaseOptions, SubmoduleAction, SubmoduleOptions, WorktreeAction, WorktreeOptions,
    run_cherry_pick, run_merge, run_rebase, run_submodule, run_worktree,
};
use crate::tools::ok_result;

/// Action selected by the `action` parameter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAction {
    /// Run a stash operation (default).
    #[default]
    Stash,
    /// Stash everything, untracked files included.
    StashAll,
    /// Rebase the current branch.
    Rebase,
    /// Cherry-pick commits.
    CherryPick,
    /// Merge references.
    Merge,
    /// Drive a bisect session.
    Bisect,
    /// Manage tags.
    Tag,
    /// Manage worktrees.
    Worktree,
    /// Manage submodules.
    Submodule,
}

/// Arguments accepted by `git_workspace`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WorkspaceArgs {
    /// Absolute path to the local Git repository. If omitted, falls back to the
    /// server default set via the `GIT_REPO_PATH` environment variable or
    /// `--repo-path` CLI argument.
    #[serde(default)]
    pub repo_path: Option<String>,
    /// Which workspace operation to perform.
    #[serde(default)]
    pub action: WorkspaceAction,
    /// Stash operation.
    #[serde(default)]
    pub stash_action: Option<StashAction>,
    /// Stash or tag message.
    #[serde(default)]
    pub message: Option<String>,
    /// Stash index.
    #[serde(default)]
    pub index: Option<usize>,
    /// Include untracked files when stashing.
    #[serde(default)]
    pub include_untracked: bool,
    /// Rebase operation.
    #[serde(default)]
    pub rebase_action: Option<RebaseAction>,
    /// Rebase interactively.
    #[serde(default)]
    pub rebase_interactive: bool,
    /// Move fixup and squash commits into place.
    #[serde(default)]
    pub rebase_autosquash: bool,
    /// Preserve merge commits while rebasing.
    #[serde(default)]
    pub rebase_merges: bool,
    /// New base for `--onto`.
    #[serde(default)]
    pub rebase_onto: Option<String>,
    /// Upstream to rebase onto.
    #[serde(default)]
    pub rebase_upstream: Option<String>,
    /// Branch to rebase.
    #[serde(default)]
    pub rebase_branch: Option<String>,
    /// Cherry-pick operation.
    #[serde(default)]
    pub cherry_pick_action: Option<CherryPickAction>,
    /// Commits to cherry-pick.
    #[serde(default)]
    pub cherry_pick_refs: Option<Vec<String>>,
    /// Parent number when cherry-picking a merge commit.
    #[serde(default)]
    pub cherry_pick_mainline: Option<u32>,
    /// Record the source commit in the message.
    #[serde(default)]
    pub cherry_pick_record_origin: bool,
    /// Apply without committing.
    #[serde(default)]
    pub cherry_pick_no_commit: bool,
    /// Cherry-pick merge strategy.
    #[serde(default)]
    pub cherry_pick_strategy: Option<String>,
    /// Cherry-pick strategy options.
    #[serde(default)]
    pub cherry_pick_strategy_options: Option<Vec<String>>,
    /// Merge operation.
    #[serde(default)]
    pub merge_action: Option<MergeAction>,
    /// Commits to merge.
    #[serde(default)]
    pub merge_refs: Option<Vec<String>>,
    /// Always create a merge commit.
    #[serde(default)]
    pub merge_no_ff: bool,
    /// Refuse anything but a fast-forward.
    #[serde(default)]
    pub merge_ff_only: bool,
    /// Squash the merge into a single staged change.
    #[serde(default)]
    pub merge_squash: bool,
    /// Stage the merge result without committing.
    #[serde(default)]
    pub merge_no_commit: bool,
    /// Include a shortlog of merged commits.
    #[serde(default)]
    pub merge_log: bool,
    /// Merge strategy.
    #[serde(default)]
    pub merge_strategy: Option<String>,
    /// Merge strategy options.
    #[serde(default)]
    pub merge_strategy_options: Option<Vec<String>>,
    /// Conflict marker style.
    #[serde(default)]
    pub conflict_style: Option<ConflictStyle>,
    /// Bisect operation.
    #[serde(default)]
    pub bisect_action: Option<BisectAction>,
    /// Tag operation.
    #[serde(default)]
    pub tag_action: Option<TagAction>,
    /// Worktree operation.
    #[serde(default)]
    pub worktree_action: Option<WorktreeAction>,
    /// Submodule operation.
    #[serde(default)]
    pub submodule_action: Option<SubmoduleAction>,
    /// Ref used by cherry-pick, merge and bisect.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// Alias for `rebase_onto`.
    #[serde(default)]
    pub onto: Option<String>,
    /// Known-good commit for `bisect start`.
    #[serde(default)]
    pub good_ref: Option<String>,
    /// Known-bad commit for `bisect start`.
    #[serde(default)]
    pub bad_ref: Option<String>,
    /// Single executable token for `bisect run`.
    #[serde(default)]
    pub command: Option<String>,
    /// Full argv for `bisect run`.
    #[serde(default)]
    pub command_args: Option<Vec<String>>,
    /// Tag or remote name.
    #[serde(default)]
    pub name: Option<String>,
    /// Commit a tag points at.
    #[serde(default)]
    pub target: Option<String>,
    /// Sign the tag. Defaults to the server `GIT_AUTO_SIGN_TAGS` setting.
    #[serde(default)]
    pub sign: Option<bool>,
    /// Signing key to use.
    #[serde(default)]
    pub signing_key: Option<String>,
    /// Worktree or submodule path.
    #[serde(default)]
    pub path: Option<String>,
    /// Paths for `worktree repair` and `submodule update`.
    #[serde(default)]
    pub paths: Option<Vec<String>>,
    /// Branch for worktree and submodule operations.
    #[serde(default)]
    pub branch: Option<String>,
    /// URL for `submodule add`.
    #[serde(default)]
    pub url: Option<String>,
    /// Operate recursively.
    #[serde(default = "default_true")]
    pub recursive: bool,
    /// Force a worktree operation.
    #[serde(default)]
    pub worktree_force: bool,
    /// Create the worktree with a detached HEAD.
    #[serde(default)]
    pub worktree_detached: bool,
    /// Reason recorded when locking a worktree.
    #[serde(default)]
    pub worktree_lock_reason: Option<String>,
    /// Expiry for `worktree prune`.
    #[serde(default)]
    pub worktree_expire: Option<String>,
    /// Track the remote branch when updating submodules.
    #[serde(default)]
    pub submodule_remote: bool,
    /// Clone depth for submodules.
    #[serde(default)]
    pub submodule_depth: Option<u32>,
    /// Parallel jobs for submodule operations.
    #[serde(default)]
    pub submodule_jobs: Option<u32>,
    /// Output format for the response.
    #[serde(default)]
    pub response_format: ResponseFormat,
}

fn default_true() -> bool {
    true
}

/// Performs the requested workspace operation.
pub async fn run(args: &WorkspaceArgs) -> Result<CallToolResult, GitError> {
    let repo_path = crate::config::resolve_repo_path(args.repo_path.as_deref())?;

    let output = match args.action {
        WorkspaceAction::Stash => stash_action(args, &repo_path).await?,
        WorkspaceAction::StashAll => stash_all_action(args, &repo_path).await?,
        WorkspaceAction::Rebase => rebase_action(args, &repo_path).await?,
        WorkspaceAction::CherryPick => cherry_pick_action(args, &repo_path).await?,
        WorkspaceAction::Merge => merge_action(args, &repo_path).await?,
        WorkspaceAction::Bisect => bisect_action(args, &repo_path).await?,
        WorkspaceAction::Tag => tag_action(args, &repo_path).await?,
        WorkspaceAction::Worktree => worktree_action(args, &repo_path).await?,
        WorkspaceAction::Submodule => submodule_action(args, &repo_path).await?,
    };

    let text = render_content(&to_value(&output)?, args.response_format)?;
    Ok(ok_result(text, json!({ "output": output })))
}

async fn stash_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_stash(
        repo_path,
        &StashOptions {
            action: args.stash_action.unwrap_or_default(),
            message: args.message.clone(),
            index: args.index,
            include_untracked: args.include_untracked,
        },
    )
    .await
}

async fn stash_all_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_stash(
        repo_path,
        &StashOptions {
            action: StashAction::Save,
            message: args.message.clone(),
            index: None,
            include_untracked: true,
        },
    )
    .await
}

async fn rebase_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_rebase(
        repo_path,
        &RebaseOptions {
            action: args.rebase_action.unwrap_or_default(),
            interactive: args.rebase_interactive,
            autosquash: args.rebase_autosquash,
            merges: args.rebase_merges,
            onto: args.rebase_onto.clone().or_else(|| args.onto.clone()),
            upstream: args.rebase_upstream.clone(),
            branch: args.rebase_branch.clone(),
        },
    )
    .await
}

async fn cherry_pick_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_cherry_pick(
        repo_path,
        &CherryPickOptions {
            action: args.cherry_pick_action.unwrap_or_default(),
            refs: refs_or_single(args.cherry_pick_refs.as_deref(), args.ref_name.as_deref()),
            mainline: args.cherry_pick_mainline,
            record_origin: args.cherry_pick_record_origin,
            no_commit: args.cherry_pick_no_commit,
            strategy: args.cherry_pick_strategy.clone(),
            strategy_options: args.cherry_pick_strategy_options.clone(),
        },
    )
    .await
}

async fn merge_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_merge(
        repo_path,
        &MergeOptions {
            action: args.merge_action.unwrap_or_default(),
            refs: refs_or_single(args.merge_refs.as_deref(), args.ref_name.as_deref()),
            no_ff: args.merge_no_ff,
            ff_only: args.merge_ff_only,
            squash: args.merge_squash,
            no_commit: args.merge_no_commit,
            log: args.merge_log,
            strategy: args.merge_strategy.clone(),
            strategy_options: args.merge_strategy_options.clone(),
            conflict_style: args.conflict_style,
        },
    )
    .await
}

async fn bisect_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_bisect(
        repo_path,
        &BisectOptions {
            action: args.bisect_action.unwrap_or_default(),
            reference: args.ref_name.clone(),
            good_ref: args.good_ref.clone(),
            bad_ref: args.bad_ref.clone(),
            command: args.command.clone(),
            command_args: args.command_args.clone(),
        },
    )
    .await
}

async fn tag_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_tag(
        repo_path,
        &TagOptions {
            action: args.tag_action.unwrap_or_default(),
            name: args.name.clone(),
            target: args.target.clone(),
            message: args.message.clone(),
            sign: args.sign,
            signing_key: args.signing_key.clone(),
        },
    )
    .await
}

async fn worktree_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_worktree(
        repo_path,
        &WorktreeOptions {
            action: args.worktree_action.unwrap_or_default(),
            path: args.path.clone(),
            paths: args.paths.clone(),
            branch: args.branch.clone(),
            force: args.worktree_force,
            detached: args.worktree_detached,
            lock_reason: args.worktree_lock_reason.clone(),
            expire: args.worktree_expire.clone(),
        },
    )
    .await
}

async fn submodule_action(args: &WorkspaceArgs, repo_path: &Path) -> Result<String, GitError> {
    run_submodule(
        repo_path,
        &SubmoduleOptions {
            action: args.submodule_action.unwrap_or_default(),
            url: args.url.clone(),
            path: args.path.clone(),
            paths: args.paths.clone(),
            branch: args.branch.clone(),
            recursive: args.recursive,
            remote: args.submodule_remote,
            depth: args.submodule_depth,
            jobs: args.submodule_jobs,
        },
    )
    .await
}

/// Uses the explicit ref list when present, otherwise the single `ref`.
fn refs_or_single(refs: Option<&[String]>, reference: Option<&str>) -> Vec<String> {
    match refs {
        Some(refs) => refs.to_vec(),
        None => reference.map(str::to_owned).into_iter().collect(),
    }
}
