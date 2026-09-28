//! Branch service: listing, creating, deleting, renaming, checkout, upstream.

use std::path::{Path, PathBuf};

use crate::error::GitError;
use crate::git::Git;
use crate::security::{assert_safe_arg, assert_safe_ref};
use crate::types::BranchInfo;

/// Options for creating a branch.
#[derive(Debug, Clone)]
pub struct CreateBranchOptions {
    /// Branch name.
    pub name: String,
    /// Starting point.
    pub from_ref: Option<String>,
    /// Check the branch out after creating it.
    pub checkout: bool,
}

/// Options for deleting a branch.
#[derive(Debug, Clone)]
pub struct DeleteBranchOptions {
    /// Branch name.
    pub name: String,
    /// Delete even when unmerged.
    pub force: bool,
}

/// Lists branches with their tip commit and upstream.
pub async fn list_branches(repo_path: &Path, all: bool) -> Result<Vec<BranchInfo>, GitError> {
    let git = Git::open(repo_path)?;

    let mut args = vec![
        "branch".to_owned(),
        "--format=%(refname:short)%09%(objectname:short)%09%(HEAD)%09%(upstream:short)".to_owned(),
    ];
    if all {
        args.push("-a".to_owned());
    }

    let output = git.raw(&args).await?;
    Ok(parse_branch_list(&output))
}

/// Parses `git branch --format` output into branch records.
#[must_use]
pub fn parse_branch_list(output: &str) -> Vec<BranchInfo> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next().unwrap_or_default().to_owned();
            let commit = fields.next().unwrap_or_default();
            let marker = fields.next().unwrap_or_default();
            let upstream = fields.next().unwrap_or_default();
            BranchInfo {
                name,
                is_current: marker == "*",
                commit: (!commit.is_empty()).then(|| commit.to_owned()),
                upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
            }
        })
        .collect()
}

/// Creates a branch, optionally checking it out.
pub async fn create_branch(
    repo_path: &Path,
    options: &CreateBranchOptions,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let name = assert_safe_arg(&options.name, "branch name")?;
    let from_ref = options
        .from_ref
        .as_deref()
        .map(|reference| assert_safe_ref(reference, "from_ref"))
        .transpose()?;

    if let Some(from_ref) = from_ref {
        if options.checkout {
            git.raw(&["checkout", "-b", &name, &from_ref]).await?;
        } else {
            git.raw(&["branch", &name, &from_ref]).await?;
        }
        return Ok(if options.checkout {
            format!("Created and checked out {name} from {from_ref}.")
        } else {
            format!("Created branch {name} from {from_ref}.")
        });
    }

    git.raw(&["branch", &name]).await?;
    if options.checkout {
        git.raw(&["checkout", &name]).await?;
    }

    Ok(if options.checkout {
        format!("Created and checked out {name}.")
    } else {
        format!("Created branch {name}.")
    })
}

/// Deletes a local branch.
pub async fn delete_branch(
    repo_path: &Path,
    options: &DeleteBranchOptions,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let name = assert_safe_arg(&options.name, "branch name")?;
    let flag = if options.force { "-D" } else { "-d" };
    git.raw(&["branch", flag, &name]).await?;
    Ok(format!("Deleted branch {name}."))
}

/// Renames a branch.
pub async fn rename_branch(
    repo_path: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let old_safe = assert_safe_arg(old_name, "old branch name")?;
    let new_safe = assert_safe_arg(new_name, "new branch name")?;
    git.raw(&["branch", "-m", &old_safe, &new_safe]).await?;
    Ok(format!("Renamed branch {old_safe} to {new_safe}."))
}

/// Checks out a ref, optionally creating a branch.
pub async fn checkout_ref(
    repo_path: &Path,
    reference: &str,
    create: bool,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let safe_ref = assert_safe_ref(reference, "ref")?;

    if create {
        git.raw(&["checkout", "-b", &safe_ref]).await?;
        return Ok(format!("Created and checked out {safe_ref}."));
    }

    git.raw(&["checkout", &safe_ref]).await?;
    Ok(format!("Checked out {safe_ref}."))
}

/// Points a branch at an upstream.
pub async fn set_upstream(
    repo_path: &Path,
    branch: &str,
    upstream: &str,
) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let branch_safe = assert_safe_arg(branch, "branch")?;
    let upstream_safe = assert_safe_ref(upstream, "upstream")?;
    git.raw(&["branch", "--set-upstream-to", &upstream_safe, &branch_safe])
        .await?;
    Ok(format!("Set upstream of {branch_safe} to {upstream_safe}."))
}

/// Lists the most recently committed branches.
///
/// `git branch` has no `--count` option, so the limit is applied to the
/// formatted output instead.
pub async fn recent_branches(repo_path: &PathBuf, count: usize) -> Result<String, GitError> {
    let git = Git::open(repo_path)?;
    let output = git
        .raw(&[
            "branch",
            "--sort=-committerdate",
            "--format=%(refname:short) (%(committerdate:relative))",
        ])
        .await?;

    Ok(output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(count)
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_listing() {
        let output = "main\tabc1234\t*\torigin/main\nfeature\tdef5678\t\t\n";
        let branches = parse_branch_list(output);
        assert_eq!(branches.len(), 2);
        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_current);
        assert_eq!(branches[0].commit.as_deref(), Some("abc1234"));
        assert_eq!(branches[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!(branches[1].name, "feature");
        assert!(!branches[1].is_current);
        assert_eq!(branches[1].upstream, None);
    }

    #[test]
    fn parses_empty_branch_listing() {
        assert!(parse_branch_list("").is_empty());
        assert!(parse_branch_list("\n").is_empty());
    }

    #[test]
    fn omitted_optionals_stay_out_of_the_payload() {
        let branches = parse_branch_list("feature\tdef5678\t\t\n");
        let value = serde_json::to_value(&branches[0]).unwrap();
        assert_eq!(value["name"], "feature");
        assert!(value.get("upstream").is_none(), "{value}");
    }
}
