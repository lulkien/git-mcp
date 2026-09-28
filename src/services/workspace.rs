//! Workspace service: rebase, cherry-pick, merge, worktree and submodule
//! operations.

use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::GitError;
use crate::git::{Git, validate_path_arguments};
use crate::security::{assert_safe_arg, assert_safe_ref};
use crate::services::advanced::{fallback, validate_worktree_path};

/// Action selected by the rebase operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RebaseAction {
    /// Start a rebase (default).
    #[default]
    Start,
    /// Resume the rebase after resolving conflicts.
    Continue,
    /// Abort the rebase.
    Abort,
    /// Skip the current commit.
    Skip,
}

/// Options for a rebase operation.
#[derive(Debug, Clone, Default)]
pub struct RebaseOptions {
    /// Operation to perform.
    pub action: RebaseAction,
    /// Rebase interactively.
    pub interactive: bool,
    /// Move fixup/squash commits into place.
    pub autosquash: bool,
    /// Preserve merge commits.
    pub merges: bool,
    /// New base for `--onto`.
    pub onto: Option<String>,
    /// Upstream to rebase onto.
    pub upstream: Option<String>,
    /// Branch to rebase.
    pub branch: Option<String>,
}

/// Action selected by the cherry-pick operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CherryPickAction {
    /// Cherry-pick commits (default).
    #[default]
    Start,
    /// Resume after resolving conflicts.
    Continue,
    /// Abort the cherry-pick.
    Abort,
}

/// Options for a cherry-pick operation.
#[derive(Debug, Clone, Default)]
pub struct CherryPickOptions {
    /// Operation to perform.
    pub action: CherryPickAction,
    /// Commits to cherry-pick.
    pub refs: Vec<String>,
    /// Parent number for a merge commit.
    pub mainline: Option<u32>,
    /// Record the source commit in the message.
    pub record_origin: bool,
    /// Apply without committing.
    pub no_commit: bool,
    /// Merge strategy.
    pub strategy: Option<String>,
    /// Strategy-specific options.
    pub strategy_options: Option<Vec<String>>,
}

/// Action selected by the merge operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MergeAction {
    /// Merge references (default).
    #[default]
    Start,
    /// Resume the merge after resolving conflicts.
    Continue,
    /// Abort the merge.
    Abort,
}

/// Conflict marker style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStyle {
    /// Two-way diff markers.
    Merge,
    /// Three-way diff markers.
    Diff3,
    /// Zealous three-way diff markers.
    Zdiff3,
}

impl ConflictStyle {
    /// Wire representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Diff3 => "diff3",
            Self::Zdiff3 => "zdiff3",
        }
    }
}

/// Options for a merge operation.
#[derive(Debug, Clone, Default)]
pub struct MergeOptions {
    /// Operation to perform.
    pub action: MergeAction,
    /// Commits to merge.
    pub refs: Vec<String>,
    /// Always create a merge commit.
    pub no_ff: bool,
    /// Refuse anything but a fast-forward.
    pub ff_only: bool,
    /// Squash the merge into a single staged change.
    pub squash: bool,
    /// Stage the result without committing.
    pub no_commit: bool,
    /// Include shortlog of merged commits.
    pub log: bool,
    /// Merge strategy.
    pub strategy: Option<String>,
    /// Strategy-specific options.
    pub strategy_options: Option<Vec<String>>,
    /// Conflict marker style.
    pub conflict_style: Option<ConflictStyle>,
}

/// Action selected by the worktree operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeAction {
    /// Add a worktree.
    Add,
    /// List worktrees (default).
    #[default]
    List,
    /// Remove a worktree.
    Remove,
    /// Lock a worktree.
    Lock,
    /// Unlock a worktree.
    Unlock,
    /// Prune administrative files for missing worktrees.
    Prune,
    /// Repair administrative files.
    Repair,
}

/// Options for a worktree operation.
#[derive(Debug, Clone, Default)]
pub struct WorktreeOptions {
    /// Operation to perform.
    pub action: WorktreeAction,
    /// Worktree path.
    pub path: Option<String>,
    /// Paths for `repair`.
    pub paths: Option<Vec<String>>,
    /// Branch checked out in the new worktree.
    pub branch: Option<String>,
    /// Force the operation.
    pub force: bool,
    /// Check out a detached HEAD.
    pub detached: bool,
    /// Reason recorded when locking.
    pub lock_reason: Option<String>,
    /// Expiry for `prune`.
    pub expire: Option<String>,
}

