//! Preflight assertions for destructive Git operations.
//!
//! Every history-rewriting tool checks these before acting so a dirty worktree,
//! a detached HEAD, or an unfinished operation cannot silently corrupt the
//! repository.

use std::path::{Path, PathBuf};

use crate::error::GitError;
use crate::git::Git;
use crate::services::inspect::get_status;

/// Fails when the worktree has uncommitted changes.
pub async fn assert_clean_worktree(repo_path: &Path) -> Result<(), GitError> {
    let status = get_status(repo_path).await?;
    if !status.is_clean {
        return Err(GitError::classified(
            "Working tree is not clean. Commit or stash changes before this operation.",
        ));
    }
    Ok(())
}

/// Fails when HEAD is not on a branch.
pub async fn assert_not_detached(repo_path: &Path) -> Result<(), GitError> {
    let git = Git::open(repo_path)?;
    if git.raw(&["symbolic-ref", "HEAD"]).await.is_err() {
        return Err(GitError::classified(
            "Repository is in detached HEAD state. Checkout a branch before this operation.",
        ));
    }
    Ok(())
}

/// Git state markers that indicate an unfinished operation.
const IN_PROGRESS_MARKERS: [&str; 5] = [
    "MERGE_HEAD",
    "rebase-merge",
    "rebase-apply",
    "CHERRY_PICK_HEAD",
    "BISECT_LOG",
];

/// Fails when a merge, rebase, cherry-pick or bisect is in progress.
pub async fn assert_no_in_progress_operation(repo_path: &Path) -> Result<(), GitError> {
    let git_dir = git_dir(repo_path).await?;

    for marker in IN_PROGRESS_MARKERS {
        if git_dir.join(marker).exists() {
            let operation = marker
                .replace("_HEAD", "")
                .replace("-merge", "")
                .replace("-apply", "");
            return Err(GitError::classified(format!(
                "A {operation} operation is in progress. Resolve or abort it first."
            )));
        }
    }

    Ok(())
}

/// Resolves the repository's git directory.
///
/// `--absolute-git-dir` is used rather than `--git-dir` so the result can be
/// joined with marker file names regardless of the server's working directory.
pub async fn git_dir(repo_path: &Path) -> Result<PathBuf, GitError> {
    let git = Git::open(repo_path)?;
    let output = git.raw(&["rev-parse", "--absolute-git-dir"]).await?;
    Ok(PathBuf::from(output.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@example.com"],
            vec!["config", "user.name", "T"],
        ] {
            let status = Command::new("git")
                .args(&args)
                .current_dir(dir.path())
                .status()
                .expect("git runs");
            assert!(status.success());
        }
        std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        let status = Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        dir
    }

    #[tokio::test]
    async fn clean_worktree_passes_and_dirty_fails() {
        let dir = fixture();
        assert_clean_worktree(dir.path()).await.unwrap();

        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        let error = assert_clean_worktree(dir.path()).await.unwrap_err();
        assert!(error.message().contains("Working tree is not clean"));
    }

    #[tokio::test]
    async fn detached_head_is_rejected() {
        let dir = fixture();
        assert_not_detached(dir.path()).await.unwrap();

        let status = Command::new("git")
            .args(["checkout", "--detach"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());

        let error = assert_not_detached(dir.path()).await.unwrap_err();
        assert!(error.message().contains("detached HEAD"));
    }

    #[tokio::test]
    async fn in_progress_marker_is_reported_with_its_operation_name() {
        let dir = fixture();
        assert_no_in_progress_operation(dir.path()).await.unwrap();

        let git_dir = git_dir(dir.path()).await.unwrap();
        std::fs::write(git_dir.join("MERGE_HEAD"), "deadbeef\n").unwrap();

        let error = assert_no_in_progress_operation(dir.path())
            .await
            .unwrap_err();
        assert_eq!(
            error.message(),
            "A MERGE operation is in progress. Resolve or abort it first."
        );

        std::fs::remove_file(git_dir.join("MERGE_HEAD")).unwrap();
        std::fs::create_dir(git_dir.join("rebase-merge")).unwrap();
        let error = assert_no_in_progress_operation(dir.path())
            .await
            .unwrap_err();
        assert!(error.message().starts_with("A rebase operation"));
    }
}