/// Action selected by the submodule operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubmoduleAction {
    /// Add a submodule.
    Add,
    /// List submodules (default).
    #[default]
    List,
    /// Initialise and update submodules.
    Update,
    /// Sync submodule URLs.
    Sync,
    /// Set the tracked branch of a submodule.
    SetBranch,
}

/// Options for a submodule operation.
#[derive(Debug, Clone, Default)]
pub struct SubmoduleOptions {
    /// Operation to perform.
    pub action: SubmoduleAction,
    /// Repository URL for `add`.
    pub url: Option<String>,
    /// Submodule path.
    pub path: Option<String>,
    /// Paths to limit `update` to.
    pub paths: Option<Vec<String>>,
    /// Branch to track for `set_branch`.
    pub branch: Option<String>,
    /// Operate recursively.
    pub recursive: bool,
    /// Track the remote branch when updating.
    pub remote: bool,
    /// Clone depth.
    pub depth: Option<u32>,
    /// Number of parallel jobs.
    pub jobs: Option<u32>,
}

/// Starts, continues, aborts or skips a rebase.
pub async fn run_rebase(repo_path: &Path, options: &RebaseOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        RebaseAction::Continue => {
            let output = git.raw(&["rebase", "--continue"]).await?;
            Ok(fallback(output.trim(), "Rebase completed."))
        }
        RebaseAction::Abort => {
            let output = git.raw(&["rebase", "--abort"]).await?;
            Ok(fallback(output.trim(), "Rebase completed."))
        }
        RebaseAction::Skip => {
            let output = git.raw(&["rebase", "--skip"]).await?;
            Ok(fallback(output.trim(), "Rebase completed."))
        }
        RebaseAction::Start => {
            let mut args = vec!["rebase".to_owned()];
            if options.interactive {
                args.push("-i".to_owned());
            }
            if options.autosquash {
                args.push("--autosquash".to_owned());
            }
            if options.merges {
                args.push("--rebase-merges".to_owned());
            }

            let onto = options
                .onto
                .as_deref()
                .map(|onto| assert_safe_ref(onto, "onto"))
                .transpose()?;
            if let Some(onto) = &onto {
                args.push("--onto".to_owned());
                args.push(onto.clone());
            }

            let upstream_ref = options
                .upstream
                .as_deref()
                .or(options.onto.as_deref())
                .ok_or_else(|| {
                    GitError::invalid_input(
                        "rebase_upstream (or onto) is required for rebase start.",
                    )
                })?;
            args.push(assert_safe_ref(upstream_ref, "rebase_upstream")?);

            if let Some(branch) = &options.branch {
                args.push(assert_safe_ref(branch, "rebase_branch")?);
            }

            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Rebase completed."))
        }
    }
}

/// Starts, continues or aborts a cherry-pick.
pub async fn run_cherry_pick(
    repo_path: &Path,
    options: &CherryPickOptions,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        CherryPickAction::Continue => {
            let output = git.raw(&["cherry-pick", "--continue"]).await?;
            Ok(fallback(output.trim(), "Cherry-pick completed."))
        }
        CherryPickAction::Abort => {
            let output = git.raw(&["cherry-pick", "--abort"]).await?;
            Ok(fallback(output.trim(), "Cherry-pick completed."))
        }
        CherryPickAction::Start => {
            let mut args = vec!["cherry-pick".to_owned()];
            if let Some(mainline) = options.mainline {
                args.push("--mainline".to_owned());
                args.push(mainline.to_string());
            }
            if options.record_origin {
                args.push("-x".to_owned());
            }
            if options.no_commit {
                args.push("--no-commit".to_owned());
            }
            if let Some(strategy) = &options.strategy {
                args.push("--strategy".to_owned());
                args.push(assert_safe_arg(strategy, "cherry_pick_strategy")?);
            }
            for option in options.strategy_options.clone().unwrap_or_default() {
                args.push("--strategy-option".to_owned());
                args.push(assert_safe_arg(&option, "cherry_pick_strategy_option")?);
            }

            if options.refs.is_empty() {
                return Err(GitError::invalid_input(
                    "ref or cherry_pick_refs is required for cherry_pick start.",
                ));
            }
            for reference in &options.refs {
                args.push(assert_safe_ref(reference, "ref")?);
            }

            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Cherry-pick completed."))
        }
    }
}

/// Starts, continues or aborts a merge.
pub async fn run_merge(repo_path: &Path, options: &MergeOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        MergeAction::Continue => {
            let output = git.raw(&["merge", "--continue"]).await?;
            Ok(fallback(output.trim(), "Merge completed."))
        }
        MergeAction::Abort => {
            let output = git.raw(&["merge", "--abort"]).await?;
            Ok(fallback(output.trim(), "Merge completed."))
        }
        MergeAction::Start => {
            let mut args = vec!["merge".to_owned()];
            for (enabled, flag) in [
                (options.no_ff, "--no-ff"),
                (options.ff_only, "--ff-only"),
                (options.squash, "--squash"),
                (options.no_commit, "--no-commit"),
                (options.log, "--log"),
            ] {
                if enabled {
                    args.push(flag.to_owned());
                }
            }
            if let Some(strategy) = &options.strategy {
                args.push("--strategy".to_owned());
                args.push(assert_safe_arg(strategy, "merge_strategy")?);
            }
            if let Some(style) = options.conflict_style {
                args.push(format!("--conflict={}", style.as_str()));
            }
            for option in options.strategy_options.clone().unwrap_or_default() {
                args.push("--strategy-option".to_owned());
                args.push(assert_safe_arg(&option, "merge_strategy_option")?);
            }

            if options.refs.is_empty() {
                return Err(GitError::invalid_input(
                    "ref or merge_refs is required for merge start.",
                ));
            }
            for reference in &options.refs {
                args.push(assert_safe_ref(reference, "ref")?);
            }

            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Merge completed."))
        }
    }
}

/// Manages linked worktrees.
pub async fn run_worktree(repo_path: &Path, options: &WorktreeOptions) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        WorktreeAction::List => list_worktrees(&git).await,
        WorktreeAction::Remove => remove_worktree(&git, options).await,
        WorktreeAction::Lock | WorktreeAction::Unlock => toggle_worktree_lock(&git, options).await,
        WorktreeAction::Prune => prune_worktrees(&git, options).await,
        WorktreeAction::Repair => repair_worktrees(&git, options).await,
        WorktreeAction::Add => add_worktree(&git, options).await,
    }
}

async fn list_worktrees(git: &Git) -> Result<String, GitError> {
    let output = git.raw(&["worktree", "list", "--porcelain"]).await?;
    Ok(fallback(output.trim(), "No worktrees."))
}

async fn remove_worktree(git: &Git, options: &WorktreeOptions) -> Result<String, GitError> {
    let path = required_worktree_path(options, "path is required for worktree remove.")?;
    let mut args = vec!["worktree".to_owned(), "remove".to_owned()];
    if options.force {
        args.push("--force".to_owned());
    }
    args.push(path.clone());
    let output = git.raw(&args).await?;
    Ok(fallback(
        output.trim(),
        &format!("Removed worktree {path}."),
    ))
}

async fn toggle_worktree_lock(git: &Git, options: &WorktreeOptions) -> Result<String, GitError> {
    let path = required_worktree_path(options, "path is required for worktree lock/unlock.")?;
    let op = if options.action == WorktreeAction::Lock {
        "lock"
    } else {
        "unlock"
    };
    let mut args = vec!["worktree".to_owned(), op.to_owned(), path.clone()];
    if options.action == WorktreeAction::Lock
        && let Some(reason) = &options.lock_reason
    {
        args.push("--reason".to_owned());
        args.push(reason.clone());
    }
    let output = git.raw(&args).await?;
    Ok(fallback(
        output.trim(),
        &format!("Worktree {op} completed for {path}."),
    ))
}

async fn prune_worktrees(git: &Git, options: &WorktreeOptions) -> Result<String, GitError> {
    let mut args = vec!["worktree".to_owned(), "prune".to_owned()];
    if let Some(expire) = &options.expire {
        args.push(format!("--expire={expire}"));
    }
    let output = git.raw(&args).await?;
    Ok(fallback(output.trim(), "Worktree prune completed."))
}

async fn repair_worktrees(git: &Git, options: &WorktreeOptions) -> Result<String, GitError> {
    let mut args = vec!["worktree".to_owned(), "repair".to_owned()];
    for path in options.paths.clone().unwrap_or_default() {
        args.push(validate_worktree_path(&path, "path")?);
    }
    let output = git.raw(&args).await?;
    Ok(fallback(output.trim(), "Worktree repair completed."))
}

async fn add_worktree(git: &Git, options: &WorktreeOptions) -> Result<String, GitError> {
    let path = required_worktree_path(options, "path is required for worktree add.")?;

    let mut args = vec!["worktree".to_owned(), "add".to_owned()];
    if options.force {
        args.push("--force".to_owned());
    }
    if options.detached {
        args.push("--detach".to_owned());
    }
    if let Some(reason) = &options.lock_reason {
        args.push("--lock".to_owned());
        args.push("--reason".to_owned());
        args.push(reason.clone());
    }
    args.push(path.clone());

    match (&options.branch, options.detached) {
        (Some(branch), _) => args.push(assert_safe_ref(branch, "branch")?),
        (None, false) => {
            return Err(GitError::invalid_input(
                "branch is required for worktree add unless worktree_detached=true.",
            ));
        }
        (None, true) => {}
    }

    let output = git.raw(&args).await?;
    Ok(fallback(
        output.trim(),
        &format!("Added worktree at {path}."),
    ))
}

fn required_worktree_path(options: &WorktreeOptions, message: &str) -> Result<String, GitError> {
    let Some(path) = &options.path else {
        return Err(GitError::invalid_input(message));
    };
    validate_worktree_path(path, "path")
}

/// Manages submodules.
pub async fn run_submodule(
    repo_path: &Path,
    options: &SubmoduleOptions,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;

    match options.action {
        SubmoduleAction::List => {
            let output = git.raw(&["submodule", "status"]).await?;
            Ok(fallback(output.trim(), "No submodules."))
        }
        SubmoduleAction::Sync => {
            let mut args = vec!["submodule".to_owned(), "sync".to_owned()];
            if options.recursive {
                args.push("--recursive".to_owned());
            }
            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Submodule sync complete."))
        }
        SubmoduleAction::Update => {
            let mut args = vec![
                "submodule".to_owned(),
                "update".to_owned(),
                "--init".to_owned(),
            ];
            if options.recursive {
                args.push("--recursive".to_owned());
            }
            if options.remote {
                args.push("--remote".to_owned());
            }
            if let Some(depth) = options.depth {
                args.push("--depth".to_owned());
                args.push(depth.to_string());
            }
            if let Some(jobs) = options.jobs {
                args.push("--jobs".to_owned());
                args.push(jobs.to_string());
            }
            let paths = options.paths.clone().unwrap_or_default();
            if !paths.is_empty() {
                args.push("--".to_owned());
                args.extend(validate_path_arguments(repo_path, &paths)?);
            }
            let output = git.raw(&args).await?;
            Ok(fallback(output.trim(), "Submodule update complete."))
        }
        SubmoduleAction::SetBranch => {
            let (Some(branch), Some(path)) = (&options.branch, &options.path) else {
                return Err(GitError::invalid_input(
                    "branch and path are required for submodule set_branch.",
                ));
            };
            let branch = assert_safe_ref(branch, "branch")?;
            let safe_path = validate_path_arguments(repo_path, std::slice::from_ref(path))?
                .into_iter()
                .next()
                .unwrap_or_default();
            let output = git
                .raw(&[
                    "submodule",
                    "set-branch",
                    "--branch",
                    &branch,
                    "--",
                    &safe_path,
                ])
                .await?;
            Ok(fallback(
                output.trim(),
                &format!("Set submodule {safe_path} branch to {branch}."),
            ))
        }
        SubmoduleAction::Add => {
            let (Some(url), Some(path)) = (&options.url, &options.path) else {
                return Err(GitError::invalid_input(
                    "url and path are required for submodule add.",
                ));
            };
            let url = assert_safe_arg(url, "url")?;
            let safe_path = validate_path_arguments(repo_path, std::slice::from_ref(path))?
                .into_iter()
                .next()
                .unwrap_or_default();
            let output = git.raw(&["submodule", "add", &url, &safe_path]).await?;
            Ok(fallback(
                output.trim(),
                &format!("Added submodule {safe_path}."),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_styles_match_the_wire_names() {
        assert_eq!(ConflictStyle::Merge.as_str(), "merge");
        assert_eq!(ConflictStyle::Diff3.as_str(), "diff3");
        assert_eq!(ConflictStyle::Zdiff3.as_str(), "zdiff3");
    }

    #[test]
    fn action_wire_names_are_snake_case() {
        assert_eq!(
            serde_json::to_value(WorktreeAction::Repair).unwrap(),
            serde_json::json!("repair")
        );
        assert_eq!(
            serde_json::to_value(SubmoduleAction::SetBranch).unwrap(),
            serde_json::json!("set_branch")
        );
        assert_eq!(
            serde_json::to_value(MergeAction::Start).unwrap(),
            serde_json::json!("start")
        );
    }
}
